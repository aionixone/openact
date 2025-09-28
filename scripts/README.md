# OpenAct Test Scripts

This directory contains comprehensive test scripts for validating OpenAct's new **responsibility separation + shared execution core** architecture.

## 🚀 Quick Start

```bash
# Quick smoke test (recommended for development)
make test-quick
# or
./scripts/quick_test.sh

# Comprehensive test suite
make test-all
# or
./scripts/run_all_tests.sh
```

## 📋 Available Test Suites

### 1. Quick Smoke Test
**File:** `quick_test.sh`  
**Command:** `make test-quick`  
**Duration:** ~1 minute  
**Purpose:** Basic validation that the new architecture is working

**What it tests:**
- ✅ Build system compilation
- ✅ Plugin registration
- ✅ Runtime execution core
- ✅ Connector functionality
- ✅ CLI command availability

### 2. Architecture Tests
**File:** `test_architecture.sh`  
**Command:** `make test-architecture`  
**Duration:** ~3-5 minutes  
**Purpose:** Comprehensive validation of the new architecture implementation

**What it tests:**
- ✅ xtask build system with connector selection
- ✅ Plugin registration mechanism
- ✅ Runtime execution functions
- ✅ CLI command integration
- ✅ Connector isolation
- ✅ Data sanitization
- ✅ Clean compilation (no warnings)

### 3. Connector Tests
**File:** `test_connectors.sh`  
**Command:** `make test-connectors`  
**Duration:** ~2-3 minutes  
**Purpose:** Validate connector functionality and isolation

**What it tests:**
- ✅ HTTP connector compilation and tests
- ✅ PostgreSQL connector compilation and tests
- ✅ Selective connector builds
- ✅ Runtime connector loading
- ✅ Configuration validation
- ✅ Factory pattern consistency

### 4. Performance Tests
**File:** `test_performance.sh`  
**Command:** `make test-performance`  
**Duration:** ~5-10 minutes  
**Purpose:** Measure build times, binary sizes, and execution performance

**What it tests:**
- ⚡ Build performance (cold vs incremental)
- ⚡ Individual crate build times
- ⚡ Test execution performance
- ⚡ Memory usage during builds
- ⚡ Binary size optimization
- ⚡ Plugin registration speed
- ⚡ Parallel build capability

### 5. Integration Tests
**File:** `test_integration.sh`  
**Command:** `make test-integration`  
**Duration:** ~3-5 minutes  
**Purpose:** Test end-to-end workflows and real-world scenarios

**What it tests:**
- 🔗 CLI execute-file command
- 🔗 CLI execute-inline command
- 🔗 Database integration
- 🔗 Server startup
- 🔗 Configuration compatibility
- 🔗 Multi-connector scenarios
- 🔗 Error handling
- 🔗 Performance with real configs

### 6. Master Test Suite
**File:** `run_all_tests.sh`  
**Command:** `make test-all`  
**Duration:** ~10-20 minutes  
**Purpose:** Run all test suites and generate comprehensive report

**Options:**
```bash
# Run all tests (default)
./scripts/run_all_tests.sh

# Quick mode (architecture + connectors only)
./scripts/run_all_tests.sh --quick
make test-all-quick

# Performance tests only
./scripts/run_all_tests.sh --performance

# Integration tests only
./scripts/run_all_tests.sh --integration
```

## 🎯 Recommended Testing Workflow

### For Development
1. **Start with quick test:** `make test-quick` (~1 min)
2. **If quick test passes:** Continue development
3. **Before committing:** `make test-all-quick` (~5 min)
4. **Before major releases:** `make test-all` (~15 min)

### For CI/CD
```bash
# Fast feedback loop
make test-quick

# Comprehensive validation
make test-all-quick

# Full validation (optional, for releases)
make test-all
```

### For Performance Analysis
```bash
# Focus on performance
make test-performance

# Or specific performance tests
./scripts/run_all_tests.sh --performance
```

## 📊 Reports Generated

### Quick Test
- Console output with immediate feedback
- Pass/fail status for each component

### Performance Tests
- `performance_report.md` - Detailed performance metrics
- Build times, binary sizes, memory usage

### Master Test Suite
- `master_test_report.md` - Comprehensive architecture validation report
- Executive summary, test coverage, recommendations

## 🏗️ Architecture Validation

These tests specifically validate the new architecture improvements:

### ✅ Responsibility Separation
- **Runtime Core:** Connector-agnostic execution engine
- **Plugin System:** Dynamic connector registration
- **Build System:** Centralized connector control

### ✅ Shared Execution Core
- **Unified Path:** Same execution logic for CLI, REST, MCP
- **Common Interface:** `registry_from_records_ext` + `execute_action`
- **Consistent Behavior:** All entry points use runtime helpers

### ✅ Dependency Decoupling
- **No Cycles:** Resolved registry ↔ connectors dependency
- **Clean Layers:** Config ↔ Runtime ↔ Connectors ↔ Registry
- **Isolation:** Each connector can be built independently

### ✅ Build Optimization
- **Selective Compilation:** Only compile needed connectors
- **Feature Management:** Single `connectors.toml` configuration
- **Binary Optimization:** Exclude unused connectors

## 🔧 Troubleshooting

### Test Failures

1. **Build failures:**
   ```bash
   # Clean and retry
   cargo clean
   make test-quick
   ```

2. **Plugin registration failures:**
   ```bash
   # Check connector features
   cargo test -p openact-plugins -- --nocapture
   ```

3. **Performance test issues:**
   ```bash
   # Install required tools
   # macOS: brew install gnu-time
   # Ubuntu: apt-get install time bc
   ```

### Environment Requirements

- **Rust:** 1.70+ (for proper feature support)
- **Network:** Required for HTTP integration tests
- **Tools:** `bc`, `time` (for performance measurements)
- **Database:** SQLite (built-in), PostgreSQL (optional for full tests)

## 🚀 Adding New Tests

### Adding Architecture Tests
Add test functions to `test_architecture.sh`:
```bash
test_new_feature() {
    log_info "Testing new feature..."
    # Test implementation
    log_success "New feature working"
}
```

### Adding Connector Tests
Add connector-specific tests to `test_connectors.sh`:
```bash
test_new_connector() {
    log_info "Testing new connector..."
    cargo test -p openact-connectors --features new_connector
    log_success "New connector functional"
}
```

### Adding Performance Benchmarks
Add performance measurements to `test_performance.sh`:
```bash
measure_time "cargo build -p new-crate" "New crate build time"
```

## 📚 Related Documentation

- **Architecture:** See `docs/ARCHITECTURE.md` for design details
- **Build System:** See `xtask/README.md` for build tool usage
- **Connectors:** See individual connector READMEs
- **Configuration:** See `crates/openact-config/README.md`

---

**Note:** These test scripts are designed to validate the new architecture while ensuring backward compatibility. All existing functionality should continue to work as before.
