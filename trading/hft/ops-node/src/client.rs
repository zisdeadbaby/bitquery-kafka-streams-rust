use crate::{
    benchmarks::{LatencyMonitor, MetricsExporter}, // Added MetricsExporter
    config::Config,
    error::{OpsNodeError, Result as OpsNodeResult},
    grpc_client::GrpcManager,
    memory::{MemoryPool, MarketDataPacket},
    state::{SharedStateManager, PositionManager, TokenInfoV1, PoolInfoV1, PositionV1, MarketStateV1}, // Versioned states
    strategies::{TradingStrategy, TokenOpportunity, SnipeExecutionParams, ExitParams, sniper::TokenSniperConfig}, // Added more strategy items
    tx_sender::TransactionSender,
};
use parking_lot::RwLock;
use solana_sdk::{
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    instruction::Instruction, // For building actual swap instructions
};
use spl_token::solana_program::system_instruction; // Example for SOL transfer if needed for swaps
use std::sync::Arc;
use std::time::Duration; // For intervals
use tokio::sync::{mpsc, Notify}; // Added Notify for shutdown coordination
use tokio::time::interval;


const MARKET_STATE_PERSIST_INTERVAL_SECS: u64 = 300; // Persist state every 5 minutes
const STRATEGY_EVALUATION_INTERVAL_MS: u64 = 500; // Evaluate strategy every 500ms
const POSITION_REEVALUATION_INTERVAL_MS: u64 = 100; // Re-evaluate open positions more frequently

pub struct OpsNodeClient {
    config: Arc<Config>,
    keypair: Arc<Keypair>, // Main operational keypair
    grpc_manager: Arc<GrpcManager>, // Removed RwLock, GrpcManager itself handles pooling
    tx_sender: Arc<TransactionSender>,
    state_manager: Arc<SharedStateManager>,
    position_manager: Arc<PositionManager>,
    memory_pool: Arc<MemoryPool>, // For custom allocations if needed by strategies or processing
    latency_monitor: Arc<LatencyMonitor>,
    metrics_exporter: Arc<MetricsExporter>, // For serving Prometheus metrics
    strategy: Arc<dyn TradingStrategy>,
    // shutdown_tx: mpsc::Sender<()>, // Using Notify for simpler broadcast shutdown
    shutdown_notify: Arc<Notify>, // Used to signal shutdown to tasks
}

impl OpsNodeClient {
    pub async fn new(
        config: Config,
        strategy: Arc<dyn TradingStrategy>, // Pass the initialized strategy
        shutdown_notify: Arc<Notify>,
    ) -> OpsNodeResult<Self> {
        // Validate config first
        config.validate().map_err(|e| OpsNodeError::Config(config::ConfigError::Message(format!("Config validation failed: {}", e))))?;

        let app_config = Arc::new(config); // Use Arc<Config> henceforth

        // Load keypair
        let keypair_bytes = std::fs::read(&app_config.auth.private_key_path)
            .map_err(|e| OpsNodeError::Config(config::ConfigError::Message(format!("Failed to read keypair file {:?}: {}", app_config.auth.private_key_path, e))))?;
        let keypair = Arc::new(Keypair::from_bytes(&keypair_bytes)
            .map_err(|e| OpsNodeError::Config(config::ConfigError::Message(format!("Invalid keypair file format: {}", e))))?);
        tracing::info!("Operational keypair loaded successfully: {}", keypair.pubkey());

        // Initialize components
        let metrics_exporter = Arc::new(MetricsExporter::new());
        let grpc_manager = Arc::new(GrpcManager::new(&app_config).await?);

        // For TransactionSender, if Jito is enabled, we need a Jito gRPC channel.
        // This assumes GrpcManager can provide one or it's created separately.
        // For now, passing None for Jito channel, TxSender will handle it gracefully.
        // TODO: Properly initialize Jito channel for TxSender if enabled.
        let tx_sender = Arc::new(TransactionSender::new(app_config.clone(), keypair.clone(), None).await?);

        let state_manager = Arc::new(SharedStateManager::new(None, Some(app_config.performance.memory_pool_size_mb as u64 / 4))?); // e.g. use 1/4 of mem pool for mmap
        let position_manager = Arc::new(PositionManager::new());
        let memory_pool = Arc::new(MemoryPool::new(
            app_config.performance.memory_pool_size_mb,
            app_config.performance.enable_huge_pages,
        ));
        let latency_monitor = Arc::new(LatencyMonitor::new(metrics_exporter.clone()));

        tracing::info!("OpsNodeClient components initialized.");

        Ok(Self {
            config: app_config,
            keypair,
            grpc_manager,
            tx_sender,
            state_manager,
            position_manager,
            memory_pool,
            latency_monitor,
            metrics_exporter,
            strategy,
            shutdown_notify,
        })
    }

