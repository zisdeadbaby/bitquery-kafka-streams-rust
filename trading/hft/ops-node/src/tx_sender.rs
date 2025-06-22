use crate::config::Config;
use crate::error::{OpsNodeError, Result};
use crate::grpc_client::proto::jito::{Packet as JitoPacket, PacketBatch as JitoPacketBatch, shred_stream_client::ShredStreamClient as JitoShredStreamClient}; // For Jito integration
use std::collections::HashMap;
use std::sync::RwLock;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_client::RpcClient as BlockingRpcClient; // For operations that might be better blocking
use solana_client::rpc_config::{RpcSendTransactionConfig, RpcSimulateTransactionConfig}; // For simulate
use solana_client::rpc_response::RpcSimulateTransactionResult; // For simulation
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    compute_budget::ComputeBudgetInstruction,
    hash::Hash,
    instruction::Instruction,
    message::{v0::Message as MessageV0, VersionedMessage},
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::{TransactionError, VersionedTransaction}, // Added TransactionError
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tonic::transport::Channel as TonicChannel; // For Jito client

const RECENT_PRIORITY_FEES_ACCOUNTS_LIMIT: usize = 128; // Solana limit for getRecentPriorityFees
#[allow(dead_code)]
const DEFAULT_COMPUTE_UNITS: u32 = 200_000; // Fallback compute units
const MAX_COMPUTE_UNITS: u32 = 1_400_000; // Max allowable compute units
const COMPUTE_UNIT_ESTIMATION_BUFFER_PERCENT: f64 = 0.15; // 15% buffer for simulation
#[allow(dead_code)]
const JITO_TIP_ACCOUNTS: [&str; 3] = [ // Jito tip accounts
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGq5deL4ccVNZGSFV",
    "HFqU5x63VTqvQss8hp11i4wVV8tb4LsUajnpVa㈜FH4", // Typo in original, assuming JitoPayer
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
];


pub struct TransactionSender {
    rpc_client: Arc<RpcClient>, // Async RPC client
    blocking_rpc_client: Arc<BlockingRpcClient>, // Sync RPC client for specific tasks like simulate
    jito_client: Option<Arc<JitoShredStreamClient<TonicChannel>>>, // Optional Jito client
    priority_optimizer: PriorityFeeOptimizer,
    compute_unit_cache: Arc<RwLock<HashMap<String, u32>>>, // Cache key: sorted program IDs string
    rate_limiter: Arc<Semaphore>,
    config: Arc<Config>,
    keypair: Arc<Keypair>, // Payer keypair
}

impl TransactionSender {
    pub async fn new(config: Arc<Config>, keypair: Arc<Keypair>, jito_channel: Option<TonicChannel>) -> Result<Self> {
        let rpc_client = Arc::new(RpcClient::new_with_timeout_and_commitment(
            config.network.rpc_endpoint.clone(),
            Duration::from_millis(config.network.request_timeout_ms),
            CommitmentConfig::confirmed(), // Use confirmed for most operations, can be overridden
        ));

        let blocking_rpc_client = Arc::new(BlockingRpcClient::new_with_timeout_and_commitment(
            config.network.rpc_endpoint.clone(),
            Duration::from_millis(config.network.request_timeout_ms * 2), // Longer timeout for potentially slower sync calls
            CommitmentConfig::confirmed(),
        ));

        let jito_client = if config.trading.enable_jito_bundles {
            jito_channel.map(|channel| Arc::new(JitoShredStreamClient::new(channel)))
        } else {
            None
        };
        if jito_client.is_some() {
            tracing::info!("Jito bundle submission enabled.");
        } else if config.trading.enable_jito_bundles {
            tracing::warn!("Jito bundle submission is enabled in config, but no Jito channel was provided. Jito submission will be skipped.");
        }

        let priority_optimizer = PriorityFeeOptimizer::new(rpc_client.clone(), config.clone());
        let compute_unit_cache = Arc::new(RwLock::new(HashMap::new()));
        // Max concurrent *pending* operations to Solana, not just submissions
        let rate_limiter = Arc::new(Semaphore::new(config.performance.max_concurrent_operations));

        Ok(Self {
            rpc_client,
            blocking_rpc_client,
            jito_client,
            priority_optimizer,
            compute_unit_cache,
            rate_limiter,
            config,
            keypair,
        })
    }

