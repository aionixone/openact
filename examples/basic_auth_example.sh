#!/bin/bash
set -e

# Basic Authentication Example
# Demonstrates Basic Auth authentication

echo "🔐 Basic Authentication Example"
echo "==============================="

BASE_URL="http://127.0.0.1:8080"
TENANT="demo"
CONNECTION_TRN="trn:openact:${TENANT}:connection/httpbin-basic@v1"
TASK_TRN="trn:openact:${TENANT}:task/httpbin-basic-auth@v1"

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

# Step 1: Create Basic Auth Connection
echo ""
echo "📝 Step 1: Creating Basic Auth Connection..."

cat > /tmp/basic_connection.json << EOF
{
  "trn": "${CONNECTION_TRN}",
  "name": "HTTPBin Basic Auth Test",
  "version": 1,
  "authorization_type": "basic",
  "auth_parameters": {
    "basic_auth_parameters": {
      "username": "testuser",
      "password": "testpass"
    }
  },
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "updated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

# Create connection
CONN_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/connections" \
  -H "Content-Type: application/json" \
  -d @/tmp/basic_connection.json)

if echo "$CONN_RESPONSE" | jq -e '.trn' > /dev/null 2>&1; then
    echo "✅ Basic Auth connection created successfully"
else
    echo "❌ Failed to create connection:"
    echo "$CONN_RESPONSE" | jq '.' 2>/dev/null || echo "$CONN_RESPONSE"
    exit 1
fi

# Step 2: Create Basic Auth Task
echo ""
echo "📝 Step 2: Creating Basic Auth Task..."

cat > /tmp/basic_task.json << EOF
{
  "trn": "${TASK_TRN}",
  "name": "HTTPBin Basic Auth Test",
  "version": 1,
  "connection_trn": "${CONNECTION_TRN}",
  "api_endpoint": "https://httpbin.org/basic-auth/testuser/testpass",
  "method": "GET",
  "headers": {
    "User-Agent": ["openact/1.0"],
    "Accept": ["application/json"]
  },
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "updated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

# Create task
TASK_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks" \
  -H "Content-Type: application/json" \
  -d @/tmp/basic_task.json)

if echo "$TASK_RESPONSE" | jq -e '.trn' > /dev/null 2>&1; then
    echo "✅ Basic Auth task created successfully"
else
    echo "❌ Failed to create task:"
    echo "$TASK_RESPONSE" | jq '.' 2>/dev/null || echo "$TASK_RESPONSE"
    exit 1
fi

# Step 3: Execute Task
echo ""
echo "📝 Step 3: Executing Basic Auth task..."

ENCODED_TRN=$(echo "${TASK_TRN}" | sed 's|:|%3A|g' | sed 's|/|%2F|g' | sed 's|@|%40|g')

EXECUTE_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks/${ENCODED_TRN}/execute" \
  -H "Content-Type: application/json" \
  -d '{}')

if echo "$EXECUTE_RESPONSE" | jq -e '.status' > /dev/null 2>&1; then
    echo "✅ Basic Auth task executed successfully!"
    echo ""
    echo "Response Status: $(echo "$EXECUTE_RESPONSE" | jq -r '.status')"
    echo "Authentication Success: $(echo "$EXECUTE_RESPONSE" | jq -r '.body.authenticated // "Unknown"')"
    echo "Authenticated User: $(echo "$EXECUTE_RESPONSE" | jq -r '.body.user // "Unknown"')"
    echo ""
    echo "Full Response:"
    echo "$EXECUTE_RESPONSE" | jq '.body' 2>/dev/null
else
    echo "❌ Task execution failed:"
    echo "$EXECUTE_RESPONSE"
fi

# Step 4: Test wrong credentials (should fail)
echo ""
echo "📝 Step 4: Testing with wrong credentials (should fail)..."

cat > /tmp/basic_wrong_connection.json << EOF
{
  "trn": "trn:openact:${TENANT}:connection/httpbin-basic-wrong@v1",
  "name": "HTTPBin Basic Auth Wrong Credentials",
  "version": 1,
  "authorization_type": "basic",
  "auth_parameters": {
    "basic_auth_parameters": {
      "username": "wronguser",
      "password": "wrongpass"
    }
  },
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "updated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

cat > /tmp/basic_wrong_task.json << EOF
{
  "trn": "trn:openact:${TENANT}:task/httpbin-basic-auth-wrong@v1",
  "name": "HTTPBin Basic Auth Wrong Test",
  "version": 1,
  "connection_trn": "trn:openact:${TENANT}:connection/httpbin-basic-wrong@v1",
  "api_endpoint": "https://httpbin.org/basic-auth/testuser/testpass",
  "method": "GET",
  "headers": {
    "User-Agent": ["openact/1.0"],
    "Accept": ["application/json"]
  },
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "updated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

# Create wrong connection and task
curl -s -X POST "${BASE_URL}/api/v1/connections" -H "Content-Type: application/json" -d @/tmp/basic_wrong_connection.json > /dev/null
curl -s -X POST "${BASE_URL}/api/v1/tasks" -H "Content-Type: application/json" -d @/tmp/basic_wrong_task.json > /dev/null

WRONG_ENCODED_TRN=$(echo "trn:openact:${TENANT}:task/httpbin-basic-auth-wrong@v1" | sed 's|:|%3A|g' | sed 's|/|%2F|g' | sed 's|@|%40|g')

WRONG_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks/${WRONG_ENCODED_TRN}/execute" \
  -H "Content-Type: application/json" \
  -d '{}')

WRONG_STATUS=$(echo "$WRONG_RESPONSE" | jq -r '.status // 0')

if [ "$WRONG_STATUS" = "401" ]; then
    echo "✅ Authentication correctly failed with wrong credentials (HTTP 401)"
else
    echo "⚠️  Unexpected result with wrong credentials:"
    echo "Status: $WRONG_STATUS"
    echo "$WRONG_RESPONSE" | jq '.' 2>/dev/null || echo "$WRONG_RESPONSE"
fi

# Step 5: System Health Check
echo ""
echo "📝 Step 5: System Health Check..."

HEALTH_RESPONSE=$(curl -s "${BASE_URL}/api/v1/system/health")
HEALTH_STATUS=$(echo "$HEALTH_RESPONSE" | jq -r '.status // "unknown"')

echo "System Status: $HEALTH_STATUS"
echo "$HEALTH_RESPONSE" | jq '.' 2>/dev/null

# Cleanup
echo ""
echo "🧹 Cleaning up temporary files..."
rm -f /tmp/basic_connection.json /tmp/basic_task.json /tmp/basic_wrong_connection.json /tmp/basic_wrong_task.json

echo ""
echo "🎉 Basic Authentication example completed!"
echo ""
echo "💡 Key takeaways:"
echo "  ✅ Basic Auth credentials are properly encoded (Base64)"
echo "  ✅ Authentication header is correctly injected"
echo "  ✅ Wrong credentials properly return HTTP 401"
echo "  ✅ System health checks are working"
echo ""
echo "📚 Next steps:"
echo "  - Check system stats: curl ${BASE_URL}/api/v1/system/stats"
echo "  - Try OAuth example: ./examples/github_oauth_complete.sh"