    pub async fn run(&mut self) -> OpsNodeResult<()> {
        tracing::info!("OpsNodeClient running with strategy: {}", self.strategy.name());

        // Set thread affinity (for the main task pool, if not done per task)
        if !self.config.performance.cpu_cores.is_empty() {
            if let Err(e) = self.set_cpu_affinity_for_current_thread() {
                tracing::warn!("Failed to set CPU affinity for main client thread: {}. Performance may vary.", e);
            }
        }

        // Spawn core tasks
        let market_data_handle = self.spawn_market_data_listener_task();
        let slot_monitor_handle = self.spawn_slot_monitor_task();
        let strategy_executor_handle = self.spawn_strategy_evaluation_task();
        let position_manager_handle = self.spawn_position_management_task();
        let state_persister_handle = self.spawn_state_persistence_task();
        let metrics_server_handle = self.spawn_metrics_server_task();
        // Add more tasks as needed (e.g., UI, external API interactions)

        // Wait for shutdown signal or for any essential task to fail
        tokio::select! {
            _ = self.shutdown_notify.notified() => {
                tracing::info!("Shutdown signal received by OpsNodeClient run loop. Initiating graceful shutdown of tasks.");
            }
            res = market_data_handle => res?, // Propagate error if task failed
            res = slot_monitor_handle => res?,
            res = strategy_executor_handle => res?,
            res = position_manager_handle => res?,
            // Non-critical tasks might not need to stop the whole client if they error
            res = state_persister_handle => if let Err(e) = res? { tracing::error!("State persister task exited with error: {}", e); } ,
            res = metrics_server_handle => if let Err(e) = res? { tracing::error!("Metrics server task exited with error: {}", e); },
        }

        tracing::info!("OpsNodeClient run loop finished. All tasks should be terminating.");
        // Additional cleanup can be done here if necessary before OpsNodeClient is dropped.
        self.latency_monitor.print_all_stats_to_console();
        Ok(())
    }

