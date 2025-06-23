//! Advanced health monitoring with comprehensive dependency checks
//!
//! Provides detailed health status for:
//! - Kafka connectivity and consumer lag
//! - Circuit breaker states
//! - Memory and resource usage
//! - Processing pipeline health
//! - External dependencies

use crate::error::Result as SdkResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, warn, error};

/// Overall health status of the service
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    /// All systems operational
    Healthy,
    /// Minor issues, service degraded but functional
    Degraded,
    /// Critical issues, service may not function properly
    Unhealthy,
}

/// Detailed health information for a specific component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component name
    pub name: String,
    /// Overall status
    pub status: HealthStatus,
    /// Last check timestamp (Unix epoch seconds)
    pub last_check: u64,
    /// Response time in milliseconds
    pub response_time_ms: Option<f64>,
    /// Additional details about the component
    pub details: HashMap<String, serde_json::Value>,
    /// Error message if unhealthy
    pub error: Option<String>,
}

/// Comprehensive health report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// Overall service status
    pub status: HealthStatus,
    /// Service version
    pub version: String,
    /// Service uptime in seconds
    pub uptime_seconds: u64,
    /// Timestamp of this report
    pub timestamp: String,
    /// Individual component health
    pub components: HashMap<String, ComponentHealth>,
    /// Summary metrics
    pub metrics: HealthMetrics,
}

/// Key health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    /// Memory usage in MB
    pub memory_used_mb: f64,
    /// Peak memory usage in MB
    pub memory_peak_mb: f64,
    /// CPU usage percentage (0-100)
    pub cpu_usage_percent: f64,
    /// Messages processed in last minute
    pub messages_per_minute: u64,
    /// Error rate in last hour (0-1)
    pub error_rate_last_hour: f64,
    /// Consumer lag in messages
    pub consumer_lag: u64,
}

/// Health checker interface for different components
#[async_trait::async_trait]
pub trait HealthChecker: Send + Sync {
    /// Component name
    fn name(&self) -> &str;
    
    /// Perform health check
    async fn check_health(&self) -> ComponentHealth;
    
    /// Check if component is critical for service operation
    fn is_critical(&self) -> bool {
        true
    }
}

/// Kafka health checker
pub struct KafkaHealthChecker {
    // Would contain Kafka client reference in real implementation
    consumer_lag_threshold: u64,
    #[allow(dead_code)]
    connection_timeout: Duration,
}

impl KafkaHealthChecker {
    /// Create new Kafka health checker
    pub fn new(consumer_lag_threshold: u64) -> Self {
        Self {
            consumer_lag_threshold,
            connection_timeout: Duration::from_secs(5),
        }
    }
}

#[async_trait::async_trait]
impl HealthChecker for KafkaHealthChecker {
    fn name(&self) -> &str {
        "kafka"
    }
    
    async fn check_health(&self) -> ComponentHealth {
        let start_time = std::time::Instant::now();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        // Simulate Kafka health check (in real implementation, would check actual Kafka)
        let (status, consumer_lag, error) = self.check_kafka_connection().await;
        
        let response_time = start_time.elapsed().as_millis() as f64;
        let mut details = HashMap::new();
        details.insert("consumer_lag".to_string(), serde_json::Value::from(consumer_lag));
        details.insert("lag_threshold".to_string(), serde_json::Value::from(self.consumer_lag_threshold));
        
        ComponentHealth {
            name: self.name().to_string(),
            status,
            last_check: timestamp,
            response_time_ms: Some(response_time),
            details,
            error,
        }
    }
}

impl KafkaHealthChecker {
    async fn check_kafka_connection(&self) -> (HealthStatus, u64, Option<String>) {
        // Simulate Kafka check - in real implementation:
        // 1. Check connection to brokers
        // 2. Verify consumer group membership
        // 3. Check consumer lag
        // 4. Verify topic accessibility
        
        // For now, simulate based on configuration
        let consumer_lag = 150u64; // Simulated lag
        
        if consumer_lag > self.consumer_lag_threshold {
            (
                HealthStatus::Degraded,
                consumer_lag,
                Some(format!("Consumer lag {} exceeds threshold {}", consumer_lag, self.consumer_lag_threshold))
            )
        } else {
            (HealthStatus::Healthy, consumer_lag, None)
        }
    }
}

