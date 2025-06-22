use super::{TokenOpportunity, TradingStrategy, ExitParams}; // Removed SnipeExecutionParams as it's not directly returned by evaluate_new_entity
use crate::state::{MarketStateV1, TokenInfoV1, PositionV1, PoolInfoV1};
use crate::config::Config; // To access strategy-specific configs
use crate::error::{OpsNodeError, Result as OpsNodeResult};
use async_trait::async_trait;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

// Configuration specific to the TokenSnipingStrategy
#[derive(Debug, Clone)] // Potentially load this from main Config.trading or a dedicated section
pub struct TokenSniperConfig {
    pub min_liquidity_sol_equivalent: f64, // Min liquidity in SOL equivalent (e.g., from USDC pool pair)
    pub max_position_size_sol_equivalent: f64, // Max position size in SOL equivalent
    pub take_profit_bps: u32,      // Take profit target in basis points (e.g., 2000 for 20%)
    pub stop_loss_bps: u32,        // Stop loss trigger in basis points (e.g., 500 for 5%)
    pub min_score_threshold: f64,  // Minimum score (0-1) to consider an opportunity
    pub max_token_age_slots: u64,  // Max age of a token in slots to be considered "new"
    pub max_holding_duration_secs: u64, // Max time to hold a position in seconds
    pub position_size_risk_factor: f64, // Adjusts position size based on score (0-1, higher = more sensitive to score)
}

impl Default for TokenSniperConfig {
    fn default() -> Self {
        Self {
            min_liquidity_sol_equivalent: 10.0, // e.g., 10 SOL worth of liquidity
            max_position_size_sol_equivalent: 1.0, // Max 1 SOL per trade
            take_profit_bps: 2000, // 20%
            stop_loss_bps: 1000,   // 10%
            min_score_threshold: 0.6,
            max_token_age_slots: 5000, // Roughly 30-40 minutes
            max_holding_duration_secs: 300, // 5 minutes
            position_size_risk_factor: 0.5,
        }
    }
}


pub struct TokenSnipingStrategy {
    config: TokenSniperConfig,
    // Potentially add ML model handlers or other stateful components here
    // ml_model: Option<MLEnhancedSniperModel>, // If using ML
}

impl TokenSnipingStrategy {
    pub fn new(strategy_config: TokenSniperConfig) -> Self {
        Self {
            config: strategy_config,
            // ml_model: None, // Initialize ML model if applicable
        }
    }

    // Helper to extract sniper config from global config
    // This depends on how strategy-specific configs are structured in the main Config.
    // For now, assuming TokenSniperConfig is passed directly.
    // pub fn from_global_config(global_config: Arc<Config>) -> Self {
    //     // Placeholder: extract or create TokenSniperConfig from global_config.trading
    //     let strategy_cfg = TokenSniperConfig { /* ... populate from global_config ... */ };
    //     Self::new(strategy_cfg)
    // }