    pub async fn send_optimized_transaction(
        &self,
        instructions: Vec<Instruction>,
        // Payer is now part of TransactionSender struct
        additional_signers: &[&Keypair], // Signers other than the main payer
        lookup_table_accounts: Option<Vec<solana_sdk::message::AddressLookupTableAccount>>, // Optional LUTs
    ) -> Result<Signature> {
        let _permit = self.rate_limiter.acquire().await.map_err(|e| OpsNodeError::BuildError(format!("Rate limiter permit acquisition failed: {}",e)))?;
        let start_time = Instant::now();

        let payer_pubkey = self.keypair.pubkey();

        // 1. Estimate Compute Units (using simulation)
        let (estimated_cus, recent_blockhash) = self.estimate_compute_units_with_simulation(&instructions, &payer_pubkey, additional_signers, lookup_table_accounts.as_deref()).await?;
        let compute_unit_limit = ComputeBudgetInstruction::set_compute_unit_limit(estimated_cus.min(MAX_COMPUTE_UNITS));

        // 2. Get Optimal Priority Fee
        let write_accounts: Vec<Pubkey> = instructions
            .iter()
            .flat_map(|ix| ix.accounts.iter().filter(|a| a.is_writable).map(|a| a.pubkey))
            .collect();

        let priority_fee_lamports = self.priority_optimizer
            .calculate_optimal_fee(&write_accounts, estimated_cus)
            .await?;
        let compute_unit_price = ComputeBudgetInstruction::set_compute_unit_price(priority_fee_lamports);

        // 3. Build Transaction
        let mut final_instructions = vec![compute_unit_limit, compute_unit_price];
        final_instructions.extend(instructions);

        let message = if let Some(luts) = lookup_table_accounts {
            MessageV0::try_compile(&payer_pubkey, &final_instructions, &luts, recent_blockhash)
        } else {
            MessageV0::try_compile(&payer_pubkey, &final_instructions, &[], recent_blockhash)
        }.map_err(|e| OpsNodeError::SolanaClient(solana_client::client_error::ClientError::from(
            solana_client::client_error::ClientErrorKind::Custom(format!("Message compilation failed: {}", e))
        )))?;

        let mut signers_vec = vec![self.keypair.as_ref()];
        signers_vec.extend(additional_signers);

        let versioned_tx = VersionedTransaction::try_new(VersionedMessage::V0(message), &signers_vec)
            .map_err(|e| OpsNodeError::SolanaClient(solana_client::client_error::ClientError::from(
                solana_client::client_error::ClientErrorKind::SigningError(e)
            )))?;

        // 4. Multi-path Submission (RPC and Jito if enabled)
        let rpc_send_config = RpcSendTransactionConfig {
            skip_preflight: true, // Already simulated
            preflight_commitment: Some(CommitmentLevel::Confirmed), // Or lower like processed
            max_retries: Some(0), // Do not retry at RPC level, handle retry logic outside if needed
            ..Default::default()
        };

        let jito_future_opt = if self.config.trading.enable_jito_bundles && self.jito_client.is_some() {
            Some(self.send_via_jito(versioned_tx.clone(), priority_fee_lamports))
        } else {
            None
        };

        // Race for the first successful submission confirmation (or just submission if not waiting for confirm)
        let signature;
        if let Some(jito_future) = jito_future_opt {
            tokio::select! {
                biased; // Prioritize Jito if both complete around the same time
                jito_res = jito_future => {
                    match jito_res {
                        Ok(sig_str) => {
                            tracing::info!("Jito submission successful (bundle ID): {}", sig_str);
                            // For Jito, the returned string is often a bundle ID, not a tx signature immediately.
                            // We might need to convert or handle this. For now, assume it's a trackable ID.
                            // Let's assume the first transaction's signature in the bundle is what we track.
                            signature = versioned_tx.signatures[0];
                        },
                        Err(e) => {
                            tracing::warn!("Jito submission failed: {:?}. Falling back to RPC.", e);
                            signature = self.send_via_rpc(versioned_tx.clone(), rpc_send_config).await?; // Create a new future
                        }
                    }
                },
                rpc_res = self.send_via_rpc(versioned_tx.clone(), rpc_send_config) => {
                    signature = rpc_res?;
                    tracing::info!("RPC submission successful: {}", signature);
                }
            }
        } else {
            signature = self.send_via_rpc(versioned_tx.clone(), rpc_send_config).await?;
            tracing::info!("RPC submission successful (Jito disabled/unavailable): {}", signature);
        }

        let elapsed_ms = start_time.elapsed().as_millis();
        tracing::info!(
            "Transaction processed in {}ms. Signature/ID: {}, Priority Fee: {} lamports, CUs: {}",
            elapsed_ms, signature, priority_fee_lamports, estimated_cus
        );

        Ok(signature)
    }