/// Circuit breaker health checker
pub struct CircuitBreakerHealthChecker;

#[async_trait::async_trait]
impl HealthChecker for CircuitBreakerHealthChecker {
    fn name(&self) -> &str {
        "circuit_breaker"
    }
    
    async fn check_health(&self) -> ComponentHealth {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        // Simulate circuit breaker check
        let (status, details) = self.check_circuit_breaker_state().await;
        
        ComponentHealth {
            name: self.name().to_string(),
            status,
            last_check: timestamp,
            response_time_ms: Some(1.0), // Circuit breaker checks are fast
            details,
            error: None,
        }
    }
}

impl CircuitBreakerHealthChecker {
    async fn check_circuit_breaker_state(&self) -> (HealthStatus, HashMap<String, serde_json::Value>) {
        let mut details = HashMap::new();
        
        // Simulate circuit breaker state - in real implementation, check actual circuit breaker
        details.insert("state".to_string(), serde_json::Value::from("closed"));
        details.insert("failure_count".to_string(), serde_json::Value::from(0));
        details.insert("success_rate".to_string(), serde_json::Value::from(0.98));
        
        (HealthStatus::Healthy, details)
    }
}

/// System resource health checker
pub struct SystemResourceChecker {
    memory_threshold_mb: f64,
    cpu_threshold_percent: f64,
}

impl SystemResourceChecker {
    /// Create new system resource checker
    pub fn new(memory_threshold_mb: f64, cpu_threshold_percent: f64) -> Self {
        Self {
            memory_threshold_mb,
            cpu_threshold_percent,
        }
    }
}

#[async_trait::async_trait]
impl HealthChecker for SystemResourceChecker {
    fn name(&self) -> &str {
        "system_resources"
    }
    
    fn is_critical(&self) -> bool {
        false // System resources are important but not critical for basic operation
    }
    
    async fn check_health(&self) -> ComponentHealth {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let (status, metrics, error) = self.check_system_resources().await;
        
        ComponentHealth {
            name: self.name().to_string(),
            status,
            last_check: timestamp,
            response_time_ms: Some(5.0), // System checks take a bit longer
            details: metrics,
            error,
        }
    }
}

impl SystemResourceChecker {
    async fn check_system_resources(&self) -> (HealthStatus, HashMap<String, serde_json::Value>, Option<String>) {
        let mut details = HashMap::new();
        
        // Simulate system resource checks - in real implementation, use system APIs
        let memory_used_mb = 245.5;
        let cpu_usage_percent = 15.2;
        
        details.insert("memory_used_mb".to_string(), serde_json::Value::from(memory_used_mb));
        details.insert("memory_threshold_mb".to_string(), serde_json::Value::from(self.memory_threshold_mb));
        details.insert("cpu_usage_percent".to_string(), serde_json::Value::from(cpu_usage_percent));
        details.insert("cpu_threshold_percent".to_string(), serde_json::Value::from(self.cpu_threshold_percent));
        
        let status = if memory_used_mb > self.memory_threshold_mb || cpu_usage_percent > self.cpu_threshold_percent {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };
        
        let error = if status == HealthStatus::Degraded {
            Some("Resource usage exceeds thresholds".to_string())
        } else {
            None
        };
        
        (status, details, error)
    }
}

/// Main health monitor that coordinates all health checkers
pub struct HealthMonitor {
    checkers: Vec<Arc<dyn HealthChecker>>,
    start_time: SystemTime,
    last_report: Arc<RwLock<Option<HealthReport>>>,
}