    // Calculates a score for a token based on various heuristics.
    // Returns a score between 0.0 and 1.0.
    fn calculate_token_score(
        &self,
        token: &TokenInfoV1,
        pool: Option<&PoolInfoV1>,
        market_state: &MarketStateV1,
    ) -> f64 {
        let mut score = 0.0;
        let mut _contributing_factors = 0; // Count how many factors contribute to the score

        // 1. Liquidity Score (0.0 - 0.4)
        if let Some(p) = pool {
            // Assuming p.reserve_b is the quote currency (e.g., SOL, USDC)
            // This needs a way to convert reserve_b to SOL equivalent if it's not SOL.
            // For simplicity, let's assume reserve_b is SOL lamports for now if it's paired with the new token.
            // Or, if it's USDC, we need SOL/USDC price.
            // Let's assume min_liquidity_sol_equivalent applies to the quote currency side of the pair.
            let liquidity_sol_equivalent = p.reserve_b as f64 / 1_000_000_000.0; // Assuming reserve_b is lamports of SOL/USDC

            if liquidity_sol_equivalent >= self.config.min_liquidity_sol_equivalent {
                // Normalize score based on how much it exceeds min, up to a cap (e.g., 10x min_liquidity)
                let liquidity_cap = self.config.min_liquidity_sol_equivalent * 10.0;
                let normalized_liquidity = (liquidity_sol_equivalent / liquidity_cap).min(1.0);
                score += normalized_liquidity * 0.4; // Weight for liquidity
                _contributing_factors += 1;
            } else {
                return 0.0; // Below minimum liquidity, no opportunity
            }
        } else {
            return 0.0; // No pool found, critical issue
        }

        // 2. Token Age Score (0.0 - 0.3) - Prefer newer tokens
        let token_age_slots = market_state.last_processed_slot.saturating_sub(token.created_slot);
        if token_age_slots <= self.config.max_token_age_slots {
            let age_score_contribution = (1.0 - (token_age_slots as f64 / self.config.max_token_age_slots as f64)) * 0.3;
            score += age_score_contribution.max(0.0); // Ensure non-negative
            _contributing_factors += 1;
        } else {
            return 0.0; // Token is too old for this sniping strategy
        }

        // 3. Name/Symbol Heuristics (0.0 - 0.2) - Highly subjective, example
        let name_lower = token.name.to_lowercase();
        let symbol_lower = token.symbol.to_lowercase();
        if name_lower.contains("inu") || name_lower.contains("elon") || symbol_lower.len() <= 4 {
            score += 0.1; // Small bonus for "meme" characteristics or short symbols
        }
        // Avoid overly long names/symbols
        if token.name.len() < 20 && token.symbol.len() < 8 {
            score += 0.1;
        }
        // Max contribution from this section is 0.2
        // This part is very basic and can be expanded with more sophisticated text analysis or ML.
        _contributing_factors += 1; // Assume it always contributes something or is neutral

        // 4. Total Supply Score (0.0 - 0.1) - Penalize extremely high supplies (potential dilution)
        if token.total_supply > 0 && token.total_supply < 1_000_000_000_000_000 { // Avoid overflow with log10
            let supply_score = (1.0 - (token.total_supply as f64).log10().max(0.0) / 18.0).max(0.0); // Normalize based on log10 of supply
            score += supply_score * 0.1;
            _contributing_factors += 1;
        }

        // Normalize score: if all factors contributed, score is already somewhat normalized by weights.
        // If only some factors contributed, the max possible score is lower.
        // This current sum of weights is 0.4 + 0.3 + 0.2 + 0.1 = 1.0.
        // So, the score is already effectively between 0 and 1.
        score.max(0.0).min(1.0) // Clamp score to [0,1]
    }

    // Calculates position size based on opportunity score and available capital.
    // `available_balance_quote` is the amount of quote currency (e.g. SOL) available for trading.
    fn calculate_dynamic_position_size_quote(
        &self,
        opportunity_score: f64,
        available_balance_quote: u64,
    ) -> u64 {
        // Base size is a fraction of max allowed position size, scaled by score
        let base_size_quote = (self.config.max_position_size_sol_equivalent * 1_000_000_000.0) as u64; // Convert SOL to lamports

        // Scale size by score (e.g., linear or exponential scaling)
        // Using risk_factor: score^risk_factor can make it more conservative for lower scores
        let score_scaling_factor = opportunity_score.powf(self.config.position_size_risk_factor);
        let mut desired_size_quote = (base_size_quote as f64 * score_scaling_factor) as u64;

        // Ensure it doesn't exceed a certain percentage of total available balance (e.g., 10%)
        let max_from_balance_quote = available_balance_quote / 10; // Max 10% of available balance

        desired_size_quote = desired_size_quote.min(max_from_balance_quote);
        desired_size_quote.min(base_size_quote) // Final cap at absolute max position size
    }

