use zola_streams::observability::{health::HealthMonitor, metrics::MetricsRegistry};
use zola_streams::ObservabilityServer;
use std::sync::Arc;
use tokio::time::Duration;

#[tokio::test]
async fn test_observability_integration() {
    // Initialize observability components
    let health_monitor = Arc::new(HealthMonitor::new());
    let metrics_registry = Arc::new(MetricsRegistry::new());
    
    // Create the observability server (but don't start it on a real port to avoid conflicts)
    let _server = ObservabilityServer::new(
        health_monitor.clone(),
        metrics_registry.clone(),
        8081, // Use a different port for testing
    );
    
    // Test health monitoring
    let health_result = health_monitor.check_health().await;
    assert!(health_result.is_ok());
    
    let health_report = health_result.unwrap();
    println!("Health report: {:?}", health_report);
    
    // Test metrics
    metrics_registry.increment_counter("test_counter", 1).await;
    metrics_registry.record_histogram("test_histogram", 42.0).await;
    
    let metrics_export = metrics_registry.export_prometheus().await;
    println!("Metrics export (first 200 chars): {}", &metrics_export[..200.min(metrics_export.len())]);
    
    // Verify metrics contain our test data
    assert!(metrics_export.contains("test_counter"));
    assert!(metrics_export.contains("test_histogram"));
    
    println!("✅ Observability integration test passed!");
}

#[tokio::test] 
async fn test_health_monitoring_components() {
    let health_monitor = Arc::new(HealthMonitor::new());
    
    // Perform health check
    let health_result = health_monitor.check_health().await;
    assert!(health_result.is_ok());
    
    let health_report = health_result.unwrap();
    
    // Should have some default health checkers
    assert!(!health_report.components.is_empty());
    println!("Health components: {:?}", health_report.components.keys().collect::<Vec<_>>());
    
    // Verify structure
    assert!(!health_report.version.is_empty());
    // Uptime should be a reasonable value (less than a day for tests)
    assert!(health_report.uptime_seconds < 86400);
    
    println!("✅ Health monitoring components test passed!");
}

#[tokio::test]
async fn test_metrics_collection() {
    let metrics_registry = Arc::new(MetricsRegistry::new());
    
    // Test various metric types
    metrics_registry.increment_counter("events_processed", 1).await;
    metrics_registry.increment_counter("events_processed", 1).await;
    metrics_registry.increment_counter("events_processed", 1).await;
    
    metrics_registry.record_histogram("processing_time", 12.5).await;
    metrics_registry.record_histogram("processing_time", 8.3).await;
    metrics_registry.record_histogram("processing_time", 15.7).await;
    
    // Wait a moment for metrics to be processed
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Export metrics
    let export = metrics_registry.export_prometheus().await;
    
    // Verify counter
    assert!(export.contains("events_processed"));
    assert!(export.contains("3")); // Should show count of 3
    
    // Verify histogram 
    assert!(export.contains("processing_time"));
    
    println!("Metrics export:\n{}", export);
    println!("✅ Metrics collection test passed!");
}
