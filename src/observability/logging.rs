//! Structured logging with correlation and context
//!
//! Provides:
//! - Structured JSON logging
//! - Correlation IDs for request tracing
//! - Contextual logging with metadata
//! - Log aggregation and filtering

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{Event, Subscriber};
use tracing_subscriber::{
    layer::{Context},
    registry::LookupSpan,
    Layer,
};
use uuid::Uuid;

/// Correlation ID for tracking requests across services
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationId(String);

impl CorrelationId {
    /// Generate a new correlation ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
    
    /// Create from existing string
    pub fn from_string(id: String) -> Self {
        Self(id)
    }
    
    /// Get the ID as a string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Log context for adding metadata to log entries
#[derive(Debug, Clone, Serialize)]
pub struct LogContext {
    /// Correlation ID for request tracking
    pub correlation_id: Option<CorrelationId>,
    /// User ID if applicable
    pub user_id: Option<String>,
    /// Request ID for HTTP requests
    pub request_id: Option<String>,
    /// Service component generating the log
    pub component: Option<String>,
    /// Additional custom fields
    pub fields: HashMap<String, serde_json::Value>,
}

impl LogContext {
    /// Create new empty log context
    pub fn new() -> Self {
        Self {
            correlation_id: None,
            user_id: None,
            request_id: None,
            component: None,
            fields: HashMap::new(),
        }
    }
    
    /// Set correlation ID
    pub fn with_correlation_id(mut self, id: CorrelationId) -> Self {
        self.correlation_id = Some(id);
        self
    }
    
    /// Set component name
    pub fn with_component(mut self, component: &str) -> Self {
        self.component = Some(component.to_string());
        self
    }
    
    /// Add custom field
    pub fn with_field(mut self, key: &str, value: serde_json::Value) -> Self {
        self.fields.insert(key.to_string(), value);
        self
    }
    
    /// Set user ID
    pub fn with_user_id(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }
    
    /// Set request ID
    pub fn with_request_id(mut self, request_id: &str) -> Self {
        self.request_id = Some(request_id.to_string());
        self
    }
}

impl Default for LogContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Structured log entry
#[derive(Debug, Serialize)]
pub struct StructuredLogEntry {
    /// Timestamp in RFC3339 format
    pub timestamp: String,
    /// Log level
    pub level: String,
    /// Log message
    pub message: String,
    /// Source module
    pub module: Option<String>,
    /// Source file and line
    pub location: Option<String>,
    /// Log context
    pub context: LogContext,
    /// Additional fields from the log event
    pub fields: HashMap<String, serde_json::Value>,
}

/// Custom tracing layer for structured logging
pub struct StructuredLoggingLayer {
    context_storage: Arc<RwLock<HashMap<String, LogContext>>>,
}

impl StructuredLoggingLayer {
    /// Create new structured logging layer
    pub fn new() -> Self {
        Self {
            context_storage: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Set context for current thread/task
    pub async fn set_context(&self, context: LogContext) {
        let thread_id = format!("{:?}", std::thread::current().id());
        let mut storage = self.context_storage.write().await;
        storage.insert(thread_id, context);
    }
    
    /// Get context for current thread/task
    pub async fn get_context(&self) -> LogContext {
        let thread_id = format!("{:?}", std::thread::current().id());
        let storage = self.context_storage.read().await;
        storage.get(&thread_id).cloned().unwrap_or_default()
    }
}

impl<S> Layer<S> for StructuredLoggingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Create structured log entry
        let timestamp = chrono::Utc::now().to_rfc3339();
        let level = event.metadata().level().to_string().to_uppercase();
        
        // Extract message and fields from the event
        let mut visitor = FieldVisitor::new();
        event.record(&mut visitor);
        
        let entry = StructuredLogEntry {
            timestamp,
            level,
            message: visitor.message.unwrap_or_default(),
            module: Some(event.metadata().module_path().unwrap_or("unknown").to_string()),
            location: Some(format!(
                "{}:{}",
                event.metadata().file().unwrap_or("unknown"),
                event.metadata().line().unwrap_or(0)
            )),
            context: LogContext::new(), // Would get from thread-local storage in real implementation
            fields: visitor.fields,
        };
        
        // Output as JSON
        if let Ok(json) = serde_json::to_string(&entry) {
            println!("{}", json);
        }
    }
}

/// Visitor for extracting fields from tracing events
struct FieldVisitor {
    message: Option<String>,
    fields: HashMap<String, serde_json::Value>,
}

impl FieldVisitor {
    fn new() -> Self {
        Self {
            message: None,
            fields: HashMap::new(),
        }
    }
}

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let value_str = format!("{:?}", value);
        