    async fn send_via_rpc(&self, tx: VersionedTransaction, config: RpcSendTransactionConfig) -> Result<Signature> {
        // Using send_transaction_async for non-blocking send without immediate confirmation
        let signature = self.rpc_client
            .send_transaction_with_config(&tx, config)
            .await
            .map_err(|e| {
                tracing::error!("RPC send_transaction error: {}", e);
                OpsNodeError::from(e)
            })?;
        Ok(signature)
    }

    async fn send_via_jito(&self, tx: VersionedTransaction, tip_lamports: u64) -> Result<String> {
        if let Some(jito_client_arc) = &self.jito_client {
            let mut jito_client = jito_client_arc.as_ref().clone(); // Clone the client for mutable use

            let serialized_tx = bincode::serialize(&tx)
                .map_err(|e| OpsNodeError::BuildError(format!("Failed to serialize tx for Jito: {}", e)))?;

            let jito_packet = JitoPacket {
                data: serialized_tx,
                meta: None, // Populate if needed, e.g., for priority
            };

            // Jito expects a PacketBatch even for a single transaction
            let packet_batch = JitoPacketBatch {
                packets: vec![jito_packet],
                header: None, // Populate if needed
            };

            // Jito tip instruction (optional, but good for priority)
            // This should ideally be part of the transaction itself if possible,
            // but some Jito relays might accept it as metadata or separate.
            // The common way is to add a specific Jito tip instruction to the transaction.
            // If `tip_lamports` is from priority fee, it's already in the tx.
            // If it's an *additional* Jito-specific tip:
            if self.config.trading.tip_amount_lamports > 0 {
                // This part is tricky. Jito bundles usually involve sending SOL to one of their tip accounts.
                // This instruction should be part of the `tx` itself.
                // The `tip_lamports` parameter here might be redundant if priority_fee_lamports is already set.
                // For now, we assume the tip is handled by the priority fee or already in `tx`.
                tracing::info!("Jito bundle tip (from priority fee): {} lamports", tip_lamports);
            }

            // This is a simplified call. Jito's ShredStream might have different methods
            // like `send_bundle` or require a stream.
            // Assuming `broadcast_packet_batch` is a placeholder for the correct Jito API method.
            // The actual Jito gRPC API for sending bundles should be used.
            // For now, let's assume there's a method like `send_bundle`.
            // The `shredstream.proto` doesn't define a `send_bundle` directly.
            // It has `BroadcastPacketBatch` which might be for sending raw packets.
            // Jito's Block Engine gRPC is typically used for bundles.
            // This part needs the correct Jito gRPC client and method for bundles.
            // Placeholder:
            // let response = jito_client.send_bundle(Request::new(bundle_request)).await?;
            // For now, using BroadcastPacketBatch as a stand-in, this will likely need correction
            // based on the actual Jito gRPC service definition for bundles.
            match jito_client.broadcast_packet_batch(tonic::Request::new(packet_batch)).await {
                Ok(response) => {
                    // The response from Jito for bundle submission needs to be parsed.
                    // It often returns a list of bundle IDs or transaction statuses.
                    // For simplicity, if it's a string response, we return it.
                    // This is highly dependent on the actual Jito API.
                    tracing::debug!("Jito broadcast_packet_batch response: {:?}", response.into_inner());
                    Ok(format!("jito_bundle_submitted_{}", tx.signatures[0])) // Placeholder response
                }
                Err(status) => {
                    tracing::error!("Jito broadcast_packet_batch failed: {}", status);
                    Err(OpsNodeError::Grpc(status))
                }
            }

        } else {
            Err(OpsNodeError::BuildError("Jito client not available".to_string()))
        }
    }

