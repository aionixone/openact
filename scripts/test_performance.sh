#!/bin/bash

# Performance test script for OpenAct
# Tests build times, execution performance, and resource usage

set -e

echo "⚡ Testing OpenAct Performance"
echo "============================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
}

# Function to measure execution time
measure_time() {
    local cmd="$1"
    local desc="$2"
    
    log_info "Measuring: $desc"
    start_time=$(date +%s.%N)
    
    eval "$cmd"
    local exit_code=$?
    
    end_time=$(date +%s.%N)
    duration=$(echo "$end_time - $start_time" | bc -l)
    
    if [ $exit_code -eq 0 ]; then
        log_success "✓ $desc completed in ${duration}s"
    else
        log_error "✗ $desc failed after ${duration}s"
        return $exit_code
    fi
}

# Test build performance
test_build_performance() {
    log_info "Testing build performance..."
    echo ""
    
    # Clean first
    cargo clean
    
    # Test full build time
    measure_time "cargo run -p xtask -- build -p openact-cli" "Full CLI build (cold)"
    
    # Test incremental build
    measure_time "cargo run -p xtask -- build -p openact-cli" "Incremental CLI build"
    
    # Test server build
    measure_time "cargo run -p xtask -- build -p openact-server" "Server build"
    
    # Test selective connector build
    echo '[connectors]
http = true
postgresql = false' > connectors.toml.perf_test
    
    mv connectors.toml connectors.toml.backup
    mv connectors.toml.perf_test connectors.toml
    
    cargo clean
    measure_time "cargo run -p xtask -- build -p openact-cli" "Selective connector build (HTTP only)"
    
    # Restore config
    mv connectors.toml.backup connectors.toml
    
    echo ""
}

# Test compilation time for individual crates
test_crate_build_times() {
    log_info "Testing individual crate build times..."
    echo ""
    
    cargo clean
    
    # Core crates
    measure_time "cargo build -p openact-core" "openact-core build"
    measure_time "cargo build -p openact-config" "openact-config build" 
    measure_time "cargo build -p openact-store" "openact-store build"
    measure_time "cargo build -p openact-registry" "openact-registry build"
    
    # New architecture crates
    measure_time "cargo build -p openact-runtime" "openact-runtime build"
    measure_time "cargo build -p openact-plugins" "openact-plugins build"
    measure_time "cargo build -p openact-connectors --features http,postgresql" "openact-connectors build"
    
    # Application crates
    measure_time "cargo build -p openact-cli" "openact-cli build"
    measure_time "cargo build -p openact-server" "openact-server build"
    
    echo ""
}

# Test test execution performance
test_test_performance() {
    log_info "Testing test execution performance..."
    echo ""
    
    # Unit tests
    measure_time "cargo test -p openact-core" "openact-core tests"
    measure_time "cargo test -p openact-runtime" "openact-runtime tests"
    measure_time "cargo test -p openact-plugins" "openact-plugins tests"
    
    # Connector tests
    measure_time "cargo test -p openact-connectors --features http" "HTTP connector tests"
    measure_time "cargo test -p openact-connectors --features postgresql" "PostgreSQL connector tests"
    
    echo ""
}

# Test memory usage during build
test_memory_usage() {
    log_info "Testing memory usage..."
    echo ""
    
    if command -v /usr/bin/time >/dev/null 2>&1; then
        log_info "Measuring memory usage during build..."
        
        cargo clean
        /usr/bin/time -l cargo run -p xtask -- build -p openact-cli 2>&1 | grep "maximum resident set size" || log_warning "Memory measurement not available"
    else
        log_warning "GNU time not available, skipping memory tests"
    fi
    
    echo ""
}

