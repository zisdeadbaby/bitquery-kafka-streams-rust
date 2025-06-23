# Pump.fun Data Streaming Guide

This guide explains how to configure the Zola Streams service to capture and process pump.fun data from Bitquery's Kafka streams.

## Overview

Pump.fun is a popular token launch platform on Solana. The Zola Streams service can capture all pump.fun activity through Bitquery's Kafka streaming topics, including:

- Token creation and metadata
- Real-time trades and price updates  
- Trading volume and market data
- Token holder analytics
- Bonding curve progression

## Kafka Topics for Pump.fun Data

### 1. `solana.dextrades.proto` - DEX Trades (Primary)

**Contains:** All DEX trades on Solana, including pump.fun trades
**Pump.fun Filter:** `Trade.Dex.ProgramAddress == "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"`

**Data includes:**
- Trade price in SOL and USD
- Buy/sell amounts and volumes
- Trader wallet addresses
- Transaction signatures
- Block timestamps
- Market addresses

### 2. `solana.tokens.proto` - Token Events

**Contains:** Token creation, supply updates, and metadata changes
**Pump.fun Filter:** `Instruction.Program.Address == "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"`

**Data includes:**
- New token creation events
- Token metadata (name, symbol, URI)
- Developer addresses
- Initial supply
- Token standard information

### 3. `solana.transactions.proto` - All Transactions

**Contains:** All Solana transactions including pump.fun activities
**Pump.fun Filter:** Filter by program address in transaction instructions

**Data includes:**
- Complete transaction details
- Instruction-level data
- Account changes
- Token balance updates

## Current Configuration

The service is already configured to listen to pump.fun data:

```rust
topics: vec![
    "solana.transactions.proto".to_string(),  // All Solana transactions including pump.fun
    "solana.dextrades.proto".to_string(),     // DEX trades including pump.fun trades  
    "solana.tokens.proto".to_string(),        // Token events including pump.fun token creation
],
```

## Identifying Pump.fun Data

### Pump.fun Program Address
```
6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
```

### Common Filters in Code

```rust
// Filter DEX trades for pump.fun only
fn is_pump_fun_trade(trade: &DexTrade) -> bool {
    trade.dex.program_address == "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
}

// Filter token creation events for pump.fun
fn is_pump_fun_token_creation(instruction: &Instruction) -> bool {
    instruction.program.address == "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P" &&
    instruction.method == "create"
}
```

## Key Pump.fun Data Points

### Real-time Trading Data
- **Buy/Sell Trades**: Immediate trade execution data
- **Price Updates**: Latest token prices in SOL and USD
- **Volume Metrics**: Trading volume per token and time period
- **Market Cap**: Calculated as `price * 1,000,000,000` (1B token supply)

### Token Lifecycle Events
- **Token Creation**: New token launches with metadata
- **Bonding Curve Progress**: Trade activity showing curve progression
- **Graduation to Raydium**: When tokens reach the graduation threshold
- **Developer Activity**: Creator wallet transactions

### Market Analytics
- **Top Performers**: Tokens by volume, price change, market cap
- **New Launches**: Recently created tokens
- **Trending Tokens**: High activity tokens
- **Holder Analytics**: Top holders and distribution

## Example Use Cases

### 1. Real-time Pump.fun Trade Monitor
```rust
// Monitor all pump.fun trades in real-time
async fn monitor_pump_fun_trades(client: &BitqueryClient) -> Result<()> {
    loop {
        if let Some(event) = client.next_event().await? {
            if let Some(trade) = event.as_dex_trade() {
                if is_pump_fun_trade(trade) {
                    println!("Pump.fun Trade: {} {} at ${}", 
                        trade.amount, 
                        trade.token.symbol, 
                        trade.price_usd
                    );
                }
            }
        }
    }
}
```

### 2. New Token Launch Detector
```rust
// Detect new pump.fun token launches
async fn detect_new_tokens(client: &BitqueryClient) -> Result<()> {
    loop {
        if let Some(event) = client.next_event().await? {
            if let Some(token_event) = event.as_token_update() {
                if is_pump_fun_token_creation(&token_event.instruction) {
                    println!("New Pump.fun Token: {} ({})", 
                        token_event.token.name,
                        token_event.token.symbol
                    );
                }
            }
        }
    }
}
```

### 3. Market Cap Tracker
```rust
// Track market cap milestones (King of the Hill at $69K)
async fn track_market_caps(client: &BitqueryClient) -> Result<()> {
    let mut token_prices = HashMap::new();
    
    loop {
        if let Some(event) = client.next_event().await? {
            if let Some(trade) = event.as_dex_trade() {
                if is_pump_fun_trade(trade) {
                    let market_cap = trade.price_usd * 1_000_000_000.0; // 1B supply
                    token_prices.insert(trade.token.address.clone(), market_cap);
                    
                    // Check for King of the Hill milestone
                    if market_cap >= 69_000.0 && market_cap <= 70_000.0 {
                        println!("🚀 King of the Hill: {} at ${:.0}", 
                            trade.token.symbol, market_cap);
                    }
                }
            }
        }
    }
}
```

## Performance Considerations

### High-Frequency Data
- Pump.fun generates high-frequency trading data
- Consider batch processing for performance
- Use efficient filtering to reduce processing overhead
- Implement backpressure handling for peak periods

### Memory Management
- Cache frequently accessed token metadata
- Periodically clean old price data
- Use streaming aggregation for statistics
- Monitor memory usage during high activity

## Integration with GraphQL

While Kafka streams provide the fastest data, you can also use Bitquery's GraphQL API for:

- Historical analysis
- Complex queries and aggregations
- Data validation and backfill
- Real-time subscriptions (slower than Kafka)

### Example GraphQL Subscription
```graphql
subscription PumpFunTrades {
  Solana {
    DEXTradeByTokens(
      where: {
        Trade: {
          Dex: {
            ProgramAddress: {
              is: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
            }
          }
        }
        Transaction: { Result: { Success: true } }
      }
    ) {
      Block { Time }
      Trade {
        Currency { MintAddress Symbol Name }
        Price
        PriceInUSD
        Amount
      }
      Transaction { Signature }
    }
  }
}
```

## Testing Configuration

To test pump.fun data streaming:

1. **Start the service**:
   ```bash
   export BITQUERY_USERNAME="your_username"
   export BITQUERY_PASSWORD="your_password"
   cargo run
   ```

2. **Look for pump.fun trades** in the logs:
   - Filter by program address `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`
   - Monitor `solana.dextrades.proto` topic
   - Check event processing metrics

3. **Verify data quality**:
   - Compare with pump.fun website
   - Validate price calculations
   - Check for missing trades or duplicates

## Monitoring and Alerts

Set up monitoring for:
- **Trade Volume**: Unusual spikes in pump.fun activity
- **New Token Rate**: High frequency of token launches
- **Price Movements**: Significant price changes or milestones
- **System Health**: Processing latency and error rates

## References

- [Bitquery Pump.fun API Documentation](https://docs.bitquery.io/docs/examples/Solana/Pump-Fun-API/)
- [Pump.fun Program Address](https://solscan.io/account/6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P)
- [Bitquery Kafka Streaming Guide](https://docs.bitquery.io/docs/streams/kafka-streaming-concepts/)
- [Solana Program Library](https://spl.solana.com/)
