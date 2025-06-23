//! HTTP server for observability endpoints
//!
//! Provides REST endpoints for:
//! - Health checks (`/health`)
//! - Metrics export (`/metrics`)
//! - Readiness checks (`/ready`)
//! - Liveness checks (`/live`)

use crate::observability::{
    health::{HealthMonitor, HealthStatus}, 
    metrics::MetricsRegistry,
};
use serde_json::json;
use std::convert::Infallible;
use std::sync::Arc;
use tracing::{info, debug};
use warp::{Filter, Reply};

/// HTTP server for observability endpoints
pub struct ObservabilityServer {
    health_monitor: Arc<HealthMonitor>,
    metrics_registry: Arc<MetricsRegistry>,
    port: u16,
}

impl ObservabilityServer {
    /// Create a new observability server
    pub fn new(
        health_monitor: Arc<HealthMonitor>,
        metrics_registry: Arc<MetricsRegistry>,
        port: u16,
    ) -> Self {
        Self {
            health_monitor,
            metrics_registry,
            port,
        }
    }

    /// Start the HTTP server
    pub async fn start(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Starting observability HTTP server on port {}", self.port);

        let health_monitor = self.health_monitor.clone();
        let metrics_registry = self.metrics_registry.clone();

        // Health endpoint
        let health = warp::path("health")
            .and(with_health_monitor(health_monitor.clone()))
            .and_then(health_handler);

        // Readiness endpoint (checks if ready to serve traffic)
        let ready = warp::path("ready")
            .and(with_health_monitor(health_monitor.clone()))
            .and_then(readiness_handler);

        // Liveness endpoint (checks if alive and should restart if not)
        let live = warp::path("live")
            .and(with_health_monitor(health_monitor.clone()))
            .and_then(liveness_handler);

        // Metrics endpoint (Prometheus format)
        let metrics = warp::path("metrics")
            .and(with_metrics_registry(metrics_registry.clone()))
            .and_then(metrics_handler);

        // Version endpoint
        let version = warp::path("version")
            .and_then(version_handler);

        // Combine all routes
        let routes = health
            .or(ready)
            .or(live)
            .or(metrics)
            .or(version)
            .with(warp::cors().allow_any_origin())
            .with(warp::log("observability_http"));

        // Start the server
        let addr = ([0, 0, 0, 0], self.port);
        info!("Observability server listening on http://{}.{}.{}.{}:{}", 
               addr.0[0], addr.0[1], addr.0[2], addr.0[3], addr.1);
        
        warp::serve(routes).run(addr).await;
        Ok(())
    }
}

/// Warp filter to inject health monitor
fn with_health_monitor(
    health_monitor: Arc<HealthMonitor>,
) -> impl Filter<Extract = (Arc<HealthMonitor>,), Error = Infallible> + Clone {
    warp::any().map(move || health_monitor.clone())
}

/// Warp filter to inject metrics registry
fn with_metrics_registry(
    metrics_registry: Arc<MetricsRegistry>,
) -> impl Filter<Extract = (Arc<MetricsRegistry>,), Error = Infallible> + Clone {
    warp::any().map(move || metrics_registry.clone())
}

/// Health check handler
async fn health_handler(
    health_monitor: Arc<HealthMonitor>,
) -> Result<impl Reply, warp::Rejection> {
    debug!("Health endpoint requested");
    
    let health_report = health_monitor.get_last_report().await.unwrap_or_else(|| {
        use crate::observability::health::{HealthReport, HealthStatus, HealthMetrics};
        use std::collections::HashMap;
        
        HealthReport {
            status: HealthStatus::Unhealthy,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            components: HashMap::new(),
            metrics: HealthMetrics {
                memory_used_mb: 0.0,
                memory_peak_mb: 0.0,
                cpu_usage_percent: 0.0,
                messages_per_minute: 0,
                error_rate_last_hour: 0.0,
                consumer_lag: 0,
            },
        }
    });
    
    let status_code = match health_report.status {
        HealthStatus::Healthy => warp::http::StatusCode::OK,
        HealthStatus::Degraded => warp::http::StatusCode::OK, // Still serving traffic
        HealthStatus::Unhealthy => warp::http::StatusCode::SERVICE_UNAVAILABLE,
    };

    Ok(warp::reply::with_status(
        warp::reply::json(&health_report),
        status_code,
    ))
}