    async fn estimate_compute_units_with_simulation(
        &self,
        instructions: &[Instruction],
        payer: &Pubkey,
        additional_signers: &[&Keypair],
        lookup_table_accounts: Option<&[solana_sdk::message::AddressLookupTableAccount]>,
    ) -> Result<(u32, Hash)> {
        let cache_key = Self::generate_instruction_cache_key(instructions);
        
        // Check cache first, without holding lock across await
        let cached_units = {
            self.compute_unit_cache.read().unwrap().get(&cache_key).copied()
        };
        
        if let Some(cached_units) = cached_units {
            if cached_units > 0 {
                 // Need a recent blockhash anyway
                 let recent_blockhash = self.rpc_client.get_latest_blockhash().await
                    .map_err(|e| OpsNodeError::SolanaClient(e))?;
                tracing::debug!("Using cached CUs: {} for key: {}", cached_units, cache_key);
                return Ok((cached_units, recent_blockhash));
            }
        }

        // Build a temporary transaction for simulation
        // Use a recent blockhash for simulation
        let (recent_blockhash, _last_valid_slot) = self.rpc_client
            .get_latest_blockhash_with_commitment(self.rpc_client.commitment())
            .await.map_err(OpsNodeError::SolanaClient)?;

        // Add a placeholder CU limit for simulation, it will be replaced by actual estimation
        let temp_cu_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(MAX_COMPUTE_UNITS); // Simulate with max possible
        let temp_priority_fee_ix = ComputeBudgetInstruction::set_compute_unit_price(0); // No fee for simulation itself

        let mut sim_instructions = vec![temp_cu_limit_ix, temp_priority_fee_ix];
        sim_instructions.extend_from_slice(instructions);

        let message = if let Some(luts) = lookup_table_accounts {
             MessageV0::try_compile(payer, &sim_instructions, luts, recent_blockhash)
        } else {
            MessageV0::try_compile(payer, &sim_instructions, &[], recent_blockhash)
        }.map_err(|e| OpsNodeError::BuildError(format!("Simulation message compile error: {}",e)))?;

        let mut sim_signers = vec![self.keypair.as_ref()]; // Payer must sign
        sim_signers.extend(additional_signers);

        let sim_tx = VersionedTransaction::try_new(VersionedMessage::V0(message), &sim_signers)
            .map_err(|e| OpsNodeError::BuildError(format!("Simulation tx build error: {}",e)))?;

        let sim_config = RpcSimulateTransactionConfig {
            sig_verify: false, // Signatures are not checked by simulation
            replace_recent_blockhash: false, // We're using a fresh blockhash
            commitment: Some(self.rpc_client.commitment()),
            encoding: Some(solana_transaction_status::UiTransactionEncoding::Base64),
            accounts: None, // Not overriding accounts for simulation
            min_context_slot: None, // Added
            inner_instructions: false, // Don't need inner instructions for simulation
        };

        // Use blocking client for simulation as it can be slow and we want the result before proceeding
        let sim_result = self.blocking_rpc_client.simulate_transaction_with_config(&sim_tx, sim_config)
            .map_err(|e| {
                tracing::warn!("Simulation RPC call failed for key {}: {}", cache_key, e);
                OpsNodeError::SolanaClient(e)
            })?;

        match sim_result.value {
            RpcSimulateTransactionResult { err: Some(tx_err), logs: Some(logs), accounts: _, units_consumed: _, return_data: _, inner_instructions: _, replacement_blockhash: _ } => {
                tracing::warn!("Simulation failed for key {}: {:?}. Logs: {:?}", cache_key, tx_err, logs.join("\n"));
                // If simulation fails, we can't reliably estimate CUs. Fallback or error.
                // Check for specific errors that might still allow CU estimation or indicate a permanent issue.
                if tx_err == TransactionError::WouldExceedMaxBlockCostLimit || tx_err.to_string().contains("would exceed block cost limit") {
                     tracing::warn!("Simulation indicates transaction would exceed max block cost limit. Capping at MAX_COMPUTE_UNITS.");
                     self.compute_unit_cache.write().unwrap().insert(cache_key, MAX_COMPUTE_UNITS);
                     return Ok((MAX_COMPUTE_UNITS, recent_blockhash));
                }
                // For other errors, it's safer to assume a higher CU or error out.
                // Returning a default or max here might lead to failed transactions if it's an actual program error.
                // Let's return an error that can be handled upstream.
                return Err(OpsNodeError::Strategy(format!("Transaction simulation failed: {:?}. Logs: {:?}", tx_err, logs)));
            }
            RpcSimulateTransactionResult { err: None, units_consumed: Some(consumed_units), logs: _, accounts: _, return_data: _, inner_instructions: _, replacement_blockhash: _ } => {
                let estimated_cus = (consumed_units as f64 * (1.0 + COMPUTE_UNIT_ESTIMATION_BUFFER_PERCENT)) as u32;
                tracing::debug!("Simulation successful for key {}. Consumed CUs: {}, Estimated with buffer: {}", cache_key, consumed_units, estimated_cus);
                self.compute_unit_cache.write().unwrap().insert(cache_key.clone(), estimated_cus);
                Ok((estimated_cus, recent_blockhash))
            }
            _ => {
                tracing::warn!("Unexpected simulation response for key {}: {:?}", cache_key, sim_result.value);
                Err(OpsNodeError::SolanaClient(solana_client::client_error::ClientError::from(
                    solana_client::client_error::ClientErrorKind::Custom("Invalid simulation response".to_string())
                )))
            }
        }
    }

