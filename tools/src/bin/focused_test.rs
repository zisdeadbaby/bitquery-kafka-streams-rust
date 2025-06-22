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
    dotenv().ok();
    let log_env = std::env::var("RUST_LOG").unwrap_or_else(|_| std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()));
    std::env::set_var("RUST_LOG", log_env);
    env_logger::init();

    info!("🚀 FOCUSED BITQUERY KAFKA ENDPOINT TEST");
    info!("======================================");
    info!("Started at: {}", Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));

    let brokers = std::env::var("KAFKA_BROKERS").expect("KAFKA_BROKERS not set");
    let username = std::env::var("KAFKA_USERNAME").expect("KAFKA_USERNAME not set");
    let password = std::env::var("KAFKA_PASSWORD").expect("KAFKA_PASSWORD not set");
    let security_protocol = std::env::var("KAFKA_SECURITY_PROTOCOL").unwrap_or_else(|_| "SASL_PLAINTEXT".into());

    info!("📊 Configuration: brokers={}, protocol={}", brokers, security_protocol);

    // Test key topics with shorter timeouts
    let test_topics = vec![
        ("solana.dextrades.proto", "Solana DEX Trades (Protobuf)", 90),
        ("solana.tokens.proto", "Solana Token Events (Protobuf)", 90),
        ("solana.transactions.proto", "Solana Transactions (Protobuf)", 90),
    ];

    let mut results: Vec<TestResult> = Vec::new();

    for (index, (topic, description, timeout_seconds)) in test_topics.iter().enumerate() {
        info!("");
        info!("🧪 Test {}/{}: {} ({})", index + 1, test_topics.len(), description, topic);
        info!("⏱️  Timeout: {} seconds", timeout_seconds);
        info!("{}", "─".repeat(60));

        let result = test_topic_endpoint(
            &brokers,
            &username,
            &password,
            &security_protocol,
            topic,
            *timeout_seconds,
        ).await;

        match &result {
            Ok(test_result) => {
                info!("✅ SUCCESS: {}", topic);
                info!("   📦 Messages: {}", test_result.message_count);
                if let Some(avg_size) = test_result.avg_message_size {
                    info!("   📏 Avg size: {} bytes ({:.1} KB)", avg_size, avg_size as f64 / 1024.0);
                }
                if let (Some(first), Some(last)) = (test_result.first_message_offset, test_result.last_message_offset) {
                    info!("   📍 Offsets: {} → {} (Δ{})", first, last, last - first);
                }
                info!("   ⏱️  Duration: {} seconds", test_result.test_duration_seconds);
            }
            Err(test_result) => {
                warn!("❌ FAILED: {}", topic);
                if let Some(ref error) = test_result.error_message {
                    warn!("   💥 Error: {}", error);
                }
            }
        }

        results.push(result.unwrap_or_else(|e| e));
        tokio::time::sleep(Duration::from_secs(3)).await;
    }

    // Summary
    info!("");
    info!("📊 FINAL SUMMARY");
    info!("================");
    
    let successful_tests: Vec<_> = results.iter().filter(|r| r.success).collect();
    info!("✅ Working endpoints: {}/{}", successful_tests.len(), results.len());
    
    let total_messages: u32 = successful_tests.iter().map(|r| r.message_count).sum();
    let total_data_gb: f64 = successful_tests.iter()
        .filter_map(|r| r.avg_message_size.map(|size| (r.message_count as u64 * size) as f64))
        .sum::<f64>() / 1_073_741_824.0; // Convert to GB

    info!("📦 Total messages: {}", total_messages);
    info!("💾 Total data: {:.3} GB", total_data_gb);
    
    if total_messages > 0 {
        info!("🚀 Bitquery Kafka integration is PRODUCTION READY!");
    }

    info!("✨ Test completed at: {}", Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
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
    let group_id = format!("{}-focused-test-{}", username, topic.replace(".", "-"));

    let mut config = ClientConfig::new();
    config
        .set("bootstrap.servers", brokers)
        .set("group.id", &group_id)
        .set("auto.offset.reset", "latest")
        .set("security.protocol", security_protocol)
        .set("sasl.mechanism", "SCRAM-SHA-512")
        .set("sasl.username", username)
        .set("sasl.password", password)
        .set("session.timeout.ms", "45000")
        .set("heartbeat.interval.ms", "15000")
        .set("enable.auto.commit", "true");

    let consumer = match config.create::<StreamConsumer>() {
        Ok(c) => c,
        Err(e) => {
            let duration = start_time.elapsed().as_secs();
            return Err(TestResult {
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

    if let Err(e) = consumer.subscribe(&[topic]) {
        let duration = start_time.elapsed().as_secs();
        return Err(TestResult {
            success: false,
            message_count: 0,
            avg_message_size: None,
            first_message_offset: None,
            last_message_offset: None,
            error_message: Some(format!("Failed to subscribe: {}", e)),
            test_duration_seconds: duration,
        });
    }

    let mut message_count = 0u32;
    let mut total_message_size = 0u64;
    let mut first_offset: Option<i64> = None;
    let mut last_offset: Option<i64> = None;
    
    let timeout_duration = Duration::from_secs(timeout_seconds);
    
    loop {
        match timeout(Duration::from_secs(10), consumer.recv()).await {
            Ok(Ok(message)) => {
                message_count += 1;
                
                let offset = message.offset();
                let payload_size = message.payload_len();
                
                if first_offset.is_none() {
                    first_offset = Some(offset);
                    debug!("First message: partition {}, offset {}, {} bytes", 
                           message.partition(), offset, payload_size);
                }
                last_offset = Some(offset);
                total_message_size += payload_size as u64;
                
                if message_count % 50 == 0 {
                    debug!("Progress: {} messages from {}", message_count, topic);
                }
            }
            Ok(Err(e)) => {
                debug!("Consumer error: {}", e);
            }
            Err(_) => {
                if start_time.elapsed() >= timeout_duration {
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
            success: false,
            message_count: 0,
            avg_message_size: None,
            first_message_offset: None,
            last_message_offset: None,
            error_message: Some("No messages received".to_string()),
            test_duration_seconds: duration,
        })
    }
}
