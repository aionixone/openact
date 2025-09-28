# OpenAct Build and Verification

.PHONY: help build build-openapi test test-openapi clean openapi-json openapi-validate
.PHONY: test-quick test-architecture test-connectors test-performance test-integration test-all

help: ## Display help information
	@echo "OpenAct Build Commands:"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build basic version
	cargo build --release

build-openapi: ## Build version with OpenAPI documentation
	cargo build --release --features openapi,server

test: ## Run basic tests
	cargo test

test-openapi: ## Run OpenAPI related tests
	cargo test --features openapi,server

openapi-json: ## Generate OpenAPI JSON file
	@echo "Generating OpenAPI specification..."
	cargo test openapi_json_generation --features openapi,server -- --nocapture --exact

openapi-validate: ## Validate OpenAPI configuration integrity
	@echo "Validating OpenAPI configuration..."
	cargo test openapi_generation --features openapi,server -- --nocapture --exact
	@echo "✅ OpenAPI configuration validation passed"

clean: ## Clean build files
	cargo clean

# CI related commands
ci-check: build test ## Basic CI check
	@echo "✅ Basic CI check completed"

ci-check-openapi: build-openapi test-openapi openapi-validate ## CI OpenAPI check
	@echo "✅ OpenAPI CI check completed"

# Complete check
check-all: ci-check ci-check-openapi ## Complete project check
	@echo "✅ All checks completed"

# New Architecture Test Commands
test-quick: ## Quick smoke test for new architecture
	@echo "Running quick architecture validation..."
	./scripts/quick_test.sh

test-architecture: ## Test architecture implementation
	@echo "Running architecture tests..."
	./scripts/test_architecture.sh

test-connectors: ## Test connector functionality and isolation
	@echo "Running connector tests..."
	./scripts/test_connectors.sh

test-performance: ## Test build and execution performance
	@echo "Running performance tests..."
	./scripts/test_performance.sh

test-integration: ## Test end-to-end integration scenarios
	@echo "Running integration tests..."
	./scripts/test_integration.sh

test-all: ## Run comprehensive test suite
	@echo "Running complete test suite..."
	./scripts/run_all_tests.sh

test-all-quick: ## Run essential tests only (architecture + connectors)
	@echo "Running essential tests..."
	./scripts/run_all_tests.sh --quick
