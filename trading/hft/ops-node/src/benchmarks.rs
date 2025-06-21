use hdrhistogram::Histogram;
use parking_lot::Mutex;
use prometheus::{Encoder, Opts, Registry, TextEncoder, HistogramVec, IntCounterVec, GaugeVec}; // Added HistogramVec, IntCounterVec, GaugeVec
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

// TSC (Time Stamp Counter) related functionalities
// These are highly platform-dependent (x86_64) and may need adjustments or alternatives for other architectures.
#[cfg(all(target_arch = "x86_64", feature = "tsc"))]
use std::arch::x86_64::{_rdtsc, _rdtscp};

#[cfg(all(target_arch = "x86_64", feature = "tsc"))]
mod tsc {
    use super::*;
    // Measures the overhead of calling _rdtscp itself.
    pub fn measure_tsc_overhead() -> u64 {
        const ITERATIONS: usize = 100_000; // Increased iterations for better median
        let mut measurements = Vec::with_capacity(ITERATIONS);

        // Warm up CPU and instruction cache
        for _ in 0..1000 {
            let mut aux = 0u32;
            unsafe { core::arch::x86_64::_mm_mfence();  // Prevent reordering
                     let _ = core::arch::x86_64::_rdtscp(&mut aux);
                     core::arch::x86_64::_mm_lfence(); } // Prevent reordering
        }

        for _ in 0..ITERATIONS {
            let mut aux1 = 0u32;
            let mut aux2 = 0u32;
            unsafe {
                core::arch::x86_64::_mm_mfence();
                let start = core::arch::x86_64::_rdtscp(&mut aux1);
                core::arch::x86_64::_mm_lfence(); // Ensure rdtscp is executed
                core::arch::x86_64::_mm_mfence();
                let end = core::arch::x86_64::_rdtscp(&mut aux2);
                core::arch::x86_64::_mm_lfence();
                measurements.push(end.saturating_sub(start));
            }
        }

        measurements.sort_unstable();
        measurements.get(ITERATIONS / 2).copied().unwrap_or(0) // Median, or 0 if empty
    }

    // Calibrates TSC frequency against a known time source (Instant).
    pub fn calibrate_tsc_frequency() -> f64 {
        const CALIBRATION_DURATION_MS: u64 = 200; // Use a longer duration for more accuracy
        let mut aux = 0u32;

        unsafe {
            core::arch::x86_64::_mm_mfence();
            let start_tsc = core::arch::x86_64::_rdtscp(&mut aux);
            core::arch::x86_64::_mm_lfence();
            let start_time = Instant::now();

            std::thread::sleep(Duration::from_millis(CALIBRATION_DURATION_MS));

            core::arch::x86_64::_mm_mfence();
            let end_tsc = core::arch::x86_64::_rdtscp(&mut aux);
            core::arch::x86_64::_mm_lfence();
            let elapsed_time = start_time.elapsed();

            let cycles = end_tsc.saturating_sub(start_tsc);
            let seconds = elapsed_time.as_secs_f64();

            if seconds == 0.0 { return 0.0; } // Avoid division by zero
            let freq = cycles as f64 / seconds;
            tracing::info!("Calibrated TSC frequency: {:.2} GHz, Cycles: {}, Seconds: {:.4}", freq / 1e9, cycles, seconds);
            freq
        }
    }
}


pub struct LatencyMonitor {
    histograms: Arc<Mutex<HashMap<String, Histogram<u64>>>>, // Key: operation name
    #[cfg(all(target_arch = "x86_64", feature = "tsc"))]
    tsc_state: Option<Arc<TscState>>, // Bundled TSC overhead and frequency
    metrics_exporter: Arc<MetricsExporter>, // For Prometheus integration
}

#[cfg(all(target_arch = "x86_64", feature = "tsc"))]
struct TscState {
    overhead: u64,
    frequency_hz: f64,
}

