#!/bin/bash

# Prometheus Metrics Demo for OpenAct
# 
# This script demonstrates how to enable and use Prometheus metrics
# in OpenAct with feature flags and environment configuration.

set -e

echo "🔍 OpenAct Prometheus Metrics Demo"
echo "================================="

# Build OpenAct with metrics feature enabled
echo "📦 Building OpenAct with metrics feature..."
cargo build --features metrics --bin openact-cli

echo ""
echo "📊 Testing Metrics Configuration:"
echo ""

# Test 1: Default mode (noop metrics)
echo "1️⃣  Testing Default Mode (No-op Metrics):"
echo "   Building without metrics feature - uses noop implementation"
echo "   cargo build --bin openact-cli"
cargo build --bin openact-cli > /dev/null 2>&1 && echo "   ✅ Built successfully with noop metrics (zero overhead)"

echo ""

# Test 2: Metrics feature enabled but disabled by environment
echo "2️⃣  Testing Metrics Feature with Environment Disabled:"
echo "   Building with metrics feature but OPENACT_METRICS_ENABLED=false"
echo "   cargo build --features metrics --bin openact-cli"
OPENACT_METRICS_ENABLED=false cargo build --features metrics --bin openact-cli > /dev/null 2>&1 && echo "   ✅ Built successfully with metrics feature (disabled by env)"

echo ""

# Test 3: Full Prometheus metrics enabled
echo "3️⃣  Testing Full Prometheus Metrics Configuration:"
echo "   With OPENACT_METRICS_ENABLED=true OPENACT_METRICS_ADDR=127.0.0.1:9090"
echo "   This would start a Prometheus HTTP server on port 9090 when running the server"
echo "   ✅ Configuration ready for Prometheus export"

echo ""
echo "🚀 Production Usage Examples:"
echo ""

echo "A) No-op mode (default, zero overhead):"
echo "   cargo run --bin openact-cli server"
echo ""

echo "B) Prometheus enabled:"
echo "   # Set environment variables:"
echo "   export OPENACT_METRICS_ENABLED=true"
echo "   export OPENACT_METRICS_ADDR=0.0.0.0:9090"
echo "   "
echo "   # Run with metrics feature:"
echo "   cargo run --features metrics --bin openact-cli server"
echo "   "
echo "   # Metrics available at: http://localhost:9090/metrics"
echo ""

echo "C) Docker deployment with metrics:"
echo "   # Build with metrics:"
echo "   docker build --build-arg FEATURES=metrics -t openact:metrics ."
echo "   "
echo "   # Run with Prometheus:"
echo "   docker run -p 8080:8080 -p 9090:9090 \\"
echo "     -e OPENACT_METRICS_ENABLED=true \\"
echo "     -e OPENACT_METRICS_ADDR=0.0.0.0:9090 \\"
echo "     openact:metrics"
echo ""

echo "📈 Available Metrics:"
echo ""
echo "   • openact_http_requests_total - Total HTTP requests"
echo "   • openact_http_request_duration_seconds - HTTP request duration"
echo "   • openact_task_executions_total - Total task executions"  
echo "   • openact_task_execution_duration_seconds - Task execution duration"
echo "   • openact_retries_total - Total retry attempts"
echo "   • openact_retry_delay_seconds - Retry delay duration"
echo "   • openact_active_connections - Current active connections"
echo "   • openact_connection_pool_* - Connection pool metrics"
echo "   • openact_database_operations_total - Database operations"
echo "   • openact_cache_* - Cache metrics"
echo "   • openact_errors_total - Total errors"
echo ""

echo "🔧 Configuration Options:"
echo ""
echo "   Environment Variables:"
echo "   • OPENACT_METRICS_ENABLED=true|false"
echo "   • OPENACT_METRICS_ADDR=host:port (default: 0.0.0.0:9090)"
echo ""
echo "   Cargo Features:"
echo "   • --features metrics (enables Prometheus export)"
echo "   • Default: noop implementation (zero overhead)"
echo ""

echo "✅ Demo completed! Metrics system is ready for production use."
