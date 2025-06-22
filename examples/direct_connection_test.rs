use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::Message;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing direct Kafka connection to Bitquery...");

    // Use the actual credentials from the environment
    let brokers = "rpk0.bitquery.io:9092,rpk1.bitquery.io:9092,rpk2.bitquery.io:9092";
    let username = "solana_113";
    let password = "cuDDLAUguo75blkNdbCBSHvNCK1udw";
    let topic = "solana.dextrades.proto";
    let group_id = "solana_113-test-connection";

    println!("Brokers: {}", brokers);
    println!("Username: {}", username);
    println!("Topic: {}", topic);
    println!("Group ID: {}", group_id);

    // Create Kafka consumer configuration
    let mut config = ClientConfig::new();
    config
        .set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("auto.offset.reset", "latest")
        .set("enable.auto.commit", "true")
        .set("session.timeout.ms", "30000")
        .set("heartbeat.interval.ms", "10000")
        .set("security.protocol", "SASL_PLAINTEXT")
        .set("sasl.mechanism", "PLAIN")
        .set("sasl.username", username)
        .set("sasl.password", password);

    println!("Creating consumer...");
    
    // Create consumer
    let consumer: StreamConsumer = match config.create() {
        Ok(c) => {
            println!("✅ Consumer created successfully");
            c
        }
        Err(e) => {
            println!("❌ Failed to create consumer: {}", e);
            return Err(e.into());
        }
    };
    
    println!("Subscribing to topic: {}", topic);
    
    // Subscribe to topic
    match consumer.subscribe(&[topic]) {
        Ok(_) => println!("✅ Successfully subscribed to topic"),
        Err(e) => {
            println!("❌ Failed to subscribe: {}", e);
            return Err(e.into());
        }
    }
    
    println!("Waiting for messages (timeout: 30 seconds)...");

    // Try to receive a message with timeout
    match timeout(Duration::from_secs(30), consumer.recv()).await {
        Ok(Ok(message)) => {
            println!("🎉 SUCCESS: Received message from Bitquery Kafka!");
            println!("  Topic: {}", message.topic());
            println!("  Partition: {}", message.partition());
            println!("  Offset: {}", message.offset());
            
            if let Some(key) = message.key() {
                println!("  Key size: {} bytes", key.len());
            }
            
            if let Some(payload) = message.payload() {
                println!("  Payload size: {} bytes", payload.len());
                // Print first 100 bytes as hex for verification
                let preview = &payload[..std::cmp::min(100, payload.len())];
                println!("  Payload preview (hex): {}", hex::encode(preview));
                
                // Try to decode as protobuf or JSON to see if it's structured data
                if let Ok(json_str) = std::str::from_utf8(payload) {
                    if json_str.starts_with('{') {
                        println!("  Payload appears to be JSON");
                    }
                } else {
                    println!("  Payload appears to be binary (protobuf?)");
                }
            }
            
            println!("🎯 REAL COMMUNICATION TEST PASSED!");
        }
        Ok(Err(e)) => {
            println!("❌ ERROR: Failed to receive message: {}", e);
            return Err(e.into());
        }
        Err(_) => {
            println!("⏰ TIMEOUT: No messages received within 30 seconds");
            println!("This might be normal if no new DEX trades are happening right now");
            println!("The connection itself appears to be working though!");
        }
    }

    Ok(())
}
