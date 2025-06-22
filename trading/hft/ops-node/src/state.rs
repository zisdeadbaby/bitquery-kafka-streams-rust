use std::collections::HashMap; // Changed from AHashMap to HashMap
use memmap2::{MmapMut, MmapOptions};
use std::sync::RwLock; // Use std::sync::RwLock instead of parking_lot for Send compatibility
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize, Infallible}; // Aliased for clarity
use solana_sdk::pubkey::Pubkey;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf}; // For path manipulation
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH, Instant}; // For timestamps

const DEFAULT_MMAP_SIZE_MB: u64 = 64; // Increased default size for market state
const MMAP_FILE_NAME: &str = "ops_node_market_state.mmap";

// It's good practice to version your Rkyv state struct
// if you anticipate schema changes in the future.
#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)] // Enable check_bytes for validation
#[archive_attr(derive(Debug))] // Derive Debug for Archived types as well
pub struct TokenInfoV1 { // Added V1 for versioning
    pub mint: [u8; 32],
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
    pub total_supply: u64,
    pub created_slot: u64,
    pub pool_address: Option<[u8; 32]>, // e.g., Raydium LP mint or AMM ID
    // Add other relevant fields: authority, freeze_authority, etc.
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug, Clone)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct PoolInfoV1 { // Added V1
    pub address: [u8; 32], // Pool's own address/ID
    pub token_a_mint: [u8; 32], // Mint of token A
    pub token_b_mint: [u8; 32], // Mint of token B
    pub reserve_a: u64, // Amount of token A in pool
    pub reserve_b: u64, // Amount of token B in pool
    pub fee_bps: u16,   // Fee in basis points
    pub volume_24h_usd: Option<f64>, // Optional: 24h volume in USD
    pub last_update_slot: u64,
    // Add other relevant fields: pool type (e.g., Raydium, Orca), specific params
}

#[derive(Archive, RkyvDeserialize, RkyvSerialize, Debug)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct MarketStateV1 { // Added V1
    pub tokens: HashMap<[u8; 32], TokenInfoV1>, // Use HashMap for faster lookups by mint
    pub pools: HashMap<[u8; 32], PoolInfoV1>,   // Use HashMap for faster lookups by pool address
    pub last_processed_slot: u64, // Slot up to which data is consistent
    pub last_persisted_timestamp_ns: u64, // Nanosecond timestamp of last persist
}

pub struct SharedStateManager {
    mmap_file_path: PathBuf,
    mmap: Arc<RwLock<MmapMut>>,
    // In-memory caches for quick access. These are primary if mmap is for persistence/backup.
    // If mmap is the source of truth, these might be read-through caches.
    // Current design implies these caches are primary and mmap is for backup/restore.
    token_cache: Arc<RwLock<HashMap<Pubkey, TokenInfoV1>>>, // Keyed by Pubkey for convenience
    pool_cache: Arc<RwLock<HashMap<Pubkey, PoolInfoV1>>>,   // Keyed by Pubkey
    last_processed_slot: Arc<RwLock<u64>>,
}

impl SharedStateManager {
    pub fn new(mmap_dir: Option<&Path>, mmap_size_mb: Option<u64>) -> std::io::Result<Self> {
        let dir = mmap_dir.unwrap_or_else(|| Path::new("/dev/shm")); // Default to /dev/shm on Linux
        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
        }
        let mmap_file_path = dir.join(MMAP_FILE_NAME);
        let size_bytes = mmap_size_mb.unwrap_or(DEFAULT_MMAP_SIZE_MB) * 1024 * 1024;

