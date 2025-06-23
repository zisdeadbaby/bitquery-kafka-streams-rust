//! Enhanced metrics collection with business context
//!
//! Provides comprehensive metrics for:
//! - Business metrics (trades, volumes, users)
//! - Technical metrics (latency, throughput, errors)
//! - Infrastructure metrics (CPU, memory, network)
//! - Custom application metrics

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::Serialize;

/// Enhanced metrics registry with automatic aggregation
pub struct MetricsRegistry {
    counters: Arc<RwLock<HashMap<String, Arc<AtomicU64>>>>,
    gauges: Arc<RwLock<HashMap<String, Arc<AtomicI64>>>>,
    histograms: Arc<RwLock<HashMap<String, Arc<RwLock<HistogramData>>>>>,
    #[allow(dead_code)]
    labels: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
}

/// Histogram data with percentiles
#[derive(Debug, Clone, Serialize)]
pub struct HistogramData {
    /// Sample values
    pub values: Vec<f64>,
    /// Number of samples
    pub count: u64,
    /// Sum of all values
    pub sum: f64,
}

/// System metrics snapshot
#[derive(Debug, Clone, Serialize)]
pub struct SystemMetrics {
    /// CPU usage percentage
    pub cpu_usage: f64,
    /// Memory usage in megabytes
    pub memory_usage_mb: f64,
    /// Network input/output in bytes per second
    pub network_io: NetworkMetrics,
    /// Process-specific metrics
    pub process: ProcessMetrics,
}

/// Network I/O metrics
#[derive(Debug, Clone, Serialize)]
pub struct NetworkMetrics {
    /// Bytes received per second
    pub bytes_in_per_sec: f64,
    /// Bytes sent per second
    pub bytes_out_per_sec: f64,
    /// Packets received per second
    pub packets_in_per_sec: f64,
    /// Packets sent per second
    pub packets_out_per_sec: f64,
}

/// Trading pair specific metrics
#[derive(Debug, Clone, Serialize)]
pub struct TradingPairMetrics {
    /// Trading pair identifier (e.g., "SOL/USDC")
    pub pair: String,
    /// Total number of trades
    pub trades: u64,
    /// Total volume in USD
    pub volume_usd: f64,
    /// Average trade price
    pub avg_price: f64,
    /// Price change percentage over 24 hours
    pub price_change_24h: f64,
}

/// DEX program metrics
#[derive(Debug, Clone, Serialize)]
pub struct DexProgramMetrics {
    /// Program ID on Solana
    pub program_id: String,
    /// DEX name (e.g., "Raydium", "Orca")
    pub name: String,
    /// Total number of trades
    pub trades: u64,
    /// Total volume in USD
    pub volume_usd: f64,
    /// Market share percentage
    pub market_share: f64,
}

/// Business metrics aggregated view
#[derive(Debug, Clone, Serialize)]
pub struct BusinessMetrics {
    /// Trading pairs metrics
    pub trading_pairs: Vec<TradingPairMetrics>,
    /// DEX programs metrics
    pub dex_programs: Vec<DexProgramMetrics>,
    /// Total trades across all pairs
    pub total_trades: u64,
    /// Total volume in USD
    pub total_volume_usd: f64,
    /// Active users count
    pub active_users: u64,
    /// Unique wallets count
    pub unique_wallets: u64,
}

/// Performance metrics with percentiles
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceMetrics {
    /// 50th percentile latency
    pub p50: f64,
    /// 90th percentile latency
    pub p90: f64,
    /// 95th percentile latency
    pub p95: f64,
    /// 99th percentile latency
    pub p99: f64,
    /// Maximum latency
    pub max: f64,
    /// Average latency
    pub avg: f64,
}

/// JVM-style memory metrics
#[derive(Debug, Clone, Serialize)]
pub struct MemoryMetrics {
    /// Used memory in MB
    pub used_mb: f64,
    /// Peak memory usage in MB
    pub peak_mb: f64,
    /// Garbage collection count
    pub gc_count: u64,
    /// Total GC time in milliseconds
    pub gc_total_time_ms: f64,
}

/// Process-specific metrics
#[derive(Debug, Clone, Serialize)]
pub struct ProcessMetrics {
    /// Process uptime in seconds
    pub uptime_seconds: u64,
    /// Thread count
    pub thread_count: u32,
    /// File descriptor count
    pub fd_count: u32,
    /// Memory metrics
    pub memory: MemoryMetrics,
}