    fn generate_instruction_cache_key(instructions: &[Instruction]) -> String {
        let mut program_ids: Vec<String> = instructions.iter().map(|ix| ix.program_id.to_string()).collect();
        program_ids.sort_unstable(); // Sort for consistent key
        program_ids.join(",")
    }
}


pub struct PriorityFeeOptimizer {
    rpc_client: Arc<RpcClient>,
    config: Arc<Config>, // Access to trading config like priority_fee_cap
    // Historical fees can be complex to manage; for now, rely on recent RPC data per call.
    // historical_fees: Arc<RwLock<VecDeque<u64>>>, // Store limited history
    // last_fetch_time: Arc<RwLock<Instant>>,
}

impl PriorityFeeOptimizer {
    // const FEE_HISTORY_LIMIT: usize = 100; // Max items in historical_fees
    // const FETCH_INTERVAL: Duration = Duration::from_secs(5); // How often to refresh full fee data

    pub fn new(rpc_client: Arc<RpcClient>, config: Arc<Config>) -> Self {
        Self {
            rpc_client,
            config,
            // historical_fees: Arc::new(RwLock::new(VecDeque::with_capacity(Self::FEE_HISTORY_LIMIT))),
            // last_fetch_time: Arc::new(RwLock::new(Instant::now() - Self::FETCH_INTERVAL * 2)), // Force initial fetch
        }
    }

    pub async fn calculate_optimal_fee(&self, accounts: &[Pubkey], _estimated_cus: u32) -> Result<u64> {
        // Get recent priority fees from RPC
        // Limit the number of accounts to avoid hitting RPC limits.
        // Prioritize accounts that are part of the transaction.
        let fee_accounts = if accounts.len() > RECENT_PRIORITY_FEES_ACCOUNTS_LIMIT {
            accounts[..RECENT_PRIORITY_FEES_ACCOUNTS_LIMIT].to_vec()
        } else {
            accounts.to_vec()
        };

        let rpc_priority_fees = self.rpc_client
            .get_recent_prioritization_fees(&fee_accounts)
            .await
            .map_err(|e| {
                tracing::warn!("Failed to get recent priority fees: {}", e);
                // Fallback or error. For now, use a default.
                OpsNodeError::SolanaClient(e)
            })?;

        if rpc_priority_fees.is_empty() {
            tracing::warn!("No priority fee data returned from RPC for accounts: {:?}. Using default.", fee_accounts);
            // Fallback to a default or a config-defined minimum.
            // This might happen if none of the provided accounts have recent fees.
            // Consider fetching global fees if account-specific fees are empty.
            return Ok(self.config.trading.priority_fee_cap.min(1000)); // Default 1000 microLamports if empty
        }

        // Aggregate fees: use the maximum `prioritization_fee` from the response.
        // Other strategies: average, median, percentile of fees.
        // Jito's recommendation is often to look at P50 or P75 of recent fees for similar transactions.
        let mut fees: Vec<u64> = rpc_priority_fees.iter().map(|fee| fee.prioritization_fee).collect();
        fees.sort_unstable();

        // Using P75 as a common strategy, but can be made configurable
        let p75_fee = fees.get( (fees.len() as f64 * 0.75) as usize ).copied().unwrap_or_else(|| {
            fees.last().copied().unwrap_or(self.config.trading.priority_fee_cap.min(5000)) // Fallback if percentile calc fails
        });

        let optimal_fee = p75_fee.min(self.config.trading.priority_fee_cap); // Cap at configured max

        tracing::debug!(
            "Priority fee calculation: Queried {} accounts, got {} fee samples. Fees: {:?}. P75 fee: {}. Optimal fee capped at: {}",
            fee_accounts.len(), fees.len(), fees, p75_fee, optimal_fee
        );

        Ok(optimal_fee.max(1)) // Ensure at least 1 microLamport
    }
}

// MarketCondition enum was part of the original snippet but not used in the simplified PFO.
// It can be reintroduced if more complex fee logic is desired.
// #[derive(Debug, Clone, Copy)]
// enum MarketCondition {
//     Low,
//     Medium,
//     High,
//     Critical,
// }