        tracing::info!("Initializing SharedStateManager with mmap file: {:?}, size: {}MB", mmap_file_path, size_bytes / (1024*1024));

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&mmap_file_path)?;

        file.set_len(size_bytes)?;

        let mmap = unsafe {
            MmapOptions::new()
                .len(size_bytes as usize)
                .map_mut(&file)?
        };

        let manager = Self {
            mmap_file_path: mmap_file_path.clone(),
            mmap: Arc::new(RwLock::new(mmap)),
            token_cache: Arc::new(RwLock::new(HashMap::new())),
            pool_cache: Arc::new(RwLock::new(HashMap::new())),
            last_processed_slot: Arc::new(RwLock::new(0)),
        };

        // Attempt to load from mmap on startup
        if let Err(e) = manager.load_from_mmap() {
            tracing::warn!("Failed to load initial state from mmap file {:?}: {}. Starting with empty state.", mmap_file_path, e);
            // If loading fails, ensure caches are empty (they are by default)
        } else {
            tracing::info!("Successfully loaded state from mmap file {:?} on startup.", mmap_file_path);
        }

        Ok(manager)
    }

    pub fn update_token(&self, token: TokenInfoV1) {
        let mint_pubkey = Pubkey::new_from_array(token.mint);
        self.token_cache.write().unwrap().insert(mint_pubkey, token);
    }

    pub fn update_pool(&self, pool: PoolInfoV1) {
        let pool_pubkey = Pubkey::new_from_array(pool.address);
        self.pool_cache.write().unwrap().insert(pool_pubkey, pool);
    }

    pub fn get_token(&self, mint: &Pubkey) -> Option<TokenInfoV1> {
        self.token_cache.read().unwrap().get(mint).cloned()
    }

    pub fn get_pool(&self, address: &Pubkey) -> Option<PoolInfoV1> {
        self.pool_cache.read().unwrap().get(address).cloned()
    }

    pub fn get_all_tokens(&self) -> Vec<TokenInfoV1> {
        self.token_cache.read().unwrap().values().cloned().collect()
    }

    pub fn get_all_pools(&self) -> Vec<PoolInfoV1> {
        self.pool_cache.read().unwrap().values().cloned().collect()
    }

    pub fn get_last_processed_slot(&self) -> u64 {
        *self.last_processed_slot.read().unwrap()
    }

    pub fn set_last_processed_slot(&self, slot: u64) {
        let mut current_slot = self.last_processed_slot.write().unwrap();
        if slot > *current_slot {
            *current_slot = slot;
        }
    }

    // Persists the current cache state to the memory-mapped file.
    pub fn persist_to_mmap(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        tracing::debug!("Starting persistence of market state to mmap file: {:?}", self.mmap_file_path);

        let tokens_map: HashMap<[u8; 32], TokenInfoV1> = self.token_cache.read().unwrap()
            .iter().map(|(pk, ti)| (pk.to_bytes(), ti.clone())).collect();
        let pools_map: HashMap<[u8; 32], PoolInfoV1> = self.pool_cache.read().unwrap()
            .iter().map(|(pk, pi)| (pk.to_bytes(), pi.clone())).collect();

        let state = MarketStateV1 {
            tokens: tokens_map,
            pools: pools_map,
            last_processed_slot: self.get_last_processed_slot(),
            last_persisted_timestamp_ns: SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64,
        };

        // Rkyv serialization can be quite large. For very large states, consider streaming or chunking.
        // The buffer size for `to_bytes` should be estimated or generous.
        // Max size of mmap is the hard limit.
        let mut mmap_guard = self.mmap.write().unwrap();
        let buffer_len = mmap_guard.len();

        let bytes = rkyv::to_bytes::<_, 256>(&state)
            .map_err(|e| format!("Rkyv serialization error: {}", e))?;

        if bytes.len() > buffer_len {
            return Err(format!(
                "Serialized state size ({} bytes) exceeds mmap capacity ({} bytes). Increase mmap size.",
                bytes.len(), buffer_len
            ).into());
        }

        mmap_guard[..bytes.len()].copy_from_slice(&bytes);
        // Optionally, write a sentinel or length prefix if not using the whole mmap
        // For simplicity, we assume the start of mmap holds the latest state.

        // Consider if `flush_async` is beneficial or if `flush` is okay (can block)
        mmap_guard.flush()?; // Ensure data is written to disk/persistent media

        tracing::info!(
            "Market state persisted to mmap file {:?} successfully in {:.2}ms. Size: {} bytes.",
            self.mmap_file_path, start_time.elapsed().as_micros() as f64 / 1000.0, bytes.len()
        );
        Ok(())
    }

    // Loads state from the memory-mapped file into the caches.
    pub fn load_from_mmap(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        tracing::debug!("Starting load of market state from mmap file: {:?}", self.mmap_file_path);
        let mmap_guard = self.mmap.read().unwrap(); // Read lock is enough for loading

        // Basic check: if the mmap looks empty (e.g., all zeros at the start), assume no valid state.
        // A more robust check would involve a magic number or checksum at the beginning.
        if mmap_guard.is_empty() || mmap_guard.get(0..8).map_or(true, |s| s.iter().all(|&b| b == 0)) {
            tracing::info!("Mmap file {:?} appears empty or uninitialized. No state loaded.", self.mmap_file_path);
            return Ok(()); // Nothing to load
        }

        // Rkyv validation: checks if the byte slice can be safely interpreted as MarketStateV1
        let archived_state = unsafe { rkyv::archived_root::<MarketStateV1>(&mmap_guard[..]) };
        // Pass `&mut rkyv::validation::validators::DefaultValidator::new()` if using check_bytes feature with validation
        // For check_bytes without explicit validator, it's usually okay for trusted sources.
        // Example with explicit validator:
        // let validator = rkyv::validation::validators::DefaultValidator::new();
        // let archived_state = rkyv::check_archived_root_with_validator::<MarketStateV1, _>(&mmap_guard[..], &mut validator)
        //   .map_err(|e| format!("Rkyv validation error: {}", e))?;

        let state: MarketStateV1 = archived_state.deserialize(&mut Infallible)
            .map_err(|e| format!("Rkyv deserialization error: {}", e))?;

        // Populate caches
        let mut token_cache_writer = self.token_cache.write().unwrap();
        token_cache_writer.clear();
        for (mint_bytes, token_info) in state.tokens {
            token_cache_writer.insert(Pubkey::new_from_array(mint_bytes), token_info);
        }

        let mut pool_cache_writer = self.pool_cache.write().unwrap();
        pool_cache_writer.clear();
        for (pool_bytes, pool_info) in state.pools {
            pool_cache_writer.insert(Pubkey::new_from_array(pool_bytes), pool_info);
        }

        *self.last_processed_slot.write().unwrap() = state.last_processed_slot;

        tracing::info!(
            "Market state loaded from mmap file {:?} successfully in {:.2}ms. {} tokens, {} pools. Last processed slot: {}. Last persisted: {}ns.",
            self.mmap_file_path, start_time.elapsed().as_micros() as f64 / 1000.0,
            token_cache_writer.len(), pool_cache_writer.len(),
            state.last_processed_slot, state.last_persisted_timestamp_ns
        );
        Ok(())
    }
}


