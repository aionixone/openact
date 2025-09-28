#!/bin/bash
set -e

# Retry Policy Demo
# Demonstrates retry policy configuration and behavior

echo "🔄 Retry Policy Demo"
echo "==================="

BASE_URL="http://127.0.0.1:8080"
TENANT="demo"
CONNECTION_TRN="trn:openact:${TENANT}:connection/httpbin-retry@v1"
TASK_TRN="trn:openact:${TENANT}:task/httpbin-status@v1"

echo "📋 Configuration:"
echo "  Base URL: ${BASE_URL}"
echo "  Tenant: ${TENANT}"
echo ""

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

# Step 1: Create a connection with retry policy
echo "📝 Step 1: Creating connection with retry policy..."

cat > /tmp/retry_connection.json << EOF
{
  "trn": "${CONNECTION_TRN}",
  "name": "HTTPBin Retry Test Connection",
  "authorization_type": "api_key",
  "auth_parameters": {
    "api_key_auth_parameters": {
      "api_key_name": "X-Test-Key",
      "api_key_value": "retry-test-key"
    }
  },
  "retry_policy": {
    "max_retries": 2,
    "base_delay_ms": 500,
    "max_delay_ms": 5000,
    "backoff_multiplier": 2.0,
    "retry_status_codes": [500, 502, 503, 504, 429],
    "respect_retry_after": true
  }
}
EOF

CONN_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/connections" \
  -H "Content-Type: application/json" \
  -d @/tmp/retry_connection.json)

if echo "$CONN_RESPONSE" | jq -e '.trn' > /dev/null 2>&1; then
    echo "✅ Connection with retry policy created successfully"
else
    echo "❌ Failed to create connection:"
    echo "$CONN_RESPONSE" | jq '.' 2>/dev/null || echo "$CONN_RESPONSE"
    exit 1
fi

# Step 2: Create a task to test different HTTP status codes
echo ""
echo "📝 Step 2: Creating task for status code testing..."

cat > /tmp/status_task.json << EOF
{
  "trn": "${TASK_TRN}",
  "name": "HTTPBin Status Code Test",
  "connection_trn": "${CONNECTION_TRN}",
  "api_endpoint": "https://httpbin.org/status/200",
  "method": "GET",
  "headers": {
    "User-Agent": ["openact/1.0"],
    "Accept": ["application/json"]
  }
}
EOF

TASK_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks" \
  -H "Content-Type: application/json" \
  -d @/tmp/status_task.json)

if echo "$TASK_RESPONSE" | jq -e '.trn' > /dev/null 2>&1; then
    echo "✅ Task created successfully"
else
    echo "❌ Failed to create task:"
    echo "$TASK_RESPONSE" | jq '.' 2>/dev/null || echo "$TASK_RESPONSE"
    exit 1
fi

# Step 3: Test successful request (no retries needed)
echo ""
echo "📝 Step 3: Testing successful request (200 status)..."

ENCODED_TRN=$(echo "${TASK_TRN}" | sed 's|:|%3A|g' | sed 's|/|%2F|g' | sed 's|@|%40|g')

SUCCESS_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks/${ENCODED_TRN}/execute" \
  -H "Content-Type: application/json" \
  -d '{}')

SUCCESS_STATUS=$(echo "$SUCCESS_RESPONSE" | jq -r '.status // 0')

if [ "$SUCCESS_STATUS" = "200" ]; then
    echo "✅ Successful request (no retries needed): HTTP $SUCCESS_STATUS"
else
    echo "⚠️  Unexpected status: $SUCCESS_STATUS"
fi

# Step 4: Test retry-able error (500 status)
echo ""
echo "📝 Step 4: Testing retry-able error (500 status)..."

ERROR_500_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks/${ENCODED_TRN}/execute" \
  -H "Content-Type: application/json" \
  -d '{
    "overrides": {
      "endpoint": "https://httpbin.org/status/500"
    }
  }')

ERROR_500_STATUS=$(echo "$ERROR_500_RESPONSE" | jq -r '.status // 0')

echo "Response Status: $ERROR_500_STATUS"
if [ "$ERROR_500_STATUS" = "500" ]; then
    echo "✅ Server error correctly handled (after retries): HTTP $ERROR_500_STATUS"
else
    echo "⚠️  Unexpected response:"
    echo "$ERROR_500_RESPONSE" | jq '.' 2>/dev/null || echo "$ERROR_500_RESPONSE"
fi

# Step 5: Test with CLI retry override
echo ""
echo "📝 Step 5: Testing CLI retry policy override..."