/// Readiness check handler
async fn readiness_handler(
    health_monitor: Arc<HealthMonitor>,
) -> Result<impl Reply, warp::Rejection> {
    debug!("Readiness endpoint requested");
    
    let health_report = health_monitor.get_last_report().await.unwrap_or_else(|| {
        use crate::observability::health::{HealthReport, HealthStatus, HealthMetrics};
        use std::collections::HashMap;
        
        HealthReport {
            status: HealthStatus::Unhealthy,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            components: HashMap::new(),
            metrics: HealthMetrics {
                memory_used_mb: 0.0,
                memory_peak_mb: 0.0,
                cpu_usage_percent: 0.0,
                messages_per_minute: 0,
                error_rate_last_hour: 0.0,
                consumer_lag: 0,
            },
        }
    });
    
    // Ready means can serve traffic (Healthy or Degraded)
    let is_ready = matches!(health_report.status, HealthStatus::Healthy | HealthStatus::Degraded);
    
    let status_code = if is_ready {
        warp::http::StatusCode::OK
    } else {
        warp::http::StatusCode::SERVICE_UNAVAILABLE
    };

    let response = json!({
        "ready": is_ready,
        "status": health_report.status,
        "timestamp": health_report.timestamp,
        "version": health_report.version,
    });

    Ok(warp::reply::with_status(
        warp::reply::json(&response),
        status_code,
    ))
}

/// Liveness check handler
async fn liveness_handler(
    health_monitor: Arc<HealthMonitor>,
) -> Result<impl Reply, warp::Rejection> {
    debug!("Liveness endpoint requested");
    
    let health_report = health_monitor.get_last_report().await.unwrap_or_else(|| {
        use crate::observability::health::{HealthReport, HealthStatus, HealthMetrics};
        use std::collections::HashMap;
        
        HealthReport {
            status: HealthStatus::Unhealthy,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            components: HashMap::new(),
            metrics: HealthMetrics {
                memory_used_mb: 0.0,
                memory_peak_mb: 0.0,
                cpu_usage_percent: 0.0,
                messages_per_minute: 0,
                error_rate_last_hour: 0.0,
                consumer_lag: 0,
            },
        }
    });
    
    // Live means the application is running (not completely failed)
    // Even degraded services are considered alive
    let is_alive = !matches!(health_report.status, HealthStatus::Unhealthy);
    
    let status_code = if is_alive {
        warp::http::StatusCode::OK
    } else {
        warp::http::StatusCode::SERVICE_UNAVAILABLE
    };

    let response = json!({
        "alive": is_alive,
        "uptime_seconds": health_report.uptime_seconds,
        "timestamp": health_report.timestamp,
        "version": health_report.version,
    });

    Ok(warp::reply::with_status(
        warp::reply::json(&response),
        status_code,
    ))
}

/// Metrics handler (Prometheus format)
async fn metrics_handler(
    metrics_registry: Arc<MetricsRegistry>,
) -> Result<impl Reply, warp::Rejection> {
    debug!("Metrics endpoint requested");
    
    let metrics_text = metrics_registry.export_prometheus().await;
    
    Ok(warp::reply::with_header(
        metrics_text,
        "content-type",
        "text/plain; version=0.0.4; charset=utf-8",
    ))
}

/// Version handler
async fn version_handler() -> Result<impl Reply, warp::Rejection> {
    debug!("Version endpoint requested");
    
    let response = json!({
        "service": "zola-streams",
        "version": env!("CARGO_PKG_VERSION"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
    });

    Ok(warp::reply::json(&response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::health::HealthMonitor;
    use crate::observability::metrics::MetricsRegistry;

    #[tokio::test]
    async fn test_health_endpoint() {
        let _health_monitor = Arc::new(HealthMonitor::new());
        let _metrics_registry = Arc::new(MetricsRegistry::new());
        
        // This would test the endpoint handlers
        // Implementation would depend on testing framework
    }
}