// Position Tracking
#[derive(Debug, Clone, RkyvSerialize, RkyvDeserialize, Archive)] // Also make positions serializable if needed
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct PositionV1 { // Added V1
    pub token_mint: [u8; 32], // Using byte array for Rkyv
    pub entry_price_fp: u64, // Fixed-point representation of price (e.g., price * 10^9)
    pub quantity_tokens: u64, // Quantity in smallest token unit (e.g., lamports for SOL-like tokens)
    pub entry_slot: u64,
    pub entry_timestamp_ns: u64, // Nanosecond timestamp
    // PNL can be calculated on the fly or stored if snapshotting
    // pub unrealized_pnl_fp: i64, // Fixed-point
    // pub realized_pnl_fp: i64,   // Fixed-point
    pub strategy_id: String, // Identifier for the strategy that opened this position
}

#[derive(Debug, Clone, RkyvSerialize, RkyvDeserialize, Archive)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
pub struct CompletedTradeV1 { // Added V1
    pub token_mint: [u8; 32],
    pub entry_price_fp: u64,
    pub exit_price_fp: u64,
    pub quantity_tokens: u64,
    pub pnl_fp: i64, // Profit or Loss in fixed-point (e.g., quote currency smallest unit)
    pub duration_ns: u64,
    pub entry_reason: String, // Optional: reason for entry
    pub exit_reason: String,  // Optional: reason for exit
    pub strategy_id: String,
    // Add transaction signatures for entry/exit if needed for audit
    // pub entry_tx_sig: Option<[u8; 64]>,
    // pub exit_tx_sig: Option<[u8; 64]>,
}

