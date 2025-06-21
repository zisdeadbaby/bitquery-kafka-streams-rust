pub mod sniper; // Make sniper module public

use async_trait::async_trait;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc; // For Arc<Config> or other shared resources if needed by strategies

use crate::state::{MarketStateV1, TokenInfoV1, PositionV1, PoolInfoV1}; // Use versioned state types
use crate::config::Config; // Allow strategies to access config if necessary
use crate::error::Result as OpsNodeResult; // For fallible strategy methods

#[derive(Debug, Clone)]
pub struct TokenOpportunity {
    pub token_mint: Pubkey,         // Mint of the token presenting the opportunity
    pub associated_pool: Pubkey,    // Pubkey of the liquidity pool
    pub initial_liquidity_quote: u64, // Liquidity in quote currency (e.g., USDC lamports)
    pub score: f64,                 // Strategy-specific score (0.0 to 1.0)
    pub expected_return_bps: i32,   // Expected return in basis points (can be negative)
    pub confidence: f64,            // Confidence in this opportunity (0.0 to 1.0)
    pub recommended_trade_size_quote: u64, // Recommended trade size in quote currency
    pub priority: u8,               // Priority for execution (higher is more important)
    // Add any other fields that might be useful for decision making or execution
    pub details: Option<String>,    // Optional: more human-readable details or justification
}

// Parameters for executing a snipe trade, constructed by the strategy
#[derive(Debug, Clone)]
pub struct SnipeExecutionParams {
    pub token_to_buy_mint: Pubkey,
    pub pool_address: Pubkey,
    pub amount_in_quote: u64,       // Amount of quote currency (e.g., SOL, USDC) to spend
    pub min_amount_out_token: u64,  // Minimum amount of token_to_buy expected (slippage protection)
    pub max_priority_fee_lamports: u64, // Max priority fee strategy is willing to pay for this tx
    pub use_jito_bundle: bool,      // Hint from strategy whether Jito bundle is preferred
    // Add other execution details: e.g., specific RPC node, retry strategy for this trade
}

// Parameters for exiting a position
#[derive(Debug, Clone)]
pub struct ExitParams {
    pub reason: String, // e.g., "take_profit", "stop_loss", "time_limit"
    pub min_amount_out_quote: u64, // Minimum quote currency expected back (slippage for exit)
    pub max_priority_fee_lamports: u64,
    pub use_jito_bundle: bool,
}


// A more comprehensive trait for trading strategies
#[async_trait]
pub trait TradingStrategy: Send + Sync {
    // Initializes the strategy, potentially with configuration.
    // fn init(&mut self, config: Arc<Config>, shared_state: Arc<SharedStateManager>) -> OpsNodeResult<()>;

    // Returns the unique name of the strategy.
    fn name(&self) -> &str;

    // Called when a new token or pool is detected, or on periodic re-evaluation.
    // `token_info` is the newly detected or updated token.
    // `pool_info` is its associated liquidity pool, if found.
    // `current_market_state` provides broader context.
    async fn evaluate_new_entity(
        &self,
        token_info: &TokenInfoV1,
        pool_info: Option<&PoolInfoV1>, // Pool might not exist or be found yet
        current_market_state: &MarketStateV1, // Provides context like current slot, all tokens/pools
        config: Arc<Config>, // Pass config for strategy-specific parameters
    ) -> OpsNodeResult<Option<TokenOpportunity>>;

    // Called periodically to re-evaluate existing open positions.
    // `position` is the current open position.
    // `current_market_state` provides context, including current prices/liquidity for the position's token.
    async fn re_evaluate_position(
        &self,
        position: &PositionV1,
        current_market_state: &MarketStateV1,
        config: Arc<Config>,
    ) -> OpsNodeResult<Option<ExitParams>>; // Returns Some(ExitParams) if exit is recommended

    // (Optional) Called when a trade is confirmed or fails.
    // async fn on_trade_result(&self, trade_result: &CompletedTradeV1, is_entry: bool);

    // (Optional) Called periodically for the strategy to perform internal updates or logging.
    // async fn on_tick(&self, current_market_state: &MarketStateV1) -> OpsNodeResult<()>;

    // (Optional) Allows strategy to provide dynamic parameters for transaction sending
    // fn get_transaction_options(&self, opportunity: &TokenOpportunity) -> Option<TransactionOptions>;
}

// Example:
// pub struct TransactionOptions {
//     pub retry_policy: RetryPolicy,
//     pub target_rpc_node: Option<String>,
// }
