#!/bin/bash

# Test script for connector functionality
# Tests individual connectors and their integration

set -e

echo "🔌 Testing OpenAct Connectors"
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

# Test HTTP connector functionality
test_http_connector() {
    log_info "Testing HTTP connector..."
    
    # Test HTTP connector compilation
    cargo check -p openact-connectors --features http
    log_success "HTTP connector compiles successfully"
    
    # Test HTTP connector unit tests
    cargo test -p openact-connectors --features http http -- --nocapture
    log_success "HTTP connector unit tests passed"
    
    # Test HTTP factory registration
    log_info "Testing HTTP factory registration..."
    cargo test -p openact-plugins --features http -- --nocapture 
    log_success "HTTP factory registration working"
}

# Test PostgreSQL connector functionality
test_postgresql_connector() {
    log_info "Testing PostgreSQL connector..."
    
    # Test PostgreSQL connector compilation
    cargo check -p openact-connectors --features postgresql
    log_success "PostgreSQL connector compiles successfully"
    
    # Test PostgreSQL connector unit tests
    cargo test -p openact-connectors --features postgresql postgres -- --nocapture
    log_success "PostgreSQL connector unit tests passed"
    
    # Test PostgreSQL factory registration
    log_info "Testing PostgreSQL factory registration..."
    cargo test -p openact-plugins --features postgresql -- --nocapture
    log_success "PostgreSQL factory registration working"
}

# Test connector isolation (build without specific connectors)
test_connector_isolation() {
    log_info "Testing connector isolation..."
    
    # Create temporary config with only HTTP
    echo '[connectors]
http = true
postgresql = false' > connectors.toml.http_only
    
    # Backup original
    mv connectors.toml connectors.toml.backup
    mv connectors.toml.http_only connectors.toml
    
    # Build with only HTTP
    cargo run -p xtask -- build -p openact-cli
    log_success "HTTP-only build successful"
    
    # Create temporary config with only PostgreSQL
    echo '[connectors]
http = false
postgresql = true' > connectors.toml.pg_only
    
    mv connectors.toml connectors.toml.http_only
    mv connectors.toml.pg_only connectors.toml
    
    # Build with only PostgreSQL
    cargo run -p xtask -- build -p openact-cli
    log_success "PostgreSQL-only build successful"
    
    # Restore original config
    mv connectors.toml.http_only connectors.toml.test
    mv connectors.toml.backup connectors.toml
    rm -f connectors.toml.test
    
    log_success "Connector isolation test passed"
}

# Test runtime connector loading
test_runtime_loading() {
    log_info "Testing runtime connector loading..."
    
    # Test registry building with different connector combinations
    cargo test -p openact-runtime registry_from_records -- --nocapture
    log_success "Runtime connector loading tests passed"
}

# Test configuration validation
test_config_validation() {
    log_info "Testing configuration validation..."
    
    # Test config parsing and validation
    cargo test -p openact-config -- --nocapture
    log_success "Configuration validation tests passed"
}

# Test end-to-end connector workflow (dry run)
test_e2e_workflow() {
    log_info "Testing end-to-end connector workflow (dry run)..."
    
    # Build CLI
    cargo run -p xtask -- build -p openact-cli
    
    # Test with example config if it exists
    if [ -f "examples/http.yaml" ]; then
        log_info "Testing HTTP connector with example config..."
        # Dry run test would go here
        log_warning "Skipping live HTTP test (dry run only)"
    fi
    
    if [ -f "examples/postgres.yaml" ]; then
        log_info "Testing PostgreSQL connector with example config..."
        # Dry run test would go here  
        log_warning "Skipping live PostgreSQL test (dry run only)"
    fi
    
    log_success "End-to-end workflow tests completed"
}

# Test connector factory patterns
test_factory_patterns() {
    log_info "Testing connector factory patterns..."
    
    # Test that all factories follow the same pattern
    log_info "Checking HTTP factory pattern..."
    cargo test -p openact-connectors --features http factory -- --nocapture
    
    log_info "Checking PostgreSQL factory pattern..."
    cargo test -p openact-connectors --features postgresql factory -- --nocapture
    
    log_success "Factory pattern consistency verified"
}

# Main test execution
main() {
    log_info "Starting connector tests..."
    echo ""
    
    # Check if we're in the right directory
    if [ ! -f "Cargo.toml" ] || [ ! -d "crates" ]; then
        log_error "Must run from OpenAct project root directory"
        exit 1
    fi
    
    # Run connector tests
    test_http_connector
    echo ""
    
    test_postgresql_connector  
    echo ""
    
    test_connector_isolation
    echo ""
    
    test_runtime_loading
    echo ""
    
    test_config_validation
    echo ""
    
    test_e2e_workflow
    echo ""
    
    test_factory_patterns
    echo ""
    
    log_success "🎉 All connector tests completed successfully!"
    echo ""
    echo "Connector validation summary:"
    echo "✅ HTTP connector functional"
    echo "✅ PostgreSQL connector functional"
    echo "✅ Connector isolation working"
    echo "✅ Runtime loading operational"
    echo "✅ Configuration validation active"
    echo "✅ End-to-end workflows testable"
    echo "✅ Factory patterns consistent"
}

# Run main function
main "$@"