impl HealthMonitor {
    /// Create new health monitor
    pub fn new() -> Self {
        let mut checkers: Vec<Arc<dyn HealthChecker>> = Vec::new();
        
        // Add default health checkers
        checkers.push(Arc::new(KafkaHealthChecker::new(1000)));
        checkers.push(Arc::new(CircuitBreakerHealthChecker));
        checkers.push(Arc::new(SystemResourceChecker::new(1024.0, 80.0)));
        
        Self {
            checkers,
            start_time: SystemTime::now(),
            last_report: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Add a custom health checker
    pub fn add_checker(&mut self, checker: Arc<dyn HealthChecker>) {
        self.checkers.push(checker);
    }
    
    /// Perform comprehensive health check
    pub async fn check_health(&self) -> SdkResult<HealthReport> {
        debug!("Starting comprehensive health check");
        
        let mut components = HashMap::new();
        let mut overall_status = HealthStatus::Healthy;
        
        // Check all components
        for checker in &self.checkers {
            let component_health = checker.check_health().await;
            
            // Determine overall status based on component status and criticality
            match (&component_health.status, checker.is_critical()) {
                (HealthStatus::Unhealthy, true) => overall_status = HealthStatus::Unhealthy,
                (HealthStatus::Degraded, true) if overall_status == HealthStatus::Healthy => {
                    overall_status = HealthStatus::Degraded;
                }
                (HealthStatus::Unhealthy, false) if overall_status == HealthStatus::Healthy => {
                    overall_status = HealthStatus::Degraded;
                }
                _ => {}
            }
            
            components.insert(checker.name().to_string(), component_health);
        }
        
        let uptime = self.start_time
            .elapsed()
            .unwrap_or_default()
            .as_secs();
        
        let timestamp = chrono::Utc::now().to_rfc3339();
        
        let metrics = self.collect_health_metrics(&components).await;
        
        let report = HealthReport {
            status: overall_status.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: uptime,
            timestamp,
            components,
            metrics,
        };
        
        // Cache the report
        {
            let mut last_report = self.last_report.write().await;
            *last_report = Some(report.clone());
        }
        
        match overall_status {
            HealthStatus::Healthy => debug!("Health check completed: all systems healthy"),
            HealthStatus::Degraded => warn!("Health check completed: service degraded"),
            HealthStatus::Unhealthy => error!("Health check completed: service unhealthy"),
        }
        
        Ok(report)
    }
    
    /// Get the last health report (cached)
    pub async fn get_last_report(&self) -> Option<HealthReport> {
        let last_report = self.last_report.read().await;
        last_report.clone()
    }
    
    /// Collect health metrics from component reports
    async fn collect_health_metrics(&self, components: &HashMap<String, ComponentHealth>) -> HealthMetrics {
        // In real implementation, collect actual metrics from monitoring systems
        HealthMetrics {
            memory_used_mb: 245.5,
            memory_peak_mb: 350.0,
            cpu_usage_percent: 15.2,
            messages_per_minute: 1250,
            error_rate_last_hour: 0.001,
            consumer_lag: components
                .get("kafka")
                .and_then(|k| k.details.get("consumer_lag"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        }
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_kafka_health_checker() {
        let checker = KafkaHealthChecker::new(500);
        let health = checker.check_health().await;
        
        assert_eq!(health.name, "kafka");
        assert!(health.response_time_ms.is_some());
        assert!(health.details.contains_key("consumer_lag"));
    }
    
    #[tokio::test]
    async fn test_health_monitor() {
        let monitor = HealthMonitor::new();
        let report = monitor.check_health().await.unwrap();
        
        assert!(!report.version.is_empty());
        // uptime_seconds is u64, always >= 0, so this assertion is removed
        assert!(!report.components.is_empty());
        assert!(report.components.contains_key("kafka"));
        assert!(report.components.contains_key("circuit_breaker"));
        assert!(report.components.contains_key("system_resources"));
    }
    
    #[test]
    fn test_health_status_serialization() {
        let status = HealthStatus::Healthy;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Healthy"));
        
        let deserialized: HealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }
}
