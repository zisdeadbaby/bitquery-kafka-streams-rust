//! Distributed tracing with OpenTelemetry integration
//!
//! Provides:
//! - Span creation and management
//! - Trace correlation across service boundaries
//! - Performance tracking and bottleneck identification
//! - Integration with Jaeger/Zipkin for visualization

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Convert Instant to microseconds since Unix epoch (approximation)
#[allow(dead_code)]
fn instant_to_micros(_instant: Instant) -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

/// Get current timestamp in microseconds
fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

/// Trace context for correlation across operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Unique trace ID
    pub trace_id: String,
    /// Current span ID
    pub span_id: String,
    /// Parent span ID (if any)
    pub parent_span_id: Option<String>,
    /// Baggage items for cross-cutting concerns
    pub baggage: HashMap<String, String>,
}

/// Span represents a unit of work in a trace
#[derive(Debug, Clone, Serialize)]
pub struct Span {
    /// Span ID
    pub span_id: String,
    /// Parent span ID
    pub parent_span_id: Option<String>,
    /// Trace ID this span belongs to
    pub trace_id: String,
    /// Operation name
    pub operation_name: String,
    /// Start time (Unix timestamp in microseconds)
    pub start_time: u64,
    /// End time (None if still active, Unix timestamp in microseconds)
    pub end_time: Option<u64>,
    /// Span tags/labels
    pub tags: HashMap<String, String>,
    /// Span events/logs
    pub events: Vec<SpanEvent>,
    /// Span status
    pub status: SpanStatus,
}

/// Span event represents a point-in-time occurrence during a span
#[derive(Debug, Clone, Serialize)]
pub struct SpanEvent {
    /// Event name
    pub name: String,
    /// Event timestamp (Unix timestamp in microseconds)
    pub timestamp: u64,
    /// Event attributes
    pub attributes: HashMap<String, String>,
}

/// Span status indicates the outcome of the operation
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum SpanStatus {
    /// Operation completed successfully
    Ok,
    /// Operation failed
    Error,
    /// Operation was cancelled
    Cancelled,
    /// Operation timed out
    Timeout,
}

/// Tracer for creating and managing spans
pub struct Tracer {
    service_name: String,
    version: String,
}

impl Tracer {
    /// Create a new tracer
    pub fn new(service_name: &str, version: &str) -> Self {
        Self {
            service_name: service_name.to_string(),
            version: version.to_string(),
        }
    }
    
    /// Start a new root span
    pub fn start_span(&self, operation_name: &str) -> ActiveSpan {
        let trace_id = Uuid::new_v4().to_string();
        let span_id = Uuid::new_v4().to_string();
        
        let mut tags = HashMap::new();
        tags.insert("service.name".to_string(), self.service_name.clone());
        tags.insert("service.version".to_string(), self.version.clone());
        
        let span = Span {
            span_id: span_id.clone(),
            parent_span_id: None,
            trace_id: trace_id.clone(),
            operation_name: operation_name.to_string(),
            start_time: now_micros(),
            end_time: None,
            tags,
            events: Vec::new(),
            status: SpanStatus::Ok,
        };
        
        ActiveSpan::new(span)
    }
    
    /// Start a child span from existing context
    pub fn start_child_span(&self, parent_context: &TraceContext, operation_name: &str) -> ActiveSpan {
        let span_id = Uuid::new_v4().to_string();
        
        let mut tags = HashMap::new();
        tags.insert("service.name".to_string(), self.service_name.clone());
        tags.insert("service.version".to_string(), self.version.clone());
        
        let span = Span {
            span_id: span_id.clone(),
            parent_span_id: Some(parent_context.span_id.clone()),
            trace_id: parent_context.trace_id.clone(),
            operation_name: operation_name.to_string(),
            start_time: now_micros(),
            end_time: None,
            tags,
            events: Vec::new(),
            status: SpanStatus::Ok,
        };
        
        ActiveSpan::new(span)
    }
}

/// Active span that can be modified and finished
pub struct ActiveSpan {
    span: Span,
    finished: bool,
}

impl ActiveSpan {
    fn new(span: Span) -> Self {
        Self {
            span,
            finished: false,
        }
    }
    
    /// Add a tag to the span
    pub fn set_tag(&mut self, key: &str, value: &str) {
        if !self.finished {
            self.span.tags.insert(key.to_string(), value.to_string());
        }
    }
    
