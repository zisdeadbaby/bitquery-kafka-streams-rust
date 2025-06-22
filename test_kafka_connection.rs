use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::Message;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env file
    let env_content = std::fs::read_to_string(".env")?;
    for line in env_content.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            std::env::set_var(key.trim(), value.trim());
        }
    }

    let brokers = std::env::var("KAFKA_BROKERS").unwrap_or_default();
    let username = std::env::var("KAFKA_USERNAME").unwrap_or_default();
    let password = std::env::var("KAFKA_PASSWORD").unwrap_or_default();
    let topic = std::env::var("KAFKA_TOPIC").unwrap_or_default();
    let group_id = std::env::var("KAFKA_GROUP_ID").unwrap_or_default();
    let security_protocol = std::env::var("KAFKA_SECURITY_PROTOCOL").unwrap_or("SASL_PLAINTEXT".to_string());

    println!("Testing Kafka connection...");
    println!("Brokers: {}", brokers);
    println!("Username: {}", username);
    println!("Topic: {}", topic);
    println!("Group ID: {}", group_id);
    println!("Security Protocol: {}", security_protocol);

    // Create Kafka consumer configuration
    let mut config = ClientConfig::new();
    config
        .set("bootstrap.servers", &brokers)
        .set("group.id", &group_id)
        .set("auto.offset.reset", "latest")
        .set("enable.auto.commit", "true")
        .set("session.timeout.ms", "30000")
        .set("heartbeat.interval.ms", "10000")
        .set("security.protocol", &security_protocol)
        .set("sasl.mechanism", "PLAIN")
        .set("sasl.username", &username)
        .set("sasl.password", &password);

    // Create consumer
    let consumer: StreamConsumer = config.create()?;
    
    // Subscribe to topic
    consumer.subscribe(&[&topic])?;
    
    println!("Successfully created consumer and subscribed to topic: {}", topic);
    println!("Waiting for messages (timeout: 30 seconds)...");

    // Try to receive a message with timeout
    match timeout(Duration::from_secs(30), consumer.recv()).await {
        Ok(Ok(message)) => {
            println!("✅ SUCCESS: Received message!");
            println!("  Topic: {}", message.topic());
            println!("  Partition: {}", message.partition());
            println!("  Offset: {}", message.offset());
            if let Some(payload) = message.payload() {
                println!("  Payload size: {} bytes", payload.len());
                // Print first 100 bytes as hex for verification
                let preview = &payload[..std::cmp::min(100, payload.len())];
                println!("  Payload preview (hex): {}", hex::encode(preview));
            }
        }
        Ok(Err(e)) => {
            println!("❌ ERROR: Failed to receive message: {}", e);
        }
        Err(_) => {
            println!("⏰ TIMEOUT: No messages received within 30 seconds");
            println!("This might be normal if no new trades are happening right now");
        }
    }

    Ok(())
}