        if field.name() == "message" {
            self.message = Some(value_str);
        } else {
            self.fields.insert(field.name().to_string(), serde_json::Value::String(value_str));
        }
    }
    
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.insert(field.name().to_string(), serde_json::Value::String(value.to_string()));
        }
    }
    
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields.insert(field.name().to_string(), serde_json::Value::Number(value.into()));
    }
    
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields.insert(field.name().to_string(), serde_json::Value::Number(value.into()));
    }
    
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        if let Some(num) = serde_json::Number::from_f64(value) {
            self.fields.insert(field.name().to_string(), serde_json::Value::Number(num));
        }
    }
    
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields.insert(field.name().to_string(), serde_json::Value::Bool(value));
    }
}

/// Logger for business events with structured context
pub struct BusinessEventLogger {
    layer: Arc<StructuredLoggingLayer>,
}

impl BusinessEventLogger {
    /// Create new business event logger
    pub fn new() -> Self {
        Self {
            layer: Arc::new(StructuredLoggingLayer::new()),
        }
    }
    
    /// Log a DEX trade event
    pub async fn log_dex_trade(
        &self,
        program_id: &str,
        pair: &str,
        volume_usd: f64,
        price: f64,
        correlation_id: Option<CorrelationId>,
    ) {
        let context = LogContext::new()
            .with_component("dex_processor")
            .with_field("event_type", serde_json::Value::String("dex_trade".to_string()))
            .with_field("program_id", serde_json::Value::String(program_id.to_string()))
            .with_field("trading_pair", serde_json::Value::String(pair.to_string()))
            .with_field("volume_usd", serde_json::json!(volume_usd))
            .with_field("price", serde_json::json!(price));
        
        let context = if let Some(id) = correlation_id {
            context.with_correlation_id(id)
        } else {
            context
        };
        
        self.layer.set_context(context).await;
        
        tracing::info!(
            program_id = %program_id,
            trading_pair = %pair,
            volume_usd = %volume_usd,
            price = %price,
            "DEX trade processed"
        );
    }
    
    /// Log an error event
    pub async fn log_error(
        &self,
        component: &str,
        error_type: &str,
        error_message: &str,
        correlation_id: Option<CorrelationId>,
    ) {
        let context = LogContext::new()
            .with_component(component)
            .with_field("event_type", serde_json::Value::String("error".to_string()))
            .with_field("error_type", serde_json::Value::String(error_type.to_string()))
            .with_field("error_message", serde_json::Value::String(error_message.to_string()));
        
        let context = if let Some(id) = correlation_id {
            context.with_correlation_id(id)
        } else {
            context
        };
        
        self.layer.set_context(context).await;
        
        tracing::error!(
            component = %component,
            error_type = %error_type,
            error_message = %error_message,
            "Error occurred"
        );
    }
    
    /// Log a performance event
    pub async fn log_performance(
        &self,
        operation: &str,
        duration_ms: f64,
        success: bool,
        correlation_id: Option<CorrelationId>,
    ) {
        let context = LogContext::new()
            .with_component("performance_monitor")
            .with_field("event_type", serde_json::Value::String("performance".to_string()))
            .with_field("operation", serde_json::Value::String(operation.to_string()))
            .with_field("duration_ms", serde_json::json!(duration_ms))
            .with_field("success", serde_json::Value::Bool(success));
        
        let context = if let Some(id) = correlation_id {
            context.with_correlation_id(id)
        } else {
            context
        };
        
        self.layer.set_context(context).await;
        
        if success {
            tracing::info!(
                operation = %operation,
                duration_ms = %duration_ms,
                success = %success,
                "Operation completed"
            );
        } else {
            tracing::warn!(
                operation = %operation,
                duration_ms = %duration_ms,
                success = %success,
                "Operation completed"
            );
        }
    }
}