    /// Add an event to the span
    pub fn add_event(&mut self, name: &str, attributes: Option<HashMap<String, String>>) {
        if !self.finished {
            let event = SpanEvent {
                name: name.to_string(),
                timestamp: now_micros(),
                attributes: attributes.unwrap_or_default(),
            };
            self.span.events.push(event);
        }
    }
    
    /// Set span status
    pub fn set_status(&mut self, status: SpanStatus) {
        if !self.finished {
            self.span.status = status;
        }
    }
    
    /// Record an error on the span
    pub fn record_error(&mut self, error: &str) {
        if !self.finished {
            self.set_status(SpanStatus::Error);
            self.set_tag("error", "true");
            self.set_tag("error.message", error);
            
            let mut attributes = HashMap::new();
            attributes.insert("error.message".to_string(), error.to_string());
            self.add_event("error", Some(attributes));
        }
    }
    
    /// Get trace context for propagation
    pub fn context(&self) -> TraceContext {
        TraceContext {
            trace_id: self.span.trace_id.clone(),
            span_id: self.span.span_id.clone(),
            parent_span_id: self.span.parent_span_id.clone(),
            baggage: HashMap::new(),
        }
    }
    
    /// Finish the span
    pub fn finish(mut self) -> FinishedSpan {
        if !self.finished {
            self.span.end_time = Some(now_micros());
            self.finished = true;
        }
        
        FinishedSpan::new(self.span)
    }
    
    /// Get span duration if finished (in microseconds)
    pub fn duration_micros(&self) -> Option<u64> {
        self.span.end_time.map(|end| end.saturating_sub(self.span.start_time))
    }
}

/// Finished span with complete timing information
pub struct FinishedSpan {
    span: Span,
}

impl FinishedSpan {
    fn new(span: Span) -> Self {
        Self { span }
    }
    
    /// Get span duration in microseconds
    pub fn duration_micros(&self) -> u64 {
        self.span.end_time
            .unwrap_or_else(now_micros)
            .saturating_sub(self.span.start_time)
    }
    
    /// Export span to Jaeger format
    pub fn to_jaeger_json(&self) -> serde_json::Value {
        let duration_micros = self.duration_micros();
        let start_time_micros = self.span.start_time;
        
        serde_json::json!({
            "traceID": self.span.trace_id,
            "spanID": self.span.span_id,
            "parentSpanID": self.span.parent_span_id,
            "operationName": self.span.operation_name,
            "startTime": start_time_micros,
            "duration": duration_micros,
            "tags": self.span.tags.iter().map(|(k, v)| {
                serde_json::json!({
                    "key": k,
                    "value": v,
                    "type": "string"
                })
            }).collect::<Vec<_>>(),
            "logs": self.span.events.iter().map(|event| {
                serde_json::json!({
                    "timestamp": event.timestamp,
                    "fields": [
                        {
                            "key": "event",
                            "value": event.name
                        }
                    ]
                })
            }).collect::<Vec<_>>()
        })
    }
}

/// Macro for creating traced functions
#[macro_export]
macro_rules! traced {
    ($tracer:expr, $operation:expr, $code:block) => {{
        let mut span = $tracer.start_span($operation);
        let result = (|| $code)();
        match &result {
            Ok(_) => span.set_status(SpanStatus::Ok),
            Err(e) => span.record_error(&format!("{:?}", e)),
        }
        let _finished_span = span.finish();
        result
    }};
}

/// Async version of traced macro
#[macro_export]
macro_rules! traced_async {
    ($tracer:expr, $operation:expr, $code:block) => {{
        let mut span = $tracer.start_span($operation);
        let result = (|| async move $code)().await;
        match &result {
            Ok(_) => span.set_status(SpanStatus::Ok),
            Err(e) => span.record_error(&format!("{:?}", e)),
        }
        let _finished_span = span.finish();
        result
    }};
}

/// Performance monitoring with automatic span creation
pub struct PerformanceMonitor {
    tracer: Tracer,
}

impl PerformanceMonitor {
    /// Create new performance monitor
    pub fn new(service_name: &str) -> Self {
        Self {
            tracer: Tracer::new(service_name, env!("CARGO_PKG_VERSION")),
        }
    }
    