impl MetricsRegistry {
    /// Create new metrics registry
    pub fn new() -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
            gauges: Arc::new(RwLock::new(HashMap::new())),
            histograms: Arc::new(RwLock::new(HashMap::new())),
            labels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Increment counter by value
    pub async fn increment_counter(&self, name: &str, value: u64) {
        let counters = self.counters.read().await;
        if let Some(counter) = counters.get(name) {
            counter.fetch_add(value, Ordering::Relaxed);
        } else {
            drop(counters);
            let mut counters = self.counters.write().await;
            let counter = Arc::new(AtomicU64::new(value));
            counters.insert(name.to_string(), counter);
        }
    }

    /// Set gauge value
    pub async fn set_gauge(&self, name: &str, value: i64) {
        let gauges = self.gauges.read().await;
        if let Some(gauge) = gauges.get(name) {
            gauge.store(value, Ordering::Relaxed);
        } else {
            drop(gauges);
            let mut gauges = self.gauges.write().await;
            let gauge = Arc::new(AtomicI64::new(value));
            gauges.insert(name.to_string(), gauge);
        }
    }

    /// Record histogram value
    pub async fn record_histogram(&self, name: &str, value: f64) {
        let histograms = self.histograms.read().await;
        if let Some(histogram) = histograms.get(name) {
            let mut data = histogram.write().await;
            data.values.push(value);
            data.count += 1;
            data.sum += value;
        } else {
            drop(histograms);
            let mut histograms = self.histograms.write().await;
            let histogram_data = HistogramData {
                values: vec![value],
                count: 1,
                sum: value,
            };
            histograms.insert(name.to_string(), Arc::new(RwLock::new(histogram_data)));
        }
    }