    // Task to listen to market data (e.g., from Yellowstone)
    fn spawn_market_data_listener_task(&self) -> tokio::task::JoinHandle<OpsNodeResult<()>> {
        let grpc_mgr = self.grpc_manager.clone();
        let state_mgr = self.state_manager.clone();
        let latency_mon = self.latency_monitor.clone();
        let shutdown = self.shutdown_notify.clone();
        // Define accounts to monitor (example) - this should come from config or strategy
        let mut accounts_to_monitor = std::collections::HashMap::new();
        accounts_to_monitor.insert(
            "spl_token_program".to_string(),
            crate::grpc_client::proto::yellowstone::SubscribeRequestFilterAccounts {
                account: Vec::new(), // Monitor all if empty, or specify accounts
                owner: vec!["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string()], // SPL Token program
                ..Default::default()
            },
        );
        // Add more specific accounts or programs based on strategy needs (e.g., Raydium, Pump.fun)

        tokio::spawn(async move {
            let callback = move |packet: MarketDataPacket| {
                let _guard = latency_mon.start_tracking("process_market_data_packet");
                tracing::trace!("Received market data packet: Slot {}, TS {}, Key: ...", packet.slot, packet.timestamp_ns);
                // TODO: Detailed parsing of MarketDataPacket (which contains raw account data)
                // This is where you'd deserialize account data based on its type (e.g., Mint, TokenAccount, PoolState)
                // and then update TokenInfoV1 or PoolInfoV1 in SharedStateManager.
                // For now, just updating last processed slot based on packet.
                state_mgr.set_last_processed_slot(packet.slot);
            };

            // Loop to allow re-subscription on error
            loop {
                tokio::select! {
                    _ = shutdown.notified() => {
                        tracing::info!("Market data listener task shutting down.");
                        return Ok(());
                    }
                    res = grpc_mgr.subscribe_to_yellowstone_accounts(accounts_to_monitor.clone(), callback.clone()) => {
                        if let Err(e) = res {
                            tracing::error!("Failed to subscribe/maintain token accounts stream: {}. Retrying in 10s.", e);
                            tokio::time::sleep(Duration::from_secs(10)).await; // Wait before retrying
                        } else {
                            // If subscribe_to_yellowstone_accounts returns Ok, it means the spawning of the internal listener succeeded.
                            // The actual stream processing happens in a task spawned by subscribe_to_yellowstone_accounts.
                            // We need a way for *that* task to signal failure back here if it dies, or this loop will just re-call subscribe.
                            // For now, assume if subscribe itself returns Ok, the inner stream is running.
                            // A robust solution would involve the spawned stream task communicating its health.
                            tracing::info!("Market data subscription initiated. Inner task will handle stream.");
                            // If the inner stream task dies, this outer loop might not know unless subscribe_to_yellowstone_accounts is designed to block or signal.
                            // Assuming the current design of subscribe_to_yellowstone_accounts spawns and returns, we might need a mechanism
                            // to detect if that spawned task has died to re-initiate.
                            // For now, if it returns Ok, we just wait for shutdown.
                            shutdown.notified().await;
                             tracing::info!("Market data listener task (outer loop) shutting down due to global signal.");
                            return Ok(());
                        }
                    }
                }
            }
        })
    }

    // Task to monitor new slots
    fn spawn_slot_monitor_task(&self) -> tokio::task::JoinHandle<OpsNodeResult<()>> {
        let grpc_mgr = self.grpc_manager.clone();
        let latency_mon = self.latency_monitor.clone();
        let state_mgr = self.state_manager.clone(); // To update last seen slot
        let shutdown = self.shutdown_notify.clone();

        tokio::spawn(async move {
            let callback = move |slot: u64| {
                let _guard = latency_mon.start_tracking("process_slot_update");
                tracing::trace!("New slot confirmed: {}", slot);
                state_mgr.set_last_processed_slot(slot); // Update global last processed slot
            };

            loop {
                 tokio::select! {
                    _ = shutdown.notified() => {
                        tracing::info!("Slot monitor task shutting down.");
                        return Ok(());
                    }
                    res = grpc_mgr.subscribe_to_yellowstone_slots(callback.clone()) => {
                         if let Err(e) = res {
                            tracing::error!("Failed to subscribe/maintain slot stream: {}. Retrying in 10s.", e);
                            tokio::time::sleep(Duration::from_secs(10)).await;
                        } else {
                            tracing::info!("Slot subscription initiated. Inner task will handle stream.");
                            shutdown.notified().await; // Wait for global shutdown if subscribe call itself doesn't block on stream
                            tracing::info!("Slot monitor task (outer loop) shutting down due to global signal.");
                            return Ok(());
                        }
                    }
                }
            }
        })
    }

    // Task for strategy evaluation (finding new opportunities)
    fn spawn_strategy_evaluation_task(&self) -> tokio::task::JoinHandle<OpsNodeResult<()>> {
        let strategy = self.strategy.clone();
        let state_mgr = self.state_manager.clone();
        let tx_sender = self.tx_sender.clone();
        let pos_mgr = self.position_manager.clone();
        let latency_mon = self.latency_monitor.clone();
        let app_config = self.config.clone(); // Arc<Config>
        let keypair = self.keypair.clone();
        let shutdown = self.shutdown_notify.clone();

        tokio::spawn(async move {
            let mut eval_interval = interval(Duration::from_millis(STRATEGY_EVALUATION_INTERVAL_MS));
            loop {
                tokio::select! {
                    _ = eval_interval.tick() => {
                        let _guard = latency_mon.start_tracking("strategy_evaluation_cycle");
                        // Create a snapshot of current market state for this evaluation cycle
                        let market_state_snapshot = MarketStateV1 {
                            tokens: state_mgr.get_all_tokens().into_iter().map(|t| (t.mint, t)).collect(),
                            pools: state_mgr.get_all_pools().into_iter().map(|p| (p.address, p)).collect(),
                            last_processed_slot: state_mgr.get_last_processed_slot(),
                            last_persisted_timestamp_ns: 0, // Not relevant for live eval
                        };

                        // Iterate over all known tokens/pools or new events to find opportunities
                        // This is a simplified loop; a real system might use an event queue for new entities.
                        for token_info in market_state_snapshot.tokens.values() {
                            let pool_info_opt = token_info.pool_address.and_then(|pa| market_state_snapshot.pools.get(&pa));

                            match strategy.evaluate_new_entity(token_info, pool_info_opt, &market_state_snapshot, app_config.clone()).await {
                                Ok(Some(opportunity)) => {
                                    tracing::info!("Strategy {} identified opportunity: {:?}", strategy.name(), opportunity);
                                    // TODO: Check if position already exists for this token_mint via PositionManager
                                    if pos_mgr.get_active_position(&opportunity.token_mint).is_some() {
                                        tracing::debug!("Skipping opportunity for {}, position already active.", opportunity.token_mint);
                                        continue;
                                    }
                                    // Convert TokenOpportunity to SnipeExecutionParams
                                    // This logic might be part of the strategy or the client.
                                    let exec_params = convert_opportunity_to_execution_params(&opportunity, &app_config.trading);

                                    if let Err(e) = execute_snipe_trade(tx_sender.clone(), exec_params, keypair.clone(), pos_mgr.clone(), token_info.decimals, opportunity.clone()).await {
                                        tracing::error!("Failed to execute snipe trade for opportunity {:?}: {}", opportunity, e);
                                    }
                                }
                                Ok(None) => { /* No opportunity for this entity */ }
                                Err(e) => tracing::error!("Error evaluating entity {:?} with strategy {}: {}", token_info.mint, strategy.name(), e),
                            }
                        }
                    }
                    _ = shutdown.notified() => {
                        tracing::info!("Strategy evaluation task shutting down.");
                        return Ok(());
                    }
                }
            }
        })
    }

    // Task for managing existing open positions (checking for exits)
    fn spawn_position_management_task(&self) -> tokio::task::JoinHandle<OpsNodeResult<()>> {
        let strategy = self.strategy.clone();
        let state_mgr = self.state_manager.clone();
        let tx_sender = self.tx_sender.clone();
        let pos_mgr = self.position_manager.clone();
        let latency_mon = self.latency_monitor.clone();
        let app_config = self.config.clone();
        let keypair = self.keypair.clone();
        let shutdown = self.shutdown_notify.clone();

        tokio::spawn(async move {
            let mut re_eval_interval = interval(Duration::from_millis(POSITION_REEVALUATION_INTERVAL_MS));
            loop {
                tokio::select! {
                    _ = re_eval_interval.tick() => {
                        let _guard = latency_mon.start_tracking("position_management_cycle");
                        let active_positions = pos_mgr.get_all_active_positions();
                        if active_positions.is_empty() { continue; }

                        let market_state_snapshot = MarketStateV1 {
                            tokens: state_mgr.get_all_tokens().into_iter().map(|t| (t.mint, t)).collect(),
                            pools: state_mgr.get_all_pools().into_iter().map(|p| (p.address, p)).collect(),
                            last_processed_slot: state_mgr.get_last_processed_slot(),
                            last_persisted_timestamp_ns: 0,
                        };

                        for position in active_positions {
                            match strategy.re_evaluate_position(&position, &market_state_snapshot, app_config.clone()).await {
                                Ok(Some(exit_params)) => {
                                    tracing::info!("Strategy {} recommends exiting position {:?} due to: {}", strategy.name(), position.token_mint, exit_params.reason);
                                    let token_decimals = market_state_snapshot.tokens.get(&position.token_mint).map_or(0, |t| t.decimals);
                                    if let Err(e) = execute_exit_trade(tx_sender.clone(), &position, exit_params, keypair.clone(), pos_mgr.clone(), token_decimals).await {
                                        tracing::error!("Failed to execute exit trade for position {:?}: {}", position.token_mint, e);
                                    }
                                }
                                Ok(None) => { /* Hold position */ }
                                Err(e) => tracing::error!("Error re-evaluating position {:?} with strategy {}: {}",position.token_mint, strategy.name(), e),
                            }
                        }
                    }
                    _ = shutdown.notified() => {
                        tracing::info!("Position management task shutting down.");
                        return Ok(());
                    }
                }
            }
        })
    }

    // Task for periodically persisting shared state
    fn spawn_state_persistence_task(&self) -> tokio::task::JoinHandle<OpsNodeResult<()>> {
        let state_mgr = self.state_manager.clone();
        let latency_mon = self.latency_monitor.clone();
        let shutdown = self.shutdown_notify.clone();

        tokio::spawn(async move {
            let mut persist_interval = interval(Duration::from_secs(MARKET_STATE_PERSIST_INTERVAL_SECS));
            loop {
                tokio::select! {
                    _ = persist_interval.tick() => {
                        let _guard = latency_mon.start_tracking("state_persistence_cycle");
                        tracing::info!("Attempting to persist market state...");
                        if let Err(e) = state_mgr.persist_to_mmap() {
                            tracing::error!("Failed to persist market state: {}", e);
                        }
                    }
                    _ = shutdown.notified() => {
                        tracing::info!("State persistence task shutting down. Performing final persistence...");
                        if let Err(e) = state_mgr.persist_to_mmap() {
                            tracing::error!("Failed to perform final market state persistence: {}", e);
                        }
                        return Ok(());
                    }
                }
            }
        })
    }

    // Task for Prometheus metrics server
    fn spawn_metrics_server_task(&self) -> tokio::task::JoinHandle<OpsNodeResult<()>> {
        let metrics_exp = self.metrics_exporter.clone();
        let port = self.config.monitoring.metrics_port;
        let shutdown = self.shutdown_notify.clone(); // For graceful shutdown of warp server

        tokio::spawn(async move {
            let metrics_route = warp::path("metrics").map(move || {
                metrics_exp.gather_metrics_text()
            });

            let (_addr, server) = warp::serve(metrics_route)
                .try_bind_with_graceful_shutdown(([0,0,0,0], port), async move {
                    shutdown.notified().await;
                })
                .map_err(|e| OpsNodeError::Network(std::io::Error::new(std::io::ErrorKind::Other, format!("Metrics server bind error: {}", e))))?;

            tracing::info!("Metrics server started on port {}", port);
            server.await;
            tracing::info!("Metrics server shut down.");
            Ok(())
        })
    }

    // Sets CPU affinity for the current thread, if configured.
    fn set_cpu_affinity_for_current_thread(&self) -> OpsNodeResult<()> {
        if self.config.performance.cpu_cores.is_empty() {
            return Ok(());
        }

        #[cfg(target_os = "linux")]
        {
            use libc::{cpu_set_t, CPU_SET, CPU_ZERO, sched_setaffinity};
            use std::mem;

            let mut cpu_set: cpu_set_t = unsafe { mem::zeroed() };
            unsafe { CPU_ZERO(&mut cpu_set) };

            for &core_id in &self.config.performance.cpu_cores {
                if core_id >= num_cpus::get() { // Check core_id validity
                    tracing::warn!("Configured CPU core ID {} is out of range (max {}). Skipping.", core_id, num_cpus::get() -1);
                    continue;
                }
                unsafe { CPU_SET(core_id, &mut cpu_set) };
            }

            // Apply to current thread (pthread_self() which is 0 for libc's sched_setaffinity)
            let result = unsafe { sched_setaffinity(0, mem::size_of::<cpu_set_t>(), &cpu_set) };

            if result != 0 {
                let err = std::io::Error::last_os_error();
                tracing::error!("Failed to set CPU affinity for current thread: {}", err);
                return Err(OpsNodeError::Network(err)); // Reusing Network error for OS errors
            }
            tracing::info!("Successfully set CPU affinity for current thread to cores: {:?}", self.config.performance.cpu_cores);
        }
        #[cfg(not(target_os = "linux"))]
        {
            tracing::warn!("CPU affinity setting is only supported on Linux. Ignored on this platform.");
        }
        Ok(())
    }
}

// Helper to convert opportunity to execution parameters
fn convert_opportunity_to_execution_params(opportunity: &TokenOpportunity, trading_config: &crate::config::TradingConfig) -> SnipeExecutionParams {
    // TODO: Implement proper slippage calculation for min_amount_out_token
    // For now, using a fixed percentage (e.g., 1% slippage from recommended size, if price is known)
    // This needs the current price of token_to_buy_mint in terms of quote.
    // Let's assume recommended_trade_size_quote is the amount of quote to spend.
    // min_amount_out_token would be (recommended_trade_size_quote / price) * (1 - slippage_bps/10000)
    // This is a placeholder.
    let slippage_bps = trading_config.max_slippage_bps; // Use configured slippage
    let min_amount_out_token_placeholder = 0; // Needs actual calculation based on price and slippage

    SnipeExecutionParams {
        token_to_buy_mint: opportunity.token_mint,
        pool_address: opportunity.associated_pool,
        amount_in_quote: opportunity.recommended_trade_size_quote,
        min_amount_out_token: min_amount_out_token_placeholder, // CRITICAL: Calculate this properly
        max_priority_fee_lamports: trading_config.priority_fee_cap, // Use global or opportunity-specific
        use_jito_bundle: trading_config.enable_jito_bundles, // Or opportunity-specific
    }
}

// Placeholder for actual snipe execution logic
async fn execute_snipe_trade(
    tx_sender: Arc<TransactionSender>,
    params: SnipeExecutionParams,
    payer_keypair: Arc<Keypair>,
    position_manager: Arc<PositionManager>,
    token_decimals: u8, // Decimals of the token being bought
    opportunity: TokenOpportunity, // Pass opportunity for recording position details
) -> OpsNodeResult<()> {
    tracing::info!("Attempting to execute snipe: {:?}", params);

    // 1. TODO: Construct actual swap instructions (e.g., for Raydium, Orca, or Pump.fun)
    // This is highly dependent on the target DEX/platform.
    // Example: If it's a simple SOL transfer for a pump.fun buy:
    // let sol_transfer_ix = system_instruction::transfer(
    //     &payer_keypair.pubkey(),
    //     &params.pool_address, // Assuming pool_address is the recipient for pump.fun
    //     params.amount_in_quote, // Amount of SOL (lamports)
    // );
    // let instructions = vec![sol_transfer_ix];

    // For a real DEX swap, you'd need:
    // - The DEX program ID.
    // - Correct instruction data for the swap.
    // - All required account metas (pool state, token accounts, authorities, etc.).
    // - Potentially pre-create ATA for the token being bought.
    if params.min_amount_out_token == 0 {
        tracing::warn!("min_amount_out_token is 0 for snipe {:?}. This is risky. Ensure it's calculated correctly.", params);
        // return Err(OpsNodeError::Strategy("min_amount_out_token cannot be 0".to_string()));
    }

    let instructions: Vec<Instruction> = vec![]; // <<<< IMPORTANT: Replace with actual swap instructions
    if instructions.is_empty() {
        tracing::warn!("No instructions provided for snipe trade {:?}. Skipping execution.", params);
        return Err(OpsNodeError::Strategy("No swap instructions generated for snipe.".to_string()));
    }

    // 2. Send transaction
    let signature = tx_sender.send_optimized_transaction(instructions, &[], None /* LUTs if any */).await?;
    tracing::info!("Snipe transaction {} sent for token {} (pool {})", signature, params.token_to_buy_mint, params.pool_address);

    // 3. Record position (optimistically, or after confirmation)
    // For HFT, often record optimistically and then update status.
    // Entry price needs to be estimated or fetched post-confirmation if not part of params.
    // For now, using a placeholder entry price.
    let entry_price_fp_placeholder = 0; // TODO: Estimate from pool reserves or execution report

    let position = PositionV1 {
        token_mint: params.token_to_buy_mint.to_bytes(),
        entry_price_fp: entry_price_fp_placeholder,
        quantity_tokens: params.min_amount_out_token, // Optimistically assume min_amount_out if tx confirms
        entry_slot: opportunity.details.as_ref().and_then(|d| d.split_whitespace().last().unwrap_or_default().parse().ok()).unwrap_or(0), // approx slot from opportunity
        entry_timestamp_ns: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64,
        strategy_id: "TokenSniperV1".to_string(), // Or get from strategy instance
    };
    if let Err(e) = position_manager.open_position(position.clone()) {
         tracing::error!("Failed to open position after snipe tx {} for token {}: {}", signature, params.token_to_buy_mint, e);
         // Consider if this requires unwinding or special handling
    } else {
        tracing::info!("Opened position for token {} after snipe tx {}: {:?}", params.token_to_buy_mint, signature, position);
    }

    // TODO: Spawn a task to monitor this transaction's confirmation and update position details (actual quantity bought, entry price).
    Ok(())
}

// Placeholder for actual exit execution logic
async fn execute_exit_trade(
    tx_sender: Arc<TransactionSender>,
    position_to_exit: &PositionV1,
    exit_params: ExitParams,
    payer_keypair: Arc<Keypair>,
    position_manager: Arc<PositionManager>,
    token_decimals: u8, // Decimals of the token being sold
) -> OpsNodeResult<()> {
    let token_mint_pk = Pubkey::new_from_array(position_to_exit.token_mint);
    tracing::info!("Attempting to execute exit for token {}: Reason '{}', Min Quote Out: {}", token_mint_pk, exit_params.reason, exit_params.min_amount_out_quote);

    // 1. TODO: Construct actual swap instructions to sell `position_to_exit.quantity_tokens`
    // This is similar to snipe, but selling the token for quote currency.
    // Ensure `exit_params.min_amount_out_quote` is used for slippage.
    let instructions: Vec<Instruction> = vec![]; // <<<< IMPORTANT: Replace with actual swap instructions
     if instructions.is_empty() {
        tracing::warn!("No instructions provided for exit trade of {}. Skipping execution.", token_mint_pk);
        return Err(OpsNodeError::Strategy("No swap instructions generated for exit.".to_string()));
    }
    if exit_params.min_amount_out_quote == 0 && exit_params.reason != "emergency_exit_market_order" { // Example condition
        tracing::warn!("min_amount_out_quote is 0 for exit of {}. This is risky unless it's a market order. Ensure calculated correctly.", token_mint_pk);
    }


    // 2. Send transaction
    let signature = tx_sender.send_optimized_transaction(instructions, &[], None).await?;
    tracing::info!("Exit transaction {} sent for token {}", signature, token_mint_pk);

    // 3. Close position in manager (optimistically, or after confirmation)
    // Exit price needs to be estimated or fetched post-confirmation.
    let exit_price_fp_placeholder = 0; // TODO: Estimate from pool reserves or execution report

    if let Some(completed_trade) = position_manager.close_position(&token_mint_pk, exit_price_fp_placeholder, exit_params.reason.clone()) {
        tracing::info!("Closed position for token {} via tx {}, Trade: {:?}", token_mint_pk, signature, completed_trade);
    } else {
        tracing::warn!("Failed to find active position for {} to close after exit tx {}, or already closed.", token_mint_pk, signature);
    }

    // TODO: Spawn a task to monitor this transaction's confirmation and update completed trade details (actual quote received, exit price).
    Ok(())
}