pub struct PositionManager {
    // Active positions: Keyed by a unique position ID (e.g., mint + entry_timestamp_ns or a UUID)
    // For simplicity, if one position per mint is assumed by a strategy:
    active_positions: Arc<RwLock<HashMap<Pubkey, PositionV1>>>, // Key: token_mint Pubkey
    historical_trades: Arc<RwLock<Vec<CompletedTradeV1>>>, // Could also be persisted
    // If persisting historical_trades, add mmap logic similar to MarketState
}

impl PositionManager {
    pub fn new() -> Self {
        Self {
            active_positions: Arc::new(RwLock::new(HashMap::new())),
            historical_trades: Arc::new(RwLock::new(Vec::with_capacity(1000))), // Pre-allocate some capacity
        }
    }

    pub fn open_position(&self, position: PositionV1) -> Result<(), String> {
        let mint_pubkey = Pubkey::new_from_array(position.token_mint);
        let mut positions_writer = self.active_positions.write().unwrap();
        if positions_writer.contains_key(&mint_pubkey) {
            return Err(format!("Position already exists for mint {}", mint_pubkey));
        }
        positions_writer.insert(mint_pubkey, position.clone());
        tracing::info!("Opened new position for {}: {:?}", mint_pubkey, position);
        Ok(())
    }

    // Closes a position and records it as a completed trade.
    // `exit_price_fp` is the fixed-point representation of the exit price.
    pub fn close_position(
        &self,
        mint: &Pubkey,
        exit_price_fp: u64,
        exit_reason: String,
    ) -> Option<CompletedTradeV1> {
        let mut positions_writer = self.active_positions.write().unwrap();
        if let Some(position) = positions_writer.remove(mint) {
            // For now, assume 6 decimals as a common default for tokens
            // In production, this should be passed as a parameter or looked up from token cache
            let token_decimals = 6u8; // Common for many SPL tokens
            let _pnl_fp = (exit_price_fp as i128 - position.entry_price_fp as i128) * position.quantity_tokens as i128 / (10_u64.pow(token_decimals as u32) as i128); // Simplified PNL calculation
            // This PNL calculation needs refinement based on how prices and quantities are represented
            // e.g., if quantity_tokens is in base units and price is quote_per_base,
            // pnl_quote = (exit_price_base_units - entry_price_base_units) * quantity_tokens / price_base_unit_divisor

            // A more robust PNL:
            // value_entry = entry_price_fp * quantity_tokens (adjust for decimals)
            // value_exit = exit_price_fp * quantity_tokens (adjust for decimals)
            // pnl = value_exit - value_entry
            // This needs careful handling of fixed point arithmetic. Assuming prices are quote per token.
            // For simplicity, let's assume prices are already in quote currency's smallest unit per token's smallest unit.
            // So, PNL = (exit_price_fp - entry_price_fp) * quantity_tokens / (10^token_decimals) if prices are per full token.
            // Or if prices are per smallest unit of token: PNL = (exit_price_fp - entry_price_fp) * quantity_tokens.
            // The current calculation `(exit_price_fp - entry_price_fp) * quantity_tokens` assumes prices are per smallest unit.
            // Let's assume `pnl_fp` is in the quote currency's smallest unit.
            // (exit_price_fp - entry_price_fp) would be profit per smallest unit of token.
            // So, total profit = (profit per smallest unit) * quantity_tokens (in smallest units).
            // This is if prices are like USDC_lamports / MYTOKEN_lamports.
            // If prices are USDC_dollars / MYTOKEN_tokens, then more scaling is needed.
            // Let's assume for now: pnl_fp = (exit_price_fp - entry_price_fp) * quantity_tokens
            // This calculation needs to be precise based on the definition of fixed-point prices.
            // For now, a placeholder:
            let pnl_fp_calculated: i64 = (exit_price_fp.saturating_sub(position.entry_price_fp) as i64)
                                        .saturating_mul(position.quantity_tokens as i64); // This is a simplification.

            let current_time_ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;

            let trade = CompletedTradeV1 {
                token_mint: position.token_mint,
                entry_price_fp: position.entry_price_fp,
                exit_price_fp,
                quantity_tokens: position.quantity_tokens,
                pnl_fp: pnl_fp_calculated,
                duration_ns: current_time_ns.saturating_sub(position.entry_timestamp_ns),
                entry_reason: "".to_string(), // TODO: Pass entry reason
                exit_reason,
                strategy_id: position.strategy_id.clone(),
            };

            self.historical_trades.write().unwrap().push(trade.clone());
            tracing::info!("Closed position for {}: Trade: {:?}", mint, trade);
            Some(trade)
        } else {
            tracing::warn!("Attempted to close non-existent position for mint {}", mint);
            None
        }
    }

