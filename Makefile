# OpenAct 构建和验证

.PHONY: help build build-openapi test test-openapi clean openapi-json openapi-validate

help: ## 显示帮助信息
	@echo "OpenAct 构建命令:"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## 构建基本版本
	cargo build --release

build-openapi: ## 构建带 OpenAPI 文档的版本
	cargo build --release --features openapi,server

test: ## 运行基本测试
	cargo test

test-openapi: ## 运行 OpenAPI 相关测试
	cargo test --features openapi,server

openapi-json: ## 生成 OpenAPI JSON 文件
	@echo "生成 OpenAPI 规范..."
	cargo test openapi_json_generation --features openapi,server -- --nocapture --exact

openapi-validate: ## 验证 OpenAPI 配置完整性
	@echo "验证 OpenAPI 配置..."
	cargo test openapi_generation --features openapi,server -- --nocapture --exact
	@echo "✅ OpenAPI 配置验证通过"

clean: ## 清理构建文件
	cargo clean

# CI 相关命令
ci-check: build test ## CI 基础检查
	@echo "✅ 基础 CI 检查完成"

ci-check-openapi: build-openapi test-openapi openapi-validate ## CI OpenAPI 检查
	@echo "✅ OpenAPI CI 检查完成"

# 完整检查
check-all: ci-check ci-check-openapi ## 完整项目检查
	@echo "✅ 所有检查完成"