    /// Time a function execution
    pub fn time_operation<F, R>(&self, operation_name: &str, f: F) -> (R, Duration)
    where
        F: FnOnce() -> R,
    {
        let mut span = self.tracer.start_span(operation_name);
        let start = Instant::now();
        let result = f();
        let duration = start.elapsed();
        
        span.set_tag("duration_ms", &duration.as_millis().to_string());
        span.finish();
        
        (result, duration)
    }
    
    /// Time an async function execution
    pub async fn time_async_operation<F, Fut, R>(&self, operation_name: &str, f: F) -> (R, Duration)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        let mut span = self.tracer.start_span(operation_name);
        let start = Instant::now();
        let result = f().await;
        let duration = start.elapsed();
        
        span.set_tag("duration_ms", &duration.as_millis().to_string());
        span.finish();
        
        (result, duration)
    }
}

/// Global tracer instance
static GLOBAL_TRACER: once_cell::sync::Lazy<Tracer> = 
    once_cell::sync::Lazy::new(|| Tracer::new("zola-streams", env!("CARGO_PKG_VERSION")));

/// Get global tracer
pub fn global_tracer() -> &'static Tracer {
    &GLOBAL_TRACER
}

/// Start a new trace span
pub fn start_span(operation_name: &str) -> ActiveSpan {
    global_tracer().start_span(operation_name)
}

/// Start a child span from context
pub fn start_child_span(parent_context: &TraceContext, operation_name: &str) -> ActiveSpan {
    global_tracer().start_child_span(parent_context, operation_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    
    #[test]
    fn test_span_creation() {
        let tracer = Tracer::new("test-service", "1.0.0");
        let mut span = tracer.start_span("test_operation");
        
        assert_eq!(span.span.operation_name, "test_operation");
        assert_eq!(span.span.status, SpanStatus::Ok);
        assert!(span.span.tags.contains_key("service.name"));
        
        span.set_tag("custom.tag", "value");
        assert_eq!(span.span.tags.get("custom.tag"), Some(&"value".to_string()));
        
        let finished = span.finish();
        assert!(finished.duration_micros() > 0);
    }
    
    #[test]
    fn test_child_span() {
        let tracer = Tracer::new("test-service", "1.0.0");
        let parent_span = tracer.start_span("parent_operation");
        let parent_context = parent_span.context();
        
        let child_span = tracer.start_child_span(&parent_context, "child_operation");
        
        assert_eq!(child_span.span.trace_id, parent_context.trace_id);
        assert_eq!(child_span.span.parent_span_id, Some(parent_context.span_id));
    }
    
    #[test]
    fn test_span_events() {
        let tracer = Tracer::new("test-service", "1.0.0");
        let mut span = tracer.start_span("test_operation");
        
        span.add_event("processing_started", None);
        
        let mut attrs = HashMap::new();
        attrs.insert("record_count".to_string(), "100".to_string());
        span.add_event("records_processed", Some(attrs));
        
        assert_eq!(span.span.events.len(), 2);
        assert_eq!(span.span.events[0].name, "processing_started");
        assert_eq!(span.span.events[1].name, "records_processed");
    }
    
    #[test]
    fn test_error_recording() {
        let tracer = Tracer::new("test-service", "1.0.0");
        let mut span = tracer.start_span("test_operation");
        
        span.record_error("Something went wrong");
        
        assert_eq!(span.span.status, SpanStatus::Error);
        assert_eq!(span.span.tags.get("error"), Some(&"true".to_string()));
        assert_eq!(span.span.tags.get("error.message"), Some(&"Something went wrong".to_string()));
        assert!(!span.span.events.is_empty());
    }
    
    #[test]
    fn test_performance_monitor() {
        let monitor = PerformanceMonitor::new("test-service");
        
        let (result, duration) = monitor.time_operation("slow_operation", || {
            thread::sleep(Duration::from_millis(10));
            42
        });
        
        assert_eq!(result, 42);
        assert!(duration.as_millis() >= 10);
    }
    
    #[tokio::test]
    async fn test_async_performance_monitor() {
        let monitor = PerformanceMonitor::new("test-service");
        
        let (result, duration) = monitor.time_async_operation("async_operation", || async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            "done"
        }).await;
        
        assert_eq!(result, "done");
        assert!(duration.as_millis() >= 10);
    }
}