    // Helper to get decimals, needs access to token_cache or pass TokenInfo
    #[allow(dead_code)]
    fn position_decimals_from_mint(&self, _mint: &Pubkey, _token_cache: &HashMap<Pubkey, TokenInfoV1>) -> u8 {
        // TODO: Implement actual lookup
        // token_cache.get(mint).map_or(0, |ti| ti.decimals)
        6 // Placeholder
    }

    pub fn get_active_position(&self, mint: &Pubkey) -> Option<PositionV1> {
        self.active_positions.read().unwrap().get(mint).cloned()
    }

    pub fn get_all_active_positions(&self) -> Vec<PositionV1> {
        self.active_positions.read().unwrap().values().cloned().collect()
    }

    pub fn get_historical_trades(&self) -> Vec<CompletedTradeV1> {
        self.historical_trades.read().unwrap().clone()
    }

    // Basic trading statistics calculation
    pub fn get_trading_stats(&self) -> TradingStats {
        let trades = self.historical_trades.read().unwrap();

        let total_trades = trades.len();
        if total_trades == 0 {
            return TradingStats::default();
        }

        let winning_trades = trades.iter().filter(|t| t.pnl_fp > 0).count();
        let total_pnl_fp: i64 = trades.iter().map(|t| t.pnl_fp).sum();

        let sum_wins_fp: i64 = trades.iter().filter(|t| t.pnl_fp > 0).map(|t| t.pnl_fp).sum();
        let sum_losses_fp: i64 = trades.iter().filter(|t| t.pnl_fp < 0).map(|t| t.pnl_fp.abs()).sum();

        let avg_win_fp = if winning_trades > 0 { sum_wins_fp / winning_trades as i64 } else { 0 };
        let losing_trades = total_trades - winning_trades;
        let avg_loss_fp = if losing_trades > 0 { sum_losses_fp / losing_trades as i64 } else { 0 };

        TradingStats {
            total_trades,
            winning_trades,
            win_rate: if total_trades > 0 { winning_trades as f64 / total_trades as f64 } else { 0.0 },
            total_pnl_fp,
            avg_win_fp,
            avg_loss_fp,
            profit_factor: if avg_loss_fp > 0 { avg_win_fp as f64 / avg_loss_fp as f64 } else { f64::INFINITY }, // Handle division by zero
        }
    }
}

#[derive(Debug, Clone, Default)] // Added Default
pub struct TradingStats {
    pub total_trades: usize,
    pub winning_trades: usize,
    pub win_rate: f64,     // e.g., 0.6 for 60%
    pub total_pnl_fp: i64, // Total PNL in fixed-point
    pub avg_win_fp: i64,   // Average PNL of winning trades in fixed-point
    pub avg_loss_fp: i64,  // Average PNL of losing trades (absolute value) in fixed-point
    pub profit_factor: f64, // Gross profit / Gross loss
}

// Example fixed-point price representation (assuming 9 decimal places for price)
// const PRICE_FP_DECIMALS: u32 = 9;
// fn price_to_fp(price: f64) -> u64 { (price * 10_f64.powi(PRICE_FP_DECIMALS as i32)) as u64 }
// fn price_from_fp(price_fp: u64) -> f64 { price_fp as f64 / 10_f64.powi(PRICE_FP_DECIMALS as i32) }
