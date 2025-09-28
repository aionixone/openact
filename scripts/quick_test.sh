#!/bin/bash

# Quick smoke test for OpenAct new architecture
# Validates basic functionality without comprehensive testing

set -e

echo "🚀 OpenAct Quick Smoke Test"
echo "==========================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

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

# Quick compilation test
quick_build_test() {
    log_info "Testing basic compilation..."
    
    # Test xtask build
    cargo run -p xtask -- build -p openact-cli >/dev/null 2>&1
    log_success "CLI builds successfully"
    
    cargo run -p xtask -- build -p openact-server >/dev/null 2>&1  
    log_success "Server builds successfully"
}

# Quick plugin test
quick_plugin_test() {
    log_info "Testing plugin system..."
    
    # Test plugin registration
    cargo test -p openact-plugins registrars -- --nocapture >/dev/null 2>&1
    log_success "Plugin registration working"
}

# Quick runtime test
quick_runtime_test() {
    log_info "Testing runtime core..."
    
    # Test runtime functions
    cargo test -p openact-runtime registry_from_records -- --nocapture >/dev/null 2>&1
    log_success "Runtime core functional"
}

# Quick connector test
quick_connector_test() {
    log_info "Testing connectors..."
    
    # Test HTTP connector
    cargo test -p openact-connectors --features http -- --nocapture >/dev/null 2>&1
    log_success "HTTP connector working"
    
    # Test PostgreSQL connector
    cargo test -p openact-connectors --features postgresql -- --nocapture >/dev/null 2>&1
    log_success "PostgreSQL connector working"
}

# Quick CLI test
quick_cli_test() {
    log_info "Testing CLI functionality..."
    
    # Test CLI help
    ./target/debug/openact --help >/dev/null 2>&1
    log_success "CLI help working"
    
    # Test CLI commands exist
    ./target/debug/openact execute-file --help >/dev/null 2>&1
    log_success "execute-file command available"
    
    ./target/debug/openact execute-inline --help >/dev/null 2>&1
    log_success "execute-inline command available"
}

# Main quick test
main() {
    local start_time=$(date +%s)
    
    log_info "Running quick smoke test..."
    echo ""
    
    # Check directory
    if [ ! -f "Cargo.toml" ] || [ ! -d "crates" ]; then
        log_error "Must run from OpenAct project root directory"
        exit 1
    fi
    
    # Run quick tests
    quick_build_test
    quick_plugin_test
    quick_runtime_test  
    quick_connector_test
    quick_cli_test
    
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))
    
    echo ""
    log_success "🎉 Quick smoke test completed in ${duration}s!"
    echo ""
    echo "Basic validation:"
    echo "✅ Build system operational"
    echo "✅ Plugin architecture functional"
    echo "✅ Runtime core working"
    echo "✅ Connectors available"
    echo "✅ CLI commands accessible"
    echo ""
    echo "💡 Run './scripts/run_all_tests.sh' for comprehensive testing"
}

main "$@"
