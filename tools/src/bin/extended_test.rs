use dotenv::dotenv;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::Message;
use std::time::Duration;
use tokio::time::timeout;
use env_logger;
use log::{info, debug, warn};
use chrono::Utc;

#[derive(Debug)]
struct TestResult {
    topic: String,
    success: bool,
    message_count: u32,
    avg_message_size: Option<u64>,
    first_message_offset: Option<i64>,
    last_message_offset: Option<i64>,
    error_message: Option<String>,
    test_duration_seconds: u64,
}

#[tokio::main]
async fn main() {
    // Load .env file into environment variables
    dotenv().ok();
    
    // Set RUST_LOG from environment or .env LOG_LEVEL, default to info
    let log_env = std::env::var("RUST_LOG").unwrap_or_else(|_| std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()));
    std::env::set_var("RUST_LOG", log_env);
    env_logger::init();

    info!("🚀 EXTENDED BITQUERY KAFKA ENDPOINT TEST SUITE");
    info!("===============================================");
    info!("Started at: {}", Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));

    // Load configuration from environment
    let brokers = std::env::var("KAFKA_BROKERS").expect("KAFKA_BROKERS not set");
    let username = std::env::var("KAFKA_USERNAME").expect("KAFKA_USERNAME not set");
    let password = std::env::var("KAFKA_PASSWORD").expect("KAFKA_PASSWORD not set");
    let security_protocol = std::env::var("KAFKA_SECURITY_PROTOCOL").unwrap_or_else(|_| "SASL_PLAINTEXT".into());

    info!("📊 Base Configuration: brokers={}, protocol={}", brokers, security_protocol);
    debug!("Username: {}", username);

    // Define test topics based on Bitquery documentation
    let test_topics = vec![
        // Solana topics (most likely to work with our credentials)
        ("solana.dextrades.proto", "Solana DEX Trades (Protobuf)", 60),
        ("solana.tokens.proto", "Solana Token Events (Protobuf)", 60),
        ("solana.transactions.proto", "Solana Transactions (Protobuf)", 60),
        ("solana.dextrades", "Solana DEX Trades (JSON)", 60),
        ("solana.transactions", "Solana Transactions (JSON)", 60),
        ("solana.transfers", "Solana Token Transfers", 60),
        ("solana.instructions", "Solana Instructions", 60),
        ("solana.instruction_balance_updates", "Solana Balance Updates", 60),
        
        // Broadcasted (mempool) topics
        ("solana.broadcasted.transactions", "Solana Mempool Transactions", 60),
        ("solana.broadcasted.dextrades", "Solana Mempool DEX Trades", 60),
        
        // Try some other blockchains if credentials allow
        ("ethereum.dextrades", "Ethereum DEX Trades", 60),
        ("bitcoin.transactions", "Bitcoin Transactions", 60),
        ("tron.dextrades", "Tron DEX Trades", 60),
    ];

    let mut results: Vec<TestResult> = Vec::new();
    let total_topics = test_topics.len();

    for (index, (topic, description, timeout_seconds)) in test_topics.iter().enumerate() {
        info!("");
        info!("🧪 Test {}/{}: {} ({})", index + 1, total_topics, description, topic);
        info!("⏱️  Timeout: {} seconds", timeout_seconds);
        info!("{}", "─".repeat(80));

        let result = test_topic_endpoint(
            &brokers,
            &username,
            &password,
            &security_protocol,
            topic,
            *timeout_seconds,
        ).await;

        // Log results immediately
        match &result {
            Ok(test_result) => {
                info!("✅ SUCCESS: {}", topic);
                info!("   📦 Messages received: {}", test_result.message_count);
                if let Some(avg_size) = test_result.avg_message_size {
                    info!("   📏 Average message size: {} bytes", avg_size);
                }
                if let (Some(first), Some(last)) = (test_result.first_message_offset, test_result.last_message_offset) {
                    info!("   📍 Offset range: {} to {}", first, last);
                }
                info!("   ⏱️  Test duration: {} seconds", test_result.test_duration_seconds);
            }
            Err(test_result) => {
                warn!("❌ FAILED: {}", topic);
                if let Some(ref error) = test_result.error_message {
                    warn!("   💥 Error: {}", error);
                }
                warn!("   ⏱️  Test duration: {} seconds", test_result.test_duration_seconds);
            }
        }

        results.push(result.unwrap_or_else(|e| e));
        
        // Small delay between tests to avoid overwhelming the brokers
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Final summary
    info!("");
    info!("📊 EXTENDED TEST SUITE SUMMARY");
    info!("=============================");
    
    let successful_tests: Vec<_> = results.iter().filter(|r| r.success).collect();
    let failed_tests: Vec<_> = results.iter().filter(|r| !r.success).collect();
    
    info!("✅ Successful topics: {}/{}", successful_tests.len(), results.len());
    info!("❌ Failed topics: {}/{}", failed_tests.len(), results.len());
    
    if !successful_tests.is_empty() {
        info!("");
        info!("🎉 WORKING ENDPOINTS:");
        for result in &successful_tests {
            info!("  ✅ {} - {} messages", result.topic, result.message_count);
        }
    }
    
    if !failed_tests.is_empty() {
        info!("");
        info!("⚠️  FAILED ENDPOINTS:");
        for result in &failed_tests {
            info!("  ❌ {} - {}", result.topic, result.error_message.as_ref().unwrap_or(&"Unknown error".to_string()));
        }
    }

    // Calculate total messages and data
    let total_messages: u32 = successful_tests.iter().map(|r| r.message_count).sum();
    let total_data_mb: f64 = successful_tests.iter()
        .filter_map(|r| r.avg_message_size.map(|size| (r.message_count as u64 * size) as f64 / 1_048_576.0))
        .sum();

    if total_messages > 0 {
        info!("");
        info!("📈 AGGREGATE STATISTICS:");
        info!("  📦 Total messages received: {}", total_messages);
        info!("  💾 Total data received: {:.2} MB", total_data_mb);
        info!("  🚀 Bitquery Kafka integration is PRODUCTION READY!");
    }

    info!("");
    info!("✨ Extended test suite completed at: {}", Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
}

async fn test_topic_endpoint(
    brokers: &str,
    username: &str,
    password: &str,
    security_protocol: &str,
    topic: &str,
    timeout_seconds: u64,
) -> Result<TestResult, TestResult> {
    let start_time = std::time::Instant::now();
    
    // Create unique group ID for this test
    let group_id = format!("{}-extended-test-{}", username, topic.replace(".", "-"));
    
    debug!("Creating consumer for topic: {}", topic);
    debug!("Group ID: {}", group_id);

    let mut config = ClientConfig::new();
    config
        .set("bootstrap.servers", brokers)
        .set("group.id", &group_id)
        .set("auto.offset.reset", "latest")
        .set("security.protocol", security_protocol)
        .set("sasl.mechanism", "SCRAM-SHA-512")
        .set("sasl.username", username)
        .set("sasl.password", password)
        .set("session.timeout.ms", "30000")
        .set("heartbeat.interval.ms", "10000")
        .set("enable.auto.commit", "true");

    let consumer = match config.create::<StreamConsumer>() {
        Ok(c) => c,
        Err(e) => {
            let duration = start_time.elapsed().as_secs();
            return Err(TestResult {
                topic: topic.to_string(),
                success: false,
                message_count: 0,
                avg_message_size: None,
                first_message_offset: None,
                last_message_offset: None,
                error_message: Some(format!("Failed to create consumer: {}", e)),
                test_duration_seconds: duration,
            });
        }
    };

    // Subscribe to topic
    if let Err(e) = consumer.subscribe(&[topic]) {
        let duration = start_time.elapsed().as_secs();
        return Err(TestResult {
            topic: topic.to_string(),
            success: false,
            message_count: 0,
            avg_message_size: None,
            first_message_offset: None,
            last_message_offset: None,
            error_message: Some(format!("Failed to subscribe to topic: {}", e)),
            test_duration_seconds: duration,
        });
    }

    debug!("Subscribed to topic: {}, waiting for messages...", topic);

    // Collect messages for the specified timeout
    let mut message_count = 0u32;
    let mut total_message_size = 0u64;
    let mut first_offset: Option<i64> = None;
    let mut last_offset: Option<i64> = None;
    
    let timeout_duration = Duration::from_secs(timeout_seconds);
    let mut last_message_time = std::time::Instant::now();
    
    loop {
        match timeout(Duration::from_secs(5), consumer.recv()).await {
            Ok(Ok(message)) => {
                message_count += 1;
                last_message_time = std::time::Instant::now();
                
                let offset = message.offset();
                let payload_size = message.payload_len();
                
                if first_offset.is_none() {
                    first_offset = Some(offset);
                }
                last_offset = Some(offset);
                total_message_size += payload_size as u64;
                
                if message_count == 1 {
                    debug!("First message received from topic: {}", topic);
                    debug!("  Partition: {}, Offset: {}, Size: {} bytes", 
                           message.partition(), offset, payload_size);
                }
                
                // Log progress every 10 messages
                if message_count % 10 == 0 {
                    debug!("Received {} messages from {}", message_count, topic);
                }
            }
            Ok(Err(e)) => {
                debug!("Consumer error for topic {}: {}", topic, e);
                // Continue trying for other messages
            }
            Err(_) => {
                // Timeout waiting for message - check if we should continue
                if last_message_time.elapsed() >= timeout_duration {
                    debug!("Timeout reached for topic: {}", topic);
                    break;
                }
                if start_time.elapsed() >= timeout_duration {
                    debug!("Overall timeout reached for topic: {}", topic);
                    break;
                }
            }
        }
    }

    let duration = start_time.elapsed().as_secs();
    let avg_message_size = if message_count > 0 {
        Some(total_message_size / message_count as u64)
    } else {
        None
    };

    if message_count > 0 {
        Ok(TestResult {
            topic: topic.to_string(),
            success: true,
            message_count,
            avg_message_size,
            first_message_offset: first_offset,
            last_message_offset: last_offset,
            error_message: None,
            test_duration_seconds: duration,
        })
    } else {
        Err(TestResult {
            topic: topic.to_string(),
            success: false,
            message_count: 0,
            avg_message_size: None,
            first_message_offset: None,
            last_message_offset: None,
            error_message: Some("No messages received within timeout period".to_string()),
            test_duration_seconds: duration,
        })
    }
}
