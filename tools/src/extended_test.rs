use dotenv::dotenv;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::Message;
use std::time::Duration;
use tokio::time::timeout;
use env_logger;
use log::{info, debug, error, warn};

#[tokio::main]
async fn main() {
    // Load .env file into environment variables
    dotenv().ok();
    // Set RUST_LOG from environment or .env LOG_LEVEL, default to info
    let log_env = std::env::var("RUST_LOG").unwrap_or_else(|_| std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()));
    std::env::set_var("RUST_LOG", log_env);
    // Initialize logger from RUST_LOG
    env_logger::init();
    
    info!("🚀 EXTENDED BITQUERY KAFKA VALIDATION SUITE");
    info!("=============================================");

    // Load configuration from environment
    let brokers = std::env::var("KAFKA_BROKERS").expect("KAFKA_BROKERS not set");
    let username = std::env::var("KAFKA_USERNAME").expect("KAFKA_USERNAME not set");
    let password = std::env::var("KAFKA_PASSWORD").expect("KAFKA_PASSWORD not set");
    let base_group_id = std::env::var("KAFKA_GROUP_ID").unwrap_or_else(|_| "extended-test-consumer".into());
    let security_protocol = std::env::var("KAFKA_SECURITY_PROTOCOL").unwrap_or_else(|_| "SASL_PLAINTEXT".into());

    info!("📊 Configuration: brokers={}, protocol={}", brokers, security_protocol);

    // Test various Solana topics with 1-minute timeouts each
    let test_topics = vec![
        ("solana.dextrades.proto", "DEX Trades"),
        ("solana.transactions.proto", "All Transactions"),
        ("solana.tokens.proto", "Token Transfers"),
        ("solana.instructions.proto", "Instructions"),
        ("solana.raw.proto", "Raw Blocks"),
        ("ethereum.dextrades.proto", "Ethereum DEX"),
        ("solana.shreds.proto", "Solana Shreds"),
        ("solana.broadcasted.transactions.proto", "Mempool Transactions"),
    ];

    for (i, (topic, description)) in test_topics.iter().enumerate() {
        info!("");
        info!("🔍 Test {}/{}: Testing {} ({})", i+1, test_topics.len(), description, topic);
        info!("⏱️  Timeout: 60 seconds per topic");

        let group_id = format!("{}-test-{}", base_group_id, i);
        
        if test_topic_with_timeout(&brokers, &username, &password, &security_protocol, topic, &group_id, 60).await {
            info!("✅ SUCCESS: {} is accessible and streaming", description);
        } else {
            warn!("⚠️  UNAVAILABLE: {} not accessible (topic may not exist or no data)", description);
        }
    }

    info!("");
    info!("🏁 EXTENDED VALIDATION COMPLETE");
    info!("===============================");
}

async fn test_topic_with_timeout(
    brokers: &str,
    username: &str,
    password: &str,
    security_protocol: &str,
    topic: &str,
    group_id: &str,
    timeout_secs: u64,
) -> bool {
    info!("🔧 Creating consumer for topic '{}'...", topic);
    
    let mut config = ClientConfig::new();
    config
        .set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("auto.offset.reset", "latest")
        .set("security.protocol", security_protocol)
        .set("sasl.mechanism", "SCRAM-SHA-512")
        .set("sasl.username", username)
        .set("sasl.password", password)
        .set("session.timeout.ms", "30000")
        .set("heartbeat.interval.ms", "10000")
        .set("enable.auto.commit", "true")
        .set("auto.commit.interval.ms", "1000");

    let consumer = match config.create::<StreamConsumer>() {
        Ok(c) => {
            debug!("✅ Consumer created for topic '{}'", topic);
            c
        }
        Err(e) => {
            error!("❌ Failed to create consumer for '{}': {}", topic, e);
            return false;
        }
    };
    
    // Subscribe to topic
    match consumer.subscribe(&[topic]) {
        Ok(_) => debug!("✅ Subscribed to topic '{}'", topic),
        Err(e) => {
            error!("❌ Failed to subscribe to '{}': {}", topic, e);
            return false;
        }
    }
    
    // Try to receive a message within the timeout
    info!("⏰ Waiting for messages from '{}' ({}s timeout)...", topic, timeout_secs);
    
    let mut message_count = 0;
    let start_time = std::time::Instant::now();
    
    while start_time.elapsed().as_secs() < timeout_secs {
        match timeout(Duration::from_secs(5), consumer.recv()).await {
            Ok(Ok(message)) => {
                message_count += 1;
                
                if message_count == 1 {
                    info!("🎉 FIRST MESSAGE RECEIVED from '{}'!", topic);
                    info!("  📌 Topic: {}", message.topic());
                    info!("  🔢 Partition: {}", message.partition());
                    info!("  📍 Offset: {}", message.offset());
                    
                    if let Some(key) = message.key() {
                        info!("  🔑 Key size: {} bytes", key.len());
                    }
                    
                    if let Some(payload) = message.payload() {
                        info!("  📦 Payload size: {} bytes", payload.len());
                        
                        // Show hex preview for binary data
                        let preview_len = std::cmp::min(32, payload.len());
                        let hex_preview = payload[..preview_len].iter()
                            .map(|b| format!("{:02x}", b))
                            .collect::<Vec<String>>()
                            .join("");
                        info!("  🔍 Payload preview (hex): {}", hex_preview);
                    }
                    
                    if let Some(timestamp) = message.timestamp().to_millis() {
                        info!("  🕐 Timestamp: {} ({}ms)", 
                            chrono::DateTime::from_timestamp_millis(timestamp)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                                .unwrap_or_else(|| "Invalid".to_string()),
                            timestamp
                        );
                    }
                }
                
                // Continue receiving for up to 10 messages or until timeout
                if message_count >= 3 {
                    info!("✅ Successfully received {} messages from '{}'", message_count, topic);
                    return true;
                }
            }
            Ok(Err(e)) => {
                debug!("Kafka error for '{}': {}", topic, e);
            }
            Err(_) => {
                debug!("No message within 5s for '{}'", topic);
            }
        }
    }
    
    if message_count > 0 {
        info!("✅ Received {} message(s) from '{}' within timeout", message_count, topic);
        true
    } else {
        debug!("No messages received from '{}' within {}s timeout", topic, timeout_secs);
        false
    }
}
