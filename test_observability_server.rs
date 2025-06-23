use zola_streams::observability::{health::HealthMonitor, metrics::MetricsRegistry};
use zola_streams::ObservabilityServer;
use std::sync::Arc;
use tokio::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing Observability Server...");
    
    // Initialize observability components
    let health_monitor = Arc::new(HealthMonitor::new());
    let metrics_registry = Arc::new(MetricsRegistry::new());
    
    println!("✅ Observability components created");
    
    // Create and start the observability server on a test port
    let server = ObservabilityServer::new(
        health_monitor.clone(),
        metrics_registry.clone(),
        3030, // Test port
    );
    
    println!("✅ Observability server created on port 3030");
    
    // Start server in background
    let server_handle = tokio::spawn(async move {
        server.start().await
    });
    
    // Wait a moment for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Test the endpoints
    let client = reqwest::Client::new();
    
    // Test health endpoint
    println!("🔍 Testing /health endpoint...");
    match client.get("http://localhost:3030/health").send().await {
        Ok(response) => {
            println!("✅ Health endpoint responded with status: {}", response.status());
            if let Ok(body) = response.text().await {
                println!("📋 Health response: {}", &body[..200.min(body.len())]);
            }
        }
        Err(e) => println!("❌ Health endpoint failed: {}", e),
    }
    
    // Test metrics endpoint
    println!("🔍 Testing /metrics endpoint...");
    match client.get("http://localhost:3030/metrics").send().await {
        Ok(response) => {
            println!("✅ Metrics endpoint responded with status: {}", response.status());
            if let Ok(body) = response.text().await {
                println!("📊 Metrics response: {}", &body[..200.min(body.len())]);
            }
        }
        Err(e) => println!("❌ Metrics endpoint failed: {}", e),
    }
    
    // Test readiness endpoint
    println!("🔍 Testing /ready endpoint...");
    match client.get("http://localhost:3030/ready").send().await {
        Ok(response) => {
            println!("✅ Ready endpoint responded with status: {}", response.status());
        }
        Err(e) => println!("❌ Ready endpoint failed: {}", e),
    }
    
    // Test liveness endpoint
    println!("🔍 Testing /live endpoint...");
    match client.get("http://localhost:3030/live").send().await {
        Ok(response) => {
            println!("✅ Live endpoint responded with status: {}", response.status());
        }
        Err(e) => println!("❌ Live endpoint failed: {}", e),
    }
    
    // Test version endpoint
    println!("🔍 Testing /version endpoint...");
    match client.get("http://localhost:3030/version").send().await {
        Ok(response) => {
            println!("✅ Version endpoint responded with status: {}", response.status());
            if let Ok(body) = response.text().await {
                println!("🏷️  Version response: {}", body);
            }
        }
        Err(e) => println!("❌ Version endpoint failed: {}", e),
    }
    
    println!("🎉 Observability server test completed!");
    
    // Shutdown server
    server_handle.abort();
    
    Ok(())
}