    // Helper to get current price from pool reserves.
    // Returns price of token (mint_a) in terms of quote_token (mint_b).
    // Price = reserve_quote / reserve_token (needs adjustment for decimals)
    fn get_current_price_fp(
        &self,
        _token_mint: &Pubkey, // The token whose price we want
        pool: &PoolInfoV1,
        market_state: &MarketStateV1,
    ) -> Option<u64> {
        // Identify which reserve is the token and which is the quote currency (e.g. SOL or USDC)
        // This requires knowing the decimals of both tokens in the pool.
        let token_a_info = market_state.tokens.get(&pool.token_a_mint)?;
        let _token_b_info = market_state.tokens.get(&pool.token_b_mint)?;

        // Assume token_a is the one we are interested in, and token_b is the quote (e.g. SOL/USDC)
        // This logic needs to be robust if the pair order can vary.
        // Price of token A in terms of token B = reserve_B / reserve_A
        // (Amount of B per unit of A)
        if pool.reserve_a == 0 || pool.reserve_b == 0 { return None; }

        // Price = (ReserveB / 10^DecimalsB) / (ReserveA / 10^DecimalsA)
        // Price_fp = Price * 10^PRICE_FP_DECIMALS
        // Price_fp = ( (ReserveB * 10^DecimalsA) / (ReserveA * 10^DecimalsB) ) * 10^PRICE_FP_DECIMALS
        // To avoid floating point: Price_fp = (ReserveB * 10^DecimalsA * 10^PRICE_FP_DECIMALS) / (ReserveA * 10^DecimalsB)
        // This requires a common fixed-point decimal basis for prices (e.g., 9 decimals for price itself)

        // Simplified: Price_fp = (pool.reserve_b * 10^token_a_info.decimals) / pool.reserve_a
        // This gives price in token_b units per full token_a unit, scaled by 10^token_b_info.decimals
        // This is not a general fixed point price. For now, returning raw ratio for simplicity.
        // A proper fixed-point library or careful u128 arithmetic is needed here.
        // Placeholder: return reserve_b / reserve_a (amount of B per unit of A, if decimals were same)
        if pool.reserve_a == 0 { return Some(u64::MAX); } // Avoid div by zero, effectively infinite price

        // This is a conceptual price, needs proper fixed point arithmetic.
        // Let's say price is quote tokens per base token.
        // If pool.token_a_mint is our token, and pool.token_b_mint is quote (e.g. SOL/USDC)
        // Price = pool.reserve_b / pool.reserve_a (ignoring decimals for simplicity here)
        // This calculation is highly simplified and needs a robust fixed-point math library for production.
        let price_fp = (pool.reserve_b as u128 * 10_u128.pow(token_a_info.decimals as u32)) / pool.reserve_a as u128;

        // If this price_fp is meant to be in units of quote_token's smallest denomination,
        // then it should be scaled by 10^token_b_info.decimals.
        // price_in_quote_smallest_units = price_fp * 10^token_b_info.decimals / (10^token_a_info.decimals * 10^token_a_info.decimals) ?
        // This is complex. For now, assume price_fp is a u64 that can be compared.
        Some(price_fp as u64)
    }
}

#[async_trait]
impl TradingStrategy for TokenSnipingStrategy {
    fn name(&self) -> &str {
        "TokenSniperV1"
    }

    async fn evaluate_new_entity(
        &self,
        token_info: &TokenInfoV1,
        pool_info: Option<&PoolInfoV1>,
        current_market_state: &MarketStateV1,
        _app_config: Arc<Config>, // Global app config, may contain available balance
    ) -> OpsNodeResult<Option<TokenOpportunity>> {

        let score = self.calculate_token_score(token_info, pool_info, current_market_state);

        if score < self.config.min_score_threshold {
            tracing::debug!("Token {} (pool {:?}) score {:.2} below threshold {:.2}", Pubkey::new_from_array(token_info.mint), pool_info.map(|p| Pubkey::new_from_array(p.address)), score, self.config.min_score_threshold);
            return Ok(None);
        }

        let current_pool = match pool_info {
            Some(p) => p,
            None => {
                tracing::debug!("No pool info provided for token {} during evaluation.", Pubkey::new_from_array(token_info.mint));
                return Ok(None); // Cannot proceed without pool for liquidity check
            }
        };

        // TODO: Get available balance from a shared state or config
        let available_balance_quote = (_app_config.trading.max_position_size_sol * 1.5 * 1_000_000_000.0) as u64; // Placeholder, should be actual balance

        let trade_size_quote = self.calculate_dynamic_position_size_quote(score, available_balance_quote);
        if trade_size_quote == 0 {
            tracing::debug!("Calculated trade size is 0 for token {}.", Pubkey::new_from_array(token_info.mint));
            return Ok(None);
        }

        // Simplified expected return based on score (e.g. higher score = higher expected TP)
        let expected_return_bps = (score * self.config.take_profit_bps as f64) as i32;

        tracing::info!(
            "Opportunity found for token {}: Score {:.2}, Pool: {}, Liquidity (quote): {:.2}, Trade Size (quote): {:.4} SOL",
            Pubkey::new_from_array(token_info.mint), score, Pubkey::new_from_array(current_pool.address),
            current_pool.reserve_b as f64 / 1e9, // Assuming reserve_b is quote lamports
            trade_size_quote as f64 / 1e9
        );

        Ok(Some(TokenOpportunity {
            token_mint: Pubkey::new_from_array(token_info.mint),
            associated_pool: Pubkey::new_from_array(current_pool.address),
            initial_liquidity_quote: current_pool.reserve_b, // Assuming reserve_b is quote currency
            score,
            expected_return_bps,
            confidence: score, // Use score as confidence for now
            recommended_trade_size_quote: trade_size_quote,
            priority: (score * 10.0) as u8, // Simple priority scaling
            details: Some(format!("Token Age: {} slots, Symbol: {}", current_market_state.last_processed_slot.saturating_sub(token_info.created_slot), token_info.symbol)),
        }))
    }

