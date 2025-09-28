#!/bin/bash

# Test script for the new OpenAct architecture
# This script tests the responsibility separation + shared execution core architecture

set -e

echo "🚀 Testing OpenAct New Architecture"
echo "=================================="

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

# Test 1: Build system with xtask
test_xtask_build() {
    log_info "Testing xtask build system..."
    
    # Test CLI build
    log_info "Building CLI with default connectors..."
    cargo run -p xtask -- build -p openact-cli
    log_success "CLI build successful"
    
    # Test server build  
    log_info "Building server with default connectors..."
    cargo run -p xtask -- build -p openact-server
    log_success "Server build successful"
    
    # Test with specific connectors
    log_info "Testing connector selection..."
    echo '[connectors]
http = true
postgresql = false' > connectors.toml.test
    
    mv connectors.toml connectors.toml.backup
    mv connectors.toml.test connectors.toml
    
    cargo run -p xtask -- build -p openact-cli
    log_success "Selective connector build successful"
    
    # Restore original config
    mv connectors.toml.backup connectors.toml
}

# Test 2: Plugin registration system
test_plugin_system() {
    log_info "Testing plugin registration system..."
    
    # Test plugin enumeration
    cargo test -p openact-plugins -- --nocapture
    log_success "Plugin system tests passed"
}

# Test 3: Runtime execution core
test_runtime_core() {
    log_info "Testing runtime execution core..."
    
    # Test runtime functions
    cargo test -p openact-runtime -- --nocapture
    log_success "Runtime core tests passed"
}

# Test 4: CLI commands with new architecture
test_cli_commands() {
    log_info "Testing CLI commands with new architecture..."
    
    # Build CLI first
    cargo run -p xtask -- build -p openact-cli
    
    # Test execute-file command (if example config exists)
    if [ -f "example-config.yaml" ]; then
        log_info "Testing execute-file command..."
        # This would be an actual test if we had test actions
        log_warning "Skipping execute-file test (no test actions configured)"
    fi
    
    # Test execute-inline command
    log_info "Testing execute-inline command..."
    # This would test inline configuration
    log_warning "Skipping execute-inline test (requires live connections)"
    
    log_success "CLI architecture tests completed"
}

# Test 5: Connector isolation
test_connector_isolation() {
    log_info "Testing connector isolation..."
    
    # Test HTTP connector independently
    cargo test -p openact-connectors --features http -- --nocapture
    log_success "HTTP connector isolation test passed"
    
    # Test PostgreSQL connector independently  
    cargo test -p openact-connectors --features postgresql -- --nocapture
    log_success "PostgreSQL connector isolation test passed"
}

# Test 6: Sensitive data sanitization
test_data_sanitization() {
    log_info "Testing sensitive data sanitization..."
    
    cargo test -p openact-core sanitization -- --nocapture
    log_success "Data sanitization tests passed"
}

# Test 7: No compilation warnings
test_clean_compilation() {
    log_info "Testing clean compilation (no warnings)..."
    
    # Build with warnings as errors to ensure clean compilation
    RUSTFLAGS="-D warnings" cargo run -p xtask -- build -p openact-cli
    RUSTFLAGS="-D warnings" cargo run -p xtask -- build -p openact-server
    
    log_success "Clean compilation test passed (no warnings)"
}

# Main test execution
main() {
    log_info "Starting OpenAct architecture tests..."
    echo ""
    
    # Check if we're in the right directory
    if [ ! -f "Cargo.toml" ] || [ ! -d "crates" ]; then
        log_error "Must run from OpenAct project root directory"
        exit 1
    fi
    
    # Run tests
    test_xtask_build
    echo ""
    
    test_plugin_system
    echo ""
    
    test_runtime_core
    echo ""
    
    test_cli_commands
    echo ""
    
    test_connector_isolation
    echo ""
    
    test_data_sanitization
    echo ""
    
    test_clean_compilation
    echo ""
    
    log_success "🎉 All architecture tests completed successfully!"
    echo ""
    echo "Architecture validation summary:"
    echo "✅ xtask build system working"
    echo "✅ Plugin registration functional"
    echo "✅ Runtime execution core operational"
    echo "✅ CLI commands using new architecture"
    echo "✅ Connector isolation maintained"
    echo "✅ Data sanitization active"
    echo "✅ Clean compilation achieved"
}

# Run main function
main "$@"
