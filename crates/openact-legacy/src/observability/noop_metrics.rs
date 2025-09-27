//! No-op metrics implementation
//!
//! This module provides zero-overhead no-op implementations of metrics traits
//! when the `metrics` feature is disabled. All operations are compile-time
//! eliminated, resulting in zero performance impact.

use std::time::Duration;

/// No-op counter implementation
#[derive(Debug, Clone)]
pub struct Counter;

impl Counter {
    /// Increment counter by the given amount (no-op)
    #[inline(always)]
    pub fn increment(&self, _value: u64) {}

    /// Increment counter by 1 (no-op)
    #[inline(always)]
    pub fn inc(&self) {}
}

/// No-op gauge implementation
#[derive(Debug, Clone)]
pub struct Gauge;

impl Gauge {
    /// Set gauge to the given value (no-op)
    #[inline(always)]
    pub fn set(&self, _value: f64) {}

    /// Increment gauge by the given amount (no-op)
    #[inline(always)]
    pub fn increment(&self, _value: f64) {}

    /// Decrement gauge by the given amount (no-op)
    #[inline(always)]
    pub fn decrement(&self, _value: f64) {}
}

/// No-op histogram implementation
#[derive(Debug, Clone)]
pub struct Histogram;

impl Histogram {
    /// Record a value in the histogram (no-op)
    #[inline(always)]
    pub fn record(&self, _value: f64) {}

    /// Record a duration in the histogram (no-op)
    #[inline(always)]
    pub fn record_duration(&self, _duration: Duration) {}
}

/// Create a no-op counter with the given name
#[inline(always)]
pub fn counter(_name: &str) -> Counter {
    Counter
}

/// Create a no-op gauge with the given name
#[inline(always)]
pub fn gauge(_name: &str) -> Gauge {
    Gauge
}

/// Create a no-op histogram with the given name
#[inline(always)]
pub fn histogram(_name: &str) -> Histogram {
    Histogram
}

/// No-op counter macro
#[macro_export]
macro_rules! noop_counter {
    ($name:expr) => {
        $crate::observability::noop_metrics::counter($name)
    };
}

/// No-op gauge macro  
#[macro_export]
macro_rules! noop_gauge {
    ($name:expr) => {
        $crate::observability::noop_metrics::gauge($name)
    };
}

/// No-op histogram macro
#[macro_export]
macro_rules! noop_histogram {
    ($name:expr) => {
        $crate::observability::noop_metrics::histogram($name)
    };
}

// Re-export macros for use when metrics feature is disabled
#[cfg(not(feature = "metrics"))]
pub use noop_counter as counter;
#[cfg(not(feature = "metrics"))]
pub use noop_gauge as gauge;
#[cfg(not(feature = "metrics"))]
pub use noop_histogram as histogram;
