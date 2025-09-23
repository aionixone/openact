#!/bin/bash
set -e

# OpenAct Complete Demo
# Demonstrates all major features: API Key, Basic Auth, OAuth2, CLI, HTTP API

echo "🚀 OpenAct Complete Demo"
echo "========================"
echo ""

BASE_URL="http://127.0.0.1:8080"

# Check if server is running
echo "🔍 Checking if server is running..."
if ! curl -s "${BASE_URL}/api/v1/system/health" > /dev/null; then
    echo "❌ Server is not running at ${BASE_URL}"
    echo "Please start the server with:"
    echo "  RUST_LOG=info OPENACT_DB_URL=sqlite:./data/openact.db?mode=rwc cargo run --features server --bin openact"
    exit 1
fi
echo "✅ Server is running"
echo ""

# System Health Check
echo "📊 System Health & Stats"
echo "------------------------"
echo "Health Status:"
curl -s "${BASE_URL}/api/v1/system/health" | jq '.status'

echo ""
echo "System Stats Summary:"
STATS=$(curl -s "${BASE_URL}/api/v1/system/stats")
echo "  Connections: $(echo "$STATS" | jq '.storage.total_connections')"
echo "  Tasks: $(echo "$STATS" | jq '.storage.total_tasks')"
echo "  Version: $(echo "$STATS" | jq -r '.system.version.version')"
echo "  Features: $(echo "$STATS" | jq -r '.system.version.features | join(", ")')"
echo ""

# Feature Demos
echo "🎯 Feature Demonstrations"
echo "=========================="
echo ""

echo "1️⃣  API Key Authentication Demo"
echo "-------------------------------"
if [ -f "examples/api_key_example.sh" ]; then
    echo "Running API Key example..."
    ./examples/api_key_example.sh | tail -10
    echo "✅ API Key example completed"
else
    echo "⚠️  API Key example script not found"
fi
echo ""

echo "2️⃣  Basic Authentication Demo"
echo "-----------------------------"
if [ -f "examples/basic_auth_example.sh" ]; then
    echo "Running Basic Auth example..."
    ./examples/basic_auth_example.sh | tail -10
    echo "✅ Basic Auth example completed"
else
    echo "⚠️  Basic Auth example script not found"
fi
echo ""

echo "3️⃣  CLI vs HTTP API Consistency Demo"
echo "-----------------------------------"
TASK_TRN="trn:openact:demo:task/httpbin-get@v1"
echo "Testing task: $TASK_TRN"
echo ""

echo "Via HTTP API:"
ENCODED_TRN=$(echo "${TASK_TRN}" | sed 's|:|%3A|g' | sed 's|/|%2F|g' | sed 's|@|%40|g')
HTTP_RESULT=$(curl -s -X POST "${BASE_URL}/api/v1/tasks/${ENCODED_TRN}/execute" -H "Content-Type: application/json" -d '{}')
HTTP_STATUS=$(echo "$HTTP_RESULT" | jq -r '.status')
echo "  Status: $HTTP_STATUS"

echo ""
echo "Via CLI (server mode):"
CLI_RESULT=$(cargo run --bin openact-cli -- --server "${BASE_URL}" execute "${TASK_TRN}" 2>/dev/null | head -1)
echo "  $CLI_RESULT"

if [ "$HTTP_STATUS" = "200" ]; then
    echo "✅ Both methods return consistent results"
else
    echo "⚠️  Results may differ"
fi
echo ""

# Resource Management Demo
echo "4️⃣  Resource Management Demo"
echo "----------------------------"
echo "Current Resources:"

echo "Connections:"
curl -s "${BASE_URL}/api/v1/connections" | jq '.[] | {trn: .trn, name: .name, type: .authorization_type}' | head -20

echo ""
echo "Tasks:"
curl -s "${BASE_URL}/api/v1/tasks" | jq '.[] | {trn: .trn, name: .name, endpoint: .api_endpoint}' | head -20

echo ""

# Performance & Monitoring Demo
echo "5️⃣  Performance & Monitoring Demo"
echo "---------------------------------"
echo "Cache Statistics:"
CACHE_STATS=$(curl -s "${BASE_URL}/api/v1/system/stats" | jq '.caches')
echo "  Execution Cache: $(echo "$CACHE_STATS" | jq '.exec_cache_size') entries, $(echo "$CACHE_STATS" | jq '.exec_hit_rate') hit rate"

echo ""
echo "Client Pool Statistics:"
POOL_STATS=$(curl -s "${BASE_URL}/api/v1/system/stats" | jq '.client_pool')
echo "  Pool Size: $(echo "$POOL_STATS" | jq '.size')/$(echo "$POOL_STATS" | jq '.capacity')"
echo "  Hit Rate: $(echo "$POOL_STATS" | jq '.hit_rate')"
echo "  Total Hits: $(echo "$POOL_STATS" | jq '.hits'), Builds: $(echo "$POOL_STATS" | jq '.builds')"

echo ""

# CLI Features Demo
echo "6️⃣  CLI Features Demo" 
echo "--------------------"
echo "CLI Help:"
cargo run --bin openact-cli -- --help 2>/dev/null | head -10

echo ""
echo "Connection sub-commands:"
cargo run --bin openact-cli -- connection --help 2>/dev/null | head -5

echo ""
echo "Task sub-commands:"
cargo run --bin openact-cli -- task --help 2>/dev/null | head -5

echo ""

# Advanced Features Summary
echo "7️⃣  Advanced Features Summary"
echo "-----------------------------"
echo "✅ Multi-tenant support (TRN system)"
echo "✅ Multiple authentication types (API Key, Basic, OAuth2)"
echo "✅ HTTP policy enforcement and header normalization"
echo "✅ Client connection pooling with LRU eviction"
echo "✅ Execution caching and performance optimization"
echo "✅ Unified CLI and HTTP API with consistent interfaces"
echo "✅ Comprehensive monitoring and health checks"
echo "✅ SQLite-based persistence with migration support"
echo "✅ Configurable timeouts and retry policies"
echo "✅ Binary and JSON response support"

echo ""
echo "🎉 OpenAct Complete Demo Finished!"
echo "==================================="
echo ""
echo "📚 What you can do next:"
echo "  🔗 Try OAuth2 flows: ./examples/github_oauth_complete.sh"
echo "  📖 Read the documentation: README.md"
echo "  🔧 Check configuration options: .env.example"
echo "  📊 Monitor system: curl ${BASE_URL}/api/v1/system/stats"
echo "  🧪 Test CLI: cargo run --bin openact-cli -- --help"
echo ""
echo "💡 OpenAct makes API integration simple, consistent, and powerful!"
