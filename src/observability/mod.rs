//! Observability module for comprehensive monitoring, logging, and tracing
//!
//! This module provides:
//! - Advanced health checks with dependency monitoring
//! - Distributed tracing with OpenTelemetry
//! - Enhanced metrics with business context
//! - Structured logging with correlation IDs
//! - Performance monitoring and alerting

/// Health monitoring with dependency checks
pub mod health;

/// Distributed tracing and spans
pub mod tracing;

/// Enhanced metrics collection and reporting
pub mod metrics;

/// Structured logging with correlation
pub mod logging;

pub use health::*;
pub use tracing::*;
pub use metrics::*;
pub use logging::*;