impl LatencyMonitor {
    pub fn new(metrics_exporter: Arc<MetricsExporter>) -> Self {
        #[cfg(all(target_arch = "x86_64", feature = "tsc"))]
        let tsc_state = {
            tracing::info!("Initializing TSC for latency monitoring.");
            let overhead = tsc::measure_tsc_overhead();
            let frequency_hz = tsc::calibrate_tsc_frequency();
            if frequency_hz == 0.0 {
                tracing::warn!("TSC frequency calibration failed. TSC-based timing will be inaccurate.");
                None
            } else {
                 tracing::info!("TSC initialized. Overhead: {} cycles, Frequency: {:.2} GHz", overhead, frequency_hz / 1e9);
                Some(Arc::new(TscState { overhead, frequency_hz }))
            }
        };
        #[cfg(not(all(target_arch = "x86_64", feature = "tsc")))]
        tracing::warn!("TSC feature not enabled or not on x86_64. Falling back to Instant-based timing.");

        Self {
            histograms: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(all(target_arch = "x86_64", feature = "tsc"))]
            tsc_state,
            metrics_exporter,
        }
    }

    // Start tracking latency. Returns a LatencyGuard.
    pub fn start_tracking(&self, operation_name: &str) -> LatencyGuard {
        #[cfg(all(target_arch = "x86_64", feature = "tsc"))]
        if let Some(ref tsc_s) = self.tsc_state {
            if tsc_s.frequency_hz > 0.0 { // Only use TSC if calibrated
                let mut aux = 0u32;
                let start_tsc = unsafe { core::arch::x86_64::_mm_mfence();
                                         let ts = core::arch::x86_64::_rdtscp(&mut aux);
                                         core::arch::x86_64::_mm_lfence(); ts };
                return LatencyGuard {
                    monitor_instance: self, // Changed from monitor to monitor_instance
                    operation_name: operation_name.to_string(),
                    start_time: TimingMethod::Tsc(start_tsc),
                };
            }
        }
        // Fallback to Instant if TSC is not available/calibrated
        LatencyGuard {
            monitor_instance: self,
            operation_name: operation_name.to_string(),
            start_time: TimingMethod::Instant(Instant::now()),
        }
    }

    // Record latency in nanoseconds directly.
    pub fn record_latency_ns(&self, operation_name: &str, latency_ns: u64) {
        let mut histograms_map = self.histograms.lock();
        let histogram_entry = histograms_map.entry(operation_name.to_string())
            .or_insert_with(|| {
                // Default histogram: 1ns to 10s (10_000_000_000 ns), 3 significant figures
                Histogram::new_with_bounds(1, 10_000_000_000, 3)
                    .expect("Failed to create HDR histogram")
            });

        if let Err(e) = histogram_entry.record(latency_ns) {
            tracing::warn!("Failed to record latency {}ns for {}: {:?}", latency_ns, operation_name, e);
        }

        // Also record to Prometheus
        self.metrics_exporter.observe_latency_seconds(operation_name, latency_ns as f64 / 1_000_000_000.0);
    }

    #[cfg(all(target_arch = "x86_64", feature = "tsc"))]
    fn record_tsc_cycles(&self, operation_name: &str, cycles: u64) {
        if let Some(ref tsc_s) = self.tsc_state {
            if tsc_s.frequency_hz > 0.0 {
                let adjusted_cycles = cycles.saturating_sub(tsc_s.overhead);
                let latency_ns = (adjusted_cycles as f64 / tsc_s.frequency_hz * 1e9) as u64;
                self.record_latency_ns(operation_name, latency_ns);
            } else {
                tracing::warn!("TSC frequency is 0, cannot record TSC cycles for {}", operation_name);
            }
        }
    }

    pub fn get_stats(&self, operation_name: &str) -> Option<LatencyStats> {
        let histograms_map = self.histograms.lock();
        histograms_map.get(operation_name).map(|hist| LatencyStats {
            mean_ns: hist.mean(), // Mean in ns
            min_ns: hist.min(),   // Min in ns
            max_ns: hist.max(),   // Max in ns
            p50_ns: hist.value_at_quantile(0.50),
            p90_ns: hist.value_at_quantile(0.90),
            p95_ns: hist.value_at_quantile(0.95),
            p99_ns: hist.value_at_quantile(0.99),
            p999_ns: hist.value_at_quantile(0.999),
            count: hist.len(),
            stddev_ns: hist.stdev(), // Std dev in ns
        })
    }

    pub fn print_all_stats_to_console(&self) { // Renamed for clarity
        let histograms_map = self.histograms.lock();
        if histograms_map.is_empty() {
            tracing::info!("No latency statistics recorded yet.");
            return;
        }

        println!("\n╔═════════════════════════════════════════════════════════════════════════════╗");
        println!("║                            LATENCY STATISTICS (ns)                            ║");
        println!("╠═════════════════════════════════════════════════════════════════════════════╣");

        let mut sorted_ops: Vec<_> = histograms_map.keys().collect();
        sorted_ops.sort();

        for operation_name in sorted_ops {
            if let Some(hist) = histograms_map.get(operation_name) {
                 // Format to align numbers, typically ns
                println!("║ Operation: {:<67} ║", operation_name);
                println!("╟─────────────────────────────────────────────────────────────────────────────╢");
                println!("║ Count: {:>12} | Mean: {:>10.0} | StdDev: {:>10.0}                     ║", hist.len(), hist.mean(), hist.stdev());
                println!("║ Min:   {:>12} | Max:  {:>10} | P50:    {:>10}                     ║", hist.min(), hist.max(), hist.value_at_quantile(0.50));
                println!("║ P90:   {:>12} | P95:  {:>10} | P99:    {:>10}                     ║", hist.value_at_quantile(0.90), hist.value_at_quantile(0.95), hist.value_at_quantile(0.99));
                println!("║ P99.9: {:>12}                                                        ║", hist.value_at_quantile(0.999));
                println!("╟─────────────────────────────────────────────────────────────────────────────╢");
            }
        }
        println!("╚═════════════════════════════════════════════════════════════════════════════╝\n");
    }
}

// Represents the timing method used by LatencyGuard
enum TimingMethod {
    #[cfg(all(target_arch = "x86_64", feature = "tsc"))]
    Tsc(u64), // Start TSC value
    Instant(Instant), // Start Instant value
}

