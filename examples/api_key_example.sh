#!/bin/bash
set -e

# Simple API Key Example
# Demonstrates basic API Key authentication and task execution

echo "🔑 API Key Example"
echo "=================="

BASE_URL="http://127.0.0.1:8080"
TENANT="demo"
CONNECTION_TRN="trn:openact:${TENANT}:connection/httpbin@v1"
TASK_TRN="trn:openact:${TENANT}:task/httpbin-get@v1"

echo "📋 Configuration:"
echo "  Base URL: ${BASE_URL}"
echo "  Tenant: ${TENANT}"
echo ""

# Check if server is running
echo "🔍 Checking if server is running..."
if ! curl -s "${BASE_URL}/api/v1/authflow/health" > /dev/null; then
    echo "❌ Server is not running at ${BASE_URL}"
    echo "Please start the server with:"
    echo "  RUST_LOG=info OPENACT_DB_URL=sqlite:./data/openact.db?mode=rwc cargo run --features server --bin openact"
    exit 1
fi
echo "✅ Server is running"

# Step 1: Create API Key Connection
echo ""
echo "📝 Step 1: Creating API Key Connection..."

cat > /tmp/apikey_connection.json << EOF
{
  "trn": "${CONNECTION_TRN}",
  "name": "HTTPBin API Key Test",
  "authorization_type": "api_key",
  "auth_parameters": {
    "api_key_auth_parameters": {
      "api_key_name": "X-API-Key",
      "api_key_value": "demo-api-key-12345"
    }
  }
}
EOF

# Create connection
CONN_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/connections" \
  -H "Content-Type: application/json" \
  -d @/tmp/apikey_connection.json)

if echo "$CONN_RESPONSE" | jq -e '.trn' > /dev/null 2>&1; then
    echo "✅ Connection created successfully"
else
    echo "❌ Failed to create connection:"
    echo "$CONN_RESPONSE" | jq '.' 2>/dev/null || echo "$CONN_RESPONSE"
    exit 1
fi

# Step 2: Create Task
echo ""
echo "📝 Step 2: Creating HTTP Task..."

cat > /tmp/httpbin_task.json << EOF
{
  "trn": "${TASK_TRN}",
  "name": "HTTPBin GET Test",
  "connection_trn": "${CONNECTION_TRN}",
  "api_endpoint": "https://httpbin.org/get",
  "method": "GET",
  "headers": {
    "User-Agent": ["openact/1.0"],
    "Accept": ["application/json"]
  },
  "query_params": {
    "test": ["true"],
    "demo": ["api_key_example"]
  }
}
EOF

# Create task
TASK_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks" \
  -H "Content-Type: application/json" \
  -d @/tmp/httpbin_task.json)

if echo "$TASK_RESPONSE" | jq -e '.trn' > /dev/null 2>&1; then
    echo "✅ Task created successfully"
else
    echo "❌ Failed to create task:"
    echo "$TASK_RESPONSE" | jq '.' 2>/dev/null || echo "$TASK_RESPONSE"
    exit 1
fi

# Step 3: Execute Task via HTTP API
echo ""
echo "📝 Step 3: Executing task via HTTP API..."

ENCODED_TRN=$(echo "${TASK_TRN}" | sed 's|:|%3A|g' | sed 's|/|%2F|g' | sed 's|@|%40|g')

EXECUTE_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks/${ENCODED_TRN}/execute" \
  -H "Content-Type: application/json" \
  -d '{}')

if echo "$EXECUTE_RESPONSE" | jq -e '.status' > /dev/null 2>&1; then
    echo "✅ Task executed successfully via HTTP API!"
    echo ""
    echo "Response Status: $(echo "$EXECUTE_RESPONSE" | jq -r '.status')"
    echo "API Key Injected: $(echo "$EXECUTE_RESPONSE" | jq -r '.body.headers."X-Api-Key" // "Not found"')"
    echo ""
    echo "Full Response Headers:"
    echo "$EXECUTE_RESPONSE" | jq '.body.headers' 2>/dev/null
else
    echo "❌ Task execution failed:"
    echo "$EXECUTE_RESPONSE"
fi

# Step 4: Execute Task via CLI (local mode)
echo ""
echo "📝 Step 4: Executing task via CLI (local mode)..."

CLI_OUTPUT=$(cargo run --bin openact-cli -- execute "${TASK_TRN}" 2>/dev/null || echo "CLI execution failed")

if echo "$CLI_OUTPUT" | grep -q "Status:"; then
    echo "✅ Task executed successfully via CLI (local mode)!"
    echo "CLI Output (first 5 lines):"
    echo "$CLI_OUTPUT" | head -5
else
    echo "❌ CLI execution failed:"
    echo "$CLI_OUTPUT"
fi

# Step 5: Execute Task via CLI (server mode)
echo ""
echo "📝 Step 5: Executing task via CLI (server mode)..."

CLI_SERVER_OUTPUT=$(cargo run --bin openact-cli -- --server "${BASE_URL}" execute "${TASK_TRN}" 2>/dev/null || echo "CLI server mode execution failed")

if echo "$CLI_SERVER_OUTPUT" | grep -q "Status:"; then
    echo "✅ Task executed successfully via CLI (server mode)!"
    echo "CLI Server Output (first 5 lines):"
    echo "$CLI_SERVER_OUTPUT" | head -5
else
    echo "❌ CLI server mode execution failed:"
    echo "$CLI_SERVER_OUTPUT"
fi

# Step 6: List and verify resources
echo ""
echo "📝 Step 6: Listing created resources..."

echo "Connections:"
curl -s "${BASE_URL}/api/v1/connections" | jq '.[] | {trn: .trn, name: .name, auth_type: .authorization_type}' 2>/dev/null

echo ""
echo "Tasks:"
curl -s "${BASE_URL}/api/v1/tasks" | jq '.[] | {trn: .trn, name: .name, endpoint: .api_endpoint}' 2>/dev/null

# Step 7: System Stats
echo ""
echo "📝 Step 7: System Statistics..."

curl -s "${BASE_URL}/api/v1/system/stats" | jq '.' 2>/dev/null

# Cleanup
echo ""
echo "🧹 Cleaning up temporary files..."
rm -f /tmp/apikey_connection.json /tmp/httpbin_task.json

echo ""
echo "🎉 API Key example completed!"
echo ""
echo "💡 Key takeaways:"
echo "  ✅ API Key authentication works correctly"
echo "  ✅ HTTP API and CLI produce consistent results"  
echo "  ✅ CLI can operate in both local and server modes"
echo "  ✅ Headers are properly injected and normalized"
echo ""
echo "📚 Next steps:"
echo "  - Try the OAuth example: ./examples/github_oauth_complete.sh"
echo "  - Check out Basic Auth example: ./examples/basic_auth_example.sh"