    /// Get counter value
    pub async fn get_counter(&self, name: &str) -> u64 {
        let counters = self.counters.read().await;
        counters.get(name)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Get gauge value
    pub async fn get_gauge(&self, name: &str) -> i64 {
        let gauges = self.gauges.read().await;
        gauges.get(name)
            .map(|g| g.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Get histogram statistics
    pub async fn get_histogram_stats(&self, name: &str) -> Option<PerformanceMetrics> {
        let histograms = self.histograms.read().await;
        if let Some(histogram) = histograms.get(name) {
            let data = histogram.read().await;
            if data.values.is_empty() {
                return None;
            }

            let mut sorted_values = data.values.clone();
            sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            Some(PerformanceMetrics {
                p50: percentile(&sorted_values, 0.5),
                p90: percentile(&sorted_values, 0.9),
                p95: percentile(&sorted_values, 0.95),
                p99: percentile(&sorted_values, 0.99),
                max: *sorted_values.last().unwrap_or(&0.0),
                avg: data.sum / data.count as f64,
            })
        } else {
            None
        }
    }

    /// Export all metrics in Prometheus format
    pub async fn export_prometheus(&self) -> String {
        let mut output = String::new();
        
        // Export counters
        let counters = self.counters.read().await;
        for (name, counter) in counters.iter() {
            let value = counter.load(Ordering::Relaxed);
            output.push_str(&format!("# TYPE {} counter\n", name));
            output.push_str(&format!("{} {}\n", name, value));
        }

        // Export gauges
        let gauges = self.gauges.read().await;
        for (name, gauge) in gauges.iter() {
            let value = gauge.load(Ordering::Relaxed);
            output.push_str(&format!("# TYPE {} gauge\n", name));
            output.push_str(&format!("{} {}\n", name, value));
        }

        // Export histograms
        let histograms = self.histograms.read().await;
        for (name, histogram) in histograms.iter() {
            let data = histogram.read().await;
            output.push_str(&format!("# TYPE {} histogram\n", name));
            output.push_str(&format!("{}_count {}\n", name, data.count));
            output.push_str(&format!("{}_sum {}\n", name, data.sum));
        }

        output
    }

    /// Get system metrics snapshot
    pub async fn get_system_metrics(&self) -> SystemMetrics {
        SystemMetrics {
            cpu_usage: self.get_gauge("cpu_usage_percent").await as f64,
            memory_usage_mb: self.get_gauge("memory_usage_mb").await as f64,
            network_io: NetworkMetrics {
                bytes_in_per_sec: self.get_gauge("network_bytes_in_per_sec").await as f64,
                bytes_out_per_sec: self.get_gauge("network_bytes_out_per_sec").await as f64,
                packets_in_per_sec: self.get_gauge("network_packets_in_per_sec").await as f64,
                packets_out_per_sec: self.get_gauge("network_packets_out_per_sec").await as f64,
            },
            process: ProcessMetrics {
                uptime_seconds: self.get_counter("process_uptime_seconds").await,
                thread_count: self.get_gauge("process_thread_count").await as u32,
                fd_count: self.get_gauge("process_fd_count").await as u32,
                memory: MemoryMetrics {
                    used_mb: self.get_gauge("jvm_memory_used_mb").await as f64,
                    peak_mb: self.get_gauge("jvm_memory_peak_mb").await as f64,
                    gc_count: self.get_counter("jvm_gc_count").await,
                    gc_total_time_ms: self.get_gauge("jvm_gc_total_time_ms").await as f64,
                },
            },
        }
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate percentile from sorted values
fn percentile(sorted_values: &[f64], p: f64) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    
    let index = (p * (sorted_values.len() - 1) as f64) as usize;
    sorted_values.get(index).copied().unwrap_or(0.0)
}

/// DEX Trading Metrics Collector
pub struct DexMetricsCollector {
    registry: Arc<MetricsRegistry>,
    start_time: Instant,
}

impl DexMetricsCollector {
    /// Create new DEX metrics collector
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        Self {
            registry,
            start_time: Instant::now(),
        }
    }

    /// Record a trade event
    pub async fn record_trade(&self, pair: &str, price: f64, volume: f64) {
        // Record general trade metrics
        self.registry.increment_counter("trades_total", 1).await;
        self.registry.record_histogram("trade_size_usd", volume).await;
        self.registry.record_histogram("trade_price", price).await;
        
        // Record pair-specific metrics
        let pair_trades_key = format!("trades_total_pair_{}", pair);
        let pair_volume_key = format!("volume_usd_pair_{}", pair);
        
        self.registry.increment_counter(&pair_trades_key, 1).await;
        self.registry.set_gauge(&pair_volume_key, volume as i64).await;
    }

    /// Record DEX program interaction
    pub async fn record_dex_interaction(&self, program_id: &str, instruction_type: &str) {
        let key = format!("dex_instructions_{}_{}", program_id, instruction_type);
        self.registry.increment_counter(&key, 1).await;
    }

    /// Record processing latency
    pub async fn record_processing_latency(&self, operation: &str, duration: Duration) {
        let key = format!("processing_latency_{}", operation);
        self.registry.record_histogram(&key, duration.as_millis() as f64).await;
    }

    /// Record user activity
    pub async fn record_user_activity(&self, wallet_address: &str) {
        self.registry.increment_counter("unique_users_total", 1).await;
        
        // In a real implementation, we'd use a more sophisticated mechanism
        // to track unique users over time periods
        let daily_key = format!("daily_active_users_{}", wallet_address);
        self.registry.set_gauge(&daily_key, 1).await;
    }

    /// Get business metrics summary
    pub async fn get_business_metrics(&self) -> BusinessMetrics {
        // In a real implementation, this would aggregate actual trading data
        // For now, return simulated data
        BusinessMetrics {
            trading_pairs: vec![
                TradingPairMetrics {
                    pair: "SOL/USDC".to_string(),
                    trades: self.registry.get_counter("trades_total_pair_SOL/USDC").await,
                    volume_usd: 1_250_000.0,
                    avg_price: 89.45,
                    price_change_24h: 3.2,
                },
                TradingPairMetrics {
                    pair: "ETH/SOL".to_string(),
                    trades: self.registry.get_counter("trades_total_pair_ETH/SOL").await,
                    volume_usd: 890_000.0,
                    avg_price: 25.8,
                    price_change_24h: -1.8,
                },
            ],
            dex_programs: vec![
                DexProgramMetrics {
                    program_id: "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(),
                    name: "Raydium".to_string(),
                    trades: 15420,
                    volume_usd: 980_000.0,
                    market_share: 45.8,
                },
                DexProgramMetrics {
                    program_id: "9W959DqEETiGZocYWCQPaJ6sBmUzgfxXfqGeTEdp3aQP".to_string(),
                    name: "Orca".to_string(),
                    trades: 8930,
                    volume_usd: 620_000.0,
                    market_share: 28.9,
                },
            ],
            total_trades: self.registry.get_counter("trades_total").await,
            total_volume_usd: 2_140_000.0,
            active_users: 1250,
            unique_wallets: 3400,
        }
    }

    /// Update system metrics
    pub async fn update_system_metrics(&self) {
        // Simulate system metrics collection
        // In a real implementation, this would collect actual system stats
        
        // CPU usage (simulated)
        let cpu_usage = 45.2;
        self.registry.set_gauge("cpu_usage_percent", cpu_usage as i64).await;
        
        // Memory usage (simulated)
        let memory_usage = 1024.0; // MB
        self.registry.set_gauge("memory_usage_mb", memory_usage as i64).await;
        
        // Process uptime
        let uptime = self.start_time.elapsed().as_secs();
        self.registry.increment_counter("process_uptime_seconds", uptime).await;
        
        // Network I/O (simulated)
        self.registry.set_gauge("network_bytes_in_per_sec", 150_000).await;
        self.registry.set_gauge("network_bytes_out_per_sec", 89_000).await;
        self.registry.set_gauge("network_packets_in_per_sec", 450).await;
        self.registry.set_gauge("network_packets_out_per_sec", 320).await;
        
        // Process metrics (simulated)
        self.registry.set_gauge("process_thread_count", 12).await;
        self.registry.set_gauge("process_fd_count", 128).await;
        
        // JVM-style memory metrics (simulated)
        self.registry.set_gauge("jvm_memory_used_mb", 512).await;
        self.registry.set_gauge("jvm_memory_peak_mb", 768).await;
        self.registry.increment_counter("jvm_gc_count", 1).await;
        self.registry.set_gauge("jvm_gc_total_time_ms", 45).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_registry() {
        let registry = MetricsRegistry::new();
        
        // Test counter
        registry.increment_counter("test_counter", 5).await;
        assert_eq!(registry.get_counter("test_counter").await, 5);
        
        // Test gauge
        registry.set_gauge("test_gauge", -10).await;
        assert_eq!(registry.get_gauge("test_gauge").await, -10);
        
        // Test histogram
        registry.record_histogram("test_histogram", 1.0).await;
        registry.record_histogram("test_histogram", 2.0).await;
        registry.record_histogram("test_histogram", 3.0).await;
        
        let stats = registry.get_histogram_stats("test_histogram").await;
        assert!(stats.is_some());
        let stats = stats.unwrap();
        assert_eq!(stats.avg, 2.0);
        assert_eq!(stats.max, 3.0);
    }

    #[tokio::test]
    async fn test_dex_metrics_collector() {
        let registry = Arc::new(MetricsRegistry::new());
        let collector = DexMetricsCollector::new(registry.clone());
        
        // Record some trades
        collector.record_trade("SOL/USDC", 89.45, 1000.0).await;
        collector.record_dex_interaction("raydium", "swap").await;
        
        // Check metrics were recorded
        assert_eq!(registry.get_counter("trades_total").await, 1);
        assert_eq!(registry.get_counter("dex_instructions_raydium_swap").await, 1);
        
        // Test business metrics
        let business_metrics = collector.get_business_metrics().await;
        assert!(!business_metrics.trading_pairs.is_empty());
        assert!(!business_metrics.dex_programs.is_empty());
    }

    #[tokio::test]
    async fn test_prometheus_export() {
        let registry = MetricsRegistry::new();
        
        registry.increment_counter("http_requests_total", 100).await;
        registry.set_gauge("memory_usage_bytes", 1024).await;
        
        let prometheus_output = registry.export_prometheus().await;
        assert!(prometheus_output.contains("http_requests_total 100"));
        assert!(prometheus_output.contains("memory_usage_bytes 1024"));
    }

    #[test]
    fn test_percentile_calculation() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile(&values, 0.5), 3.0);
        assert_eq!(percentile(&values, 0.9), 4.0); // 90th percentile with floor calculation
        assert_eq!(percentile(&[], 0.5), 0.0);
    }
}