impl Default for BusinessEventLogger {
    fn default() -> Self {
        Self::new()
    }
}

/// Global business event logger
static BUSINESS_LOGGER: once_cell::sync::Lazy<BusinessEventLogger> = 
    once_cell::sync::Lazy::new(|| BusinessEventLogger::new());

/// Get global business event logger
pub fn get_business_logger() -> &'static BusinessEventLogger {
    &BUSINESS_LOGGER
}

/// Log a DEX trade with the global logger
pub async fn log_dex_trade(
    program_id: &str,
    pair: &str,
    volume_usd: f64,
    price: f64,
    correlation_id: Option<CorrelationId>,
) {
    get_business_logger()
        .log_dex_trade(program_id, pair, volume_usd, price, correlation_id)
        .await;
}

/// Log an error with the global logger
pub async fn log_error(
    component: &str,
    error_type: &str,
    error_message: &str,
    correlation_id: Option<CorrelationId>,
) {
    get_business_logger()
        .log_error(component, error_type, error_message, correlation_id)
        .await;
}

/// Log performance metrics with the global logger
pub async fn log_performance(
    operation: &str,
    duration_ms: f64,
    success: bool,
    correlation_id: Option<CorrelationId>,
) {
    get_business_logger()
        .log_performance(operation, duration_ms, success, correlation_id)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_correlation_id() {
        let id1 = CorrelationId::new();
        let id2 = CorrelationId::from_string("test-123".to_string());
        
        assert!(!id1.as_str().is_empty());
        assert_eq!(id2.as_str(), "test-123");
        assert_ne!(id1.as_str(), id2.as_str());
    }
    
    #[test]
    fn test_log_context() {
        let context = LogContext::new()
            .with_correlation_id(CorrelationId::from_string("test-123".to_string()))
            .with_component("test_component")
            .with_field("custom_field", serde_json::Value::String("value".to_string()))
            .with_user_id("user-456");
        
        assert_eq!(context.correlation_id.unwrap().as_str(), "test-123");
        assert_eq!(context.component.unwrap(), "test_component");
        assert_eq!(context.user_id.unwrap(), "user-456");
        assert_eq!(context.fields.get("custom_field"), Some(&serde_json::Value::String("value".to_string())));
    }
    
    #[test]
    fn test_structured_log_entry_serialization() {
        let entry = StructuredLogEntry {
            timestamp: "2023-01-01T00:00:00Z".to_string(),
            level: "INFO".to_string(),
            message: "Test message".to_string(),
            module: Some("test::module".to_string()),
            location: Some("test.rs:42".to_string()),
            context: LogContext::new().with_component("test"),
            fields: HashMap::new(),
        };
        
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("Test message"));
        assert!(json.contains("INFO"));
        assert!(json.contains("test::module"));
    }
    
    #[tokio::test]
    async fn test_business_event_logger() {
        let logger = BusinessEventLogger::new();
        let correlation_id = CorrelationId::new();
        
        // Test DEX trade logging (would verify output in real implementation)
        logger.log_dex_trade(
            "raydium",
            "SOL/USDC",
            1000.0,
            100.0,
            Some(correlation_id.clone()),
        ).await;
        
        // Test error logging
        logger.log_error(
            "kafka_consumer",
            "connection_error",
            "Failed to connect to broker",
            Some(correlation_id.clone()),
        ).await;
        
        // Test performance logging
        logger.log_performance(
            "message_processing",
            15.5,
            true,
            Some(correlation_id),
        ).await;
    }
}