    async fn re_evaluate_position(
        &self,
        position: &PositionV1,
        current_market_state: &MarketStateV1,
        _app_config: Arc<Config>, // For potential dynamic config adjustments
    ) -> OpsNodeResult<Option<ExitParams>> {
        let token_mint_pk = Pubkey::new_from_array(position.token_mint);
        let token_info = match current_market_state.tokens.get(&position.token_mint) {
            Some(ti) => ti,
            None => return Err(OpsNodeError::Strategy(format!("Token info not found for position on mint {}",token_mint_pk))),
        };

        let pool_address_bytes = match token_info.pool_address {
            Some(pa) => pa,
            None => return Err(OpsNodeError::Strategy(format!("Position's token {} has no pool address in market state", token_mint_pk))),
        };

        let pool_info = match current_market_state.pools.get(&pool_address_bytes) {
            Some(pi) => pi,
            None => return Err(OpsNodeError::Strategy(format!("Pool info not found for position on mint {} (pool addr: {})", token_mint_pk, Pubkey::new_from_array(pool_address_bytes)))),
        };

        let current_price_fp = match self.get_current_price_fp(&token_mint_pk, pool_info, current_market_state) {
            Some(price) => price,
            None => {
                tracing::warn!("Could not determine current price for position on mint {}. Holding.", token_mint_pk);
                return Ok(None); // Cannot make decision without price
            }
        };

        // Price change calculation (needs robust fixed-point math)
        // (current_price_fp - entry_price_fp) / entry_price_fp
        // For simplicity, using bps directly on fixed point values if they are comparable.
        let price_change_bps_numerator = (current_price_fp as i128).saturating_sub(position.entry_price_fp as i128) * 10000;
        let price_change_bps = if position.entry_price_fp > 0 {
            (price_change_bps_numerator / position.entry_price_fp as i128) as i32
        } else {
            0 // Avoid division by zero if entry price was zero
        };

        // 1. Take Profit
        if price_change_bps >= self.config.take_profit_bps as i32 {
            tracing::info!(
                "Take profit for {}: Current Price Fp: {}, Entry Price Fp: {}, Change: {} bps (Target: {} bps)",
                token_mint_pk, current_price_fp, position.entry_price_fp, price_change_bps, self.config.take_profit_bps
            );
            return Ok(Some(ExitParams {
                reason: format!("take_profit_{}bps", price_change_bps),
                min_amount_out_quote: 0, // TODO: Calculate based on current_price_fp and slippage
                max_priority_fee_lamports: _app_config.trading.priority_fee_cap, // Use global cap or strategy specific
                use_jito_bundle: _app_config.trading.enable_jito_bundles,
            }));
        }

        // 2. Stop Loss
        if price_change_bps <= -(self.config.stop_loss_bps as i32) {
            tracing::warn!(
                "Stop loss for {}: Current Price Fp: {}, Entry Price Fp: {}, Change: {} bps (Target: -{} bps)",
                token_mint_pk, current_price_fp, position.entry_price_fp, price_change_bps, self.config.stop_loss_bps
            );
            return Ok(Some(ExitParams {
                reason: format!("stop_loss_{}bps", price_change_bps),
                min_amount_out_quote: 0, // TODO: Calculate
                max_priority_fee_lamports: _app_config.trading.priority_fee_cap,
                use_jito_bundle: _app_config.trading.enable_jito_bundles,
            }));
        }

        // 3. Time-based Exit
        let holding_duration_ns = aktuellen_timestamp_ns().saturating_sub(position.entry_timestamp_ns);
        if holding_duration_ns >= self.config.max_holding_duration_secs * 1_000_000_000 {
            tracing::info!(
                "Time-based exit for {} after {:.2}s (Max: {}s)",
                token_mint_pk, holding_duration_ns as f64 / 1e9, self.config.max_holding_duration_secs
            );
            return Ok(Some(ExitParams {
                reason: "time_limit".to_string(),
                min_amount_out_quote: 0, // TODO: Calculate
                max_priority_fee_lamports: _app_config.trading.priority_fee_cap,
                use_jito_bundle: _app_config.trading.enable_jito_bundles,
            }));
        }

        Ok(None) // Hold position
    }
}

// Helper for current time, replace with actual time source if needed
fn aktuellen_timestamp_ns() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64
}


// MLEnhancedSniper related code from original prompt - can be integrated or kept separate
// For now, it's commented out as it's a distinct strategy or an enhancement layer.
/*
pub struct MLEnhancedSniper {
    base_strategy: TokenSnipingStrategy, // Example of composition
    // feature_weights: Vec<f64>, // Or a loaded model
}

impl MLEnhancedSniper {
    // ... methods for feature extraction and prediction ...
}

// Implement TradingStrategy for MLEnhancedSniper, possibly by delegating to base_strategy
// and then adjusting its outputs or decisions.
*/