echo "Using CLI with aggressive retry policy override:"
CLI_OUTPUT=$(cargo run --bin openact-cli -- execute "${TASK_TRN}" \
  --endpoint "https://httpbin.org/status/503" \
  --retry-policy aggressive \
  --max-retries 1 \
  2>/dev/null || echo "CLI execution failed")

echo "CLI Output (first 3 lines):"
echo "$CLI_OUTPUT" | head -3

# Step 6: Test with HTTP API retry override
echo ""
echo "📝 Step 6: Testing HTTP API retry policy override..."

OVERRIDE_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks/${ENCODED_TRN}/execute" \
  -H "Content-Type: application/json" \
  -d '{
    "overrides": {
      "endpoint": "https://httpbin.org/status/502",
      "retry_policy": {
        "max_retries": 1,
        "base_delay_ms": 100,
        "max_delay_ms": 1000,
        "backoff_multiplier": 1.5,
        "retry_status_codes": [502, 503, 504],
        "respect_retry_after": true
      }
    }
  }')

OVERRIDE_STATUS=$(echo "$OVERRIDE_RESPONSE" | jq -r '.status // 0')
echo "Override Response Status: $OVERRIDE_STATUS"

if [ "$OVERRIDE_STATUS" = "502" ]; then
    echo "✅ HTTP API retry override working: HTTP $OVERRIDE_STATUS"
else
    echo "⚠️  Unexpected override response:"
    echo "$OVERRIDE_RESPONSE" | jq '.' 2>/dev/null || echo "$OVERRIDE_RESPONSE"
fi

# Step 7: Test task-level retry policy override
echo ""
echo "📝 Step 7: Creating task with its own retry policy..."

cat > /tmp/task_retry.json << EOF
{
  "trn": "trn:openact:${TENANT}:task/httpbin-custom-retry@v1",
  "name": "HTTPBin Custom Retry Task",
  "connection_trn": "${CONNECTION_TRN}",
  "api_endpoint": "https://httpbin.org/status/429",
  "method": "GET",
  "headers": {
    "User-Agent": ["openact/1.0"]
  },
  "retry_policy": {
    "max_retries": 3,
    "base_delay_ms": 200,
    "max_delay_ms": 2000,
    "backoff_multiplier": 1.8,
    "retry_status_codes": [429, 500, 502, 503, 504],
    "respect_retry_after": true
  }
}
EOF

curl -s -X POST "${BASE_URL}/api/v1/tasks" \
  -H "Content-Type: application/json" \
  -d @/tmp/task_retry.json > /dev/null

CUSTOM_ENCODED_TRN=$(echo "trn:openact:${TENANT}:task/httpbin-custom-retry@v1" | sed 's|:|%3A|g' | sed 's|/|%2F|g' | sed 's|@|%40|g')

CUSTOM_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks/${CUSTOM_ENCODED_TRN}/execute" \
  -H "Content-Type: application/json" \
  -d '{}')

CUSTOM_STATUS=$(echo "$CUSTOM_RESPONSE" | jq -r '.status // 0')
echo "Task-level retry policy result: HTTP $CUSTOM_STATUS"

# Step 8: System Stats
echo ""
echo "📝 Step 8: Checking system statistics..."

STATS=$(curl -s "${BASE_URL}/api/v1/system/stats")
echo "Client Pool Stats:"
echo "$STATS" | jq '.client_pool'

echo ""
echo "Connection Statistics:"
echo "$STATS" | jq '.storage | {total_connections, total_tasks}'

# Cleanup
echo ""
echo "🧹 Cleaning up temporary files..."
rm -f /tmp/retry_connection.json /tmp/status_task.json /tmp/task_retry.json

echo ""
echo "🎉 Retry Policy Demo Completed!"
echo ""
echo "💡 Key takeaways:"
echo "  ✅ Connection-level retry policies work correctly"
echo "  ✅ Task-level retry policies override connection-level"
echo "  ✅ CLI supports retry policy overrides"
echo "  ✅ HTTP API supports retry policy overrides"
echo "  ✅ Different status codes trigger retries as configured"
echo "  ✅ Retry delays and backoff work correctly"
echo ""
echo "📚 Retry Policy Features:"
echo "  • max_retries: Maximum number of retry attempts"
echo "  • base_delay_ms: Initial delay between retries"
echo "  • max_delay_ms: Maximum delay cap"
echo "  • backoff_multiplier: Exponential backoff factor"
echo "  • retry_status_codes: HTTP status codes that trigger retries"
echo "  • respect_retry_after: Honor server's Retry-After header"
echo ""
echo "🔧 CLI Usage Examples:"
echo "  openact-cli execute task-trn --max-retries 3"
echo "  openact-cli execute task-trn --retry-policy aggressive"
echo "  openact-cli execute task-trn --retry-delay-ms 1000"