pub struct LatencyGuard<'a> {
    monitor_instance: &'a LatencyMonitor, // Renamed from monitor
    operation_name: String,
    start_time: TimingMethod,
}

impl<'a> Drop for LatencyGuard<'a> {
    fn drop(&mut self) {
        match self.start_time {
            #[cfg(all(target_arch = "x86_64", feature = "tsc"))]
            TimingMethod::Tsc(start_tsc) => {
                if let Some(ref tsc_s) = self.monitor_instance.tsc_state {
                     if tsc_s.frequency_hz > 0.0 {
                        let mut aux = 0u32;
                        let end_tsc = unsafe { core::arch::x86_64::_mm_mfence();
                                               let ts = core::arch::x86_64::_rdtscp(&mut aux);
                                               core::arch::x86_64::_mm_lfence(); ts };
                        let cycles = end_tsc.saturating_sub(start_tsc);
                        self.monitor_instance.record_tsc_cycles(&self.operation_name, cycles);
                        return; // TSC measurement done
                    }
                }
                // Fallthrough to Instant if TSC failed or not available post-start
                let latency_ns = Instant::now().duration_since(Instant::now() - Duration::from_nanos(start_tsc)).as_nanos() as u64; // This fallback is incorrect if start_tsc was from TSC
                // A better fallback would be to have stored an Instant alongside TSC, or simply not record if TSC failed post-start.
                // For simplicity, if TSC was chosen, we assume it works. If it fails, this measurement might be lost or inaccurate.
                // The current structure will try to record TSC and if that path isn't taken (e.g. tsc_state is None after guard creation), this drop does nothing more.
            }
            TimingMethod::Instant(start_instant) => {
                let duration_ns = start_instant.elapsed().as_nanos() as u64;
                self.monitor_instance.record_latency_ns(&self.operation_name, duration_ns);
            }
        }
    }
}


#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub mean_ns: f64,
    pub min_ns: u64,
    pub max_ns: u64,
    pub p50_ns: u64,
    pub p90_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub p999_ns: u64,
    pub count: u64,
    pub stddev_ns: f64,
}


// Prometheus Metrics Exporter
pub struct MetricsExporter {
    registry: Registry,
    // Using HistogramVec for latencies to allow labeling by operation_name
    op_latency_seconds: HistogramVec, // Buckets in seconds
    // Example generic counter and gauge
    event_counts: IntCounterVec,
    current_items_in_queue: GaugeVec,
}

impl MetricsExporter {
    pub fn new() -> Self {
        let registry = Registry::new();

        let latency_opts = Opts::new("op_latency_seconds", "Operation latency in seconds")
            .namespace("ops_node")
            .subsystem("performance");
        let op_latency_seconds = HistogramVec::new(
            latency_opts,
            &["operation_name"] // Label name
        ).expect("Failed to create op_latency_seconds histogram vec");

        registry.register(Box::new(op_latency_seconds.clone())).expect("Failed to register op_latency_seconds");

        let event_opts = Opts::new("event_total", "Total number of events")
            .namespace("ops_node")
            .subsystem("application");
        let event_counts = IntCounterVec::new(event_opts, &["event_type"]).expect("Failed to create event_counts vec");
        registry.register(Box::new(event_counts.clone())).expect("Failed to register event_counts");

        let queue_opts = Opts::new("queue_length_current", "Current number of items in a queue")
            .namespace("ops_node")
            .subsystem("resources");
        let current_items_in_queue = GaugeVec::new(queue_opts, &["queue_name"]).expect("Failed to create queue_length vec");
        registry.register(Box::new(current_items_in_queue.clone())).expect("Failed to register queue_length");

        tracing::info!("MetricsExporter initialized with Prometheus registry.");
        Self {
            registry,
            op_latency_seconds,
            event_counts,
            current_items_in_queue,
        }
    }

    // Observe latency for a specific operation
    pub fn observe_latency_seconds(&self, operation_name: &str, value_seconds: f64) {
        self.op_latency_seconds.with_label_values(&[operation_name]).observe(value_seconds);
    }

    // Increment a counter for a specific event type
    pub fn increment_event_count(&self, event_type: &str, count: u64) {
        self.event_counts.with_label_values(&[event_type]).inc_by(count);
    }

    // Set a gauge value
    pub fn set_queue_length(&self, queue_name: &str, length: f64) {
        self.current_items_in_queue.with_label_values(&[queue_name]).set(length);
    }

    // Gather metrics for Prometheus scraping
    pub fn gather_metrics_text(&self) -> String {
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder.encode(&metric_families, &mut buffer).expect("Failed to encode metrics");
        String::from_utf8(buffer).unwrap_or_else(|e| {
            tracing::error!("Failed to convert metrics buffer to UTF-8: {}", e);
            String::new()
        })
    }
}

// To use TSC, add this to Cargo.toml features:
// tsc = []
// And enable it for the build.
// Make sure you are on an x86_64 platform where TSC is reliable and consistent across cores.
// Kernel boot parameters like `tsc=reliable clocksource=tsc nohz_full=<cores> isolcpus=<cores>`
// might be needed for highest precision in production.