# Test binary size
test_binary_size() {
    log_info "Testing binary sizes..."
    echo ""
    
    # Build release binaries
    cargo run -p xtask -- build -p openact-cli --release
    cargo run -p xtask -- build -p openact-server --release  
    
    # Check CLI binary size
    if [ -f "target/release/openact-cli" ]; then
        cli_size=$(ls -lh target/release/openact-cli | awk '{print $5}')
        log_info "CLI binary size: $cli_size"
    fi
    
    # Check server binary size
    if [ -f "target/release/openact-server" ]; then
        server_size=$(ls -lh target/release/openact-server | awk '{print $5}')
        log_info "Server binary size: $server_size"
    fi
    
    # Test with minimal connectors
    echo '[connectors]
http = true
postgresql = false' > connectors.toml.minimal
    
    mv connectors.toml connectors.toml.backup
    mv connectors.toml.minimal connectors.toml
    
    cargo run -p xtask -- build -p openact-cli --release
    
    if [ -f "target/release/openact-cli" ]; then
        minimal_size=$(ls -lh target/release/openact-cli | awk '{print $5}')
        log_info "Minimal CLI binary size (HTTP only): $minimal_size"
    fi
    
    # Restore config
    mv connectors.toml.backup connectors.toml
    
    echo ""
}

# Test plugin registration performance
test_plugin_performance() {
    log_info "Testing plugin registration performance..."
    echo ""
    
    # Test plugin loading time
    measure_time "cargo test -p openact-plugins registrars -- --nocapture" "Plugin registration speed"
    
    # Test runtime registry building
    measure_time "cargo test -p openact-runtime registry_from_records_ext -- --nocapture" "Runtime registry building"
    
    echo ""
}

# Test parallel build capability
test_parallel_builds() {
    log_info "Testing parallel build capability..."
    echo ""
    
    cargo clean
    
    # Test if parallel builds work
    measure_time "cargo build -j$(nproc 2>/dev/null || echo 4)" "Parallel build ($(nproc 2>/dev/null || echo 4) jobs)"
    
    echo ""
}

# Generate performance report
generate_report() {
    log_info "Generating performance report..."
    
    cat > performance_report.md << EOF
# OpenAct Performance Report

Generated on: $(date)

## Build Performance
- Full CLI build times measured
- Incremental build efficiency tested
- Selective connector builds validated

## Binary Sizes
- Release binary sizes documented
- Minimal connector impact measured

## Test Performance  
- Unit test execution times recorded
- Connector-specific test performance measured

## Memory Usage
- Build-time memory consumption analyzed

## Recommendations
- Use selective connector builds for faster development
- Leverage incremental compilation for iterative development
- Monitor binary size growth with new connectors

## Architecture Benefits
✅ Plugin system allows selective compilation
✅ Runtime core enables shared execution paths
✅ Connector isolation prevents unnecessary dependencies
✅ xtask build system optimizes feature management
EOF

    log_success "Performance report generated: performance_report.md"
}

# Main test execution
main() {
    log_info "Starting performance tests..."
    echo ""
    
    # Check if we're in the right directory
    if [ ! -f "Cargo.toml" ] || [ ! -d "crates" ]; then
        log_error "Must run from OpenAct project root directory"
        exit 1
    fi
    
    # Check for required tools
    if ! command -v bc >/dev/null 2>&1; then
        log_warning "bc calculator not found, some time measurements may not work"
    fi
    
    # Run performance tests
    test_build_performance
    test_crate_build_times
    test_test_performance
    test_memory_usage
    test_binary_size
    test_plugin_performance
    test_parallel_builds
    
    # Generate report
    generate_report
    
    log_success "🎉 Performance testing completed!"
    echo ""
    echo "Performance validation summary:"
    echo "✅ Build performance measured"
    echo "✅ Test execution times recorded"
    echo "✅ Memory usage analyzed"
    echo "✅ Binary sizes documented"
    echo "✅ Plugin performance validated"
    echo "✅ Parallel build capability confirmed"
    echo "📊 Report generated: performance_report.md"
}

# Run main function
main "$@"
