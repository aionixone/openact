#!/bin/bash
set -e

# GitHub OAuth Complete Example
# This script demonstrates a complete GitHub OAuth2 Authorization Code flow

echo "🚀 GitHub OAuth Complete Example"
echo "================================"

# Configuration
GITHUB_CLIENT_ID="${GITHUB_CLIENT_ID:-Ov23lihVkExosE0hR0Bh}"
GITHUB_CLIENT_SECRET="${GITHUB_CLIENT_SECRET:-20e4ad84113f1e2537f2581c25ecb0526ed06b55}"
BASE_URL="http://127.0.0.1:8080"
TENANT="demo"
CONNECTION_TRN="trn:openact:${TENANT}:connection/github-oauth@v1"
TASK_TRN="trn:openact:${TENANT}:task/github-user@v1"

echo "📋 Configuration:"
echo "  Client ID: ${GITHUB_CLIENT_ID}"
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

# Step 1: Create GitHub OAuth Connection
echo ""
echo "📝 Step 1: Creating GitHub OAuth Connection..."

cat > /tmp/github_connection.json << EOF
{
  "trn": "${CONNECTION_TRN}",
  "name": "GitHub OAuth Connection",
  "version": 1,
  "authorization_type": "oauth2_authorization_code",
  "auth_parameters": {
    "oauth_parameters": {
      "client_id": "${GITHUB_CLIENT_ID}",
      "client_secret": "${GITHUB_CLIENT_SECRET}",
      "token_url": "https://github.com/login/oauth/access_token",
      "scope": "user:email",
      "redirect_uri": "http://localhost:8080/oauth/callback",
      "use_pkce": false
    }
  },
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "updated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

# Create connection
CONN_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/connections" \
  -H "Content-Type: application/json" \
  -d @/tmp/github_connection.json)

if echo "$CONN_RESPONSE" | jq -e '.trn' > /dev/null 2>&1; then
    echo "✅ Connection created successfully"
else
    echo "❌ Failed to create connection:"
    echo "$CONN_RESPONSE" | jq '.' 2>/dev/null || echo "$CONN_RESPONSE"
    exit 1
fi

# Step 2: Create GitHub User Task
echo ""
echo "📝 Step 2: Creating GitHub User Task..."

cat > /tmp/github_task.json << EOF
{
  "trn": "${TASK_TRN}",
  "name": "Get GitHub User",
  "version": 1,
  "connection_trn": "${CONNECTION_TRN}",
  "api_endpoint": "https://api.github.com/user",
  "method": "GET",
  "headers": {
    "User-Agent": ["openact/1.0"],
    "Accept": ["application/vnd.github.v3+json"]
  },
  "created_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "updated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
}
EOF

# Create task
TASK_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/tasks" \
  -H "Content-Type: application/json" \
  -d @/tmp/github_task.json)

if echo "$TASK_RESPONSE" | jq -e '.trn' > /dev/null 2>&1; then
    echo "✅ Task created successfully"
else
    echo "❌ Failed to create task:"
    echo "$TASK_RESPONSE" | jq '.' 2>/dev/null || echo "$TASK_RESPONSE"
    exit 1
fi

# Step 3: Create OAuth Workflow
echo ""
echo "📝 Step 3: Creating OAuth Workflow..."

cat > /tmp/github_workflow.json << EOF
{
  "name": "GitHub OAuth Flow",
  "description": "Complete GitHub OAuth2 Authorization Code flow",
  "dsl": {
    "provider": {
      "name": "github",
      "flows": {
        "authorize": {
          "startAt": "StartAuth",
          "states": {
            "StartAuth": {
              "type": "task",
              "resource": "oauth2.authorize_redirect",
              "parameters": {
                "authorizeUrl": "https://github.com/login/oauth/authorize",
                "clientId": "${GITHUB_CLIENT_ID}",
                "redirectUri": "http://localhost:8080/oauth/callback",
                "scope": "user:email",
                "usePKCE": false
              },
              "assign": {
                "auth_state": "\$.state",
                "code_verifier": "\$.code_verifier"
              },
              "next": "AwaitCallback"
            },
            "AwaitCallback": {
              "type": "task",
              "resource": "oauth2.await_callback",
              "assign": {
                "callback_code": "\$.code"
              },
              "next": "ExchangeToken"
            },
            "ExchangeToken": {
              "type": "task",
              "resource": "oauth2.exchange_token",
              "parameters": {
                "tokenUrl": "https://github.com/login/oauth/access_token",
                "clientId": "${GITHUB_CLIENT_ID}",
                "clientSecret": "${GITHUB_CLIENT_SECRET}",
                "redirectUri": "http://localhost:8080/oauth/callback",
                "code": "\$.callback_code",
                "codeVerifier": "\$.code_verifier"
              },
              "assign": {
                "access_token": "\$.access_token",
                "user_login": "\$.user_login"
              },
              "next": "PersistConnection"
            },
            "PersistConnection": {
              "type": "task",
              "resource": "connection.update",
              "parameters": {
                "connection_ref": "trn:openact:${TENANT}:auth_connection/github_oauth2_demo",
                "access_token": "\$.access_token",
                "refresh_token": "\$.refresh_token",
                "expires_in": "\$.expires_in",
                "token_type": "\$.token_type",
                "scope": "\$.scope"
              },
              "next": "Success"
            },
            "Success": {
              "type": "succeed"
            }
          }
        }
      }
    }
  }
}
EOF

# Create workflow
WORKFLOW_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/authflow/workflows" \
  -H "Content-Type: application/json" \
  -d @/tmp/github_workflow.json)

WORKFLOW_ID=$(echo "$WORKFLOW_RESPONSE" | jq -r '.id // empty')

if [ -n "$WORKFLOW_ID" ]; then
    echo "✅ Workflow created with ID: $WORKFLOW_ID"
else
    echo "❌ Failed to create workflow:"
    echo "$WORKFLOW_RESPONSE" | jq '.' 2>/dev/null || echo "$WORKFLOW_RESPONSE"
    exit 1
fi

# Step 4: Start OAuth Flow
echo ""
echo "📝 Step 4: Starting OAuth Flow..."

START_REQUEST=$(cat << EOF
{
  "workflow_id": "$WORKFLOW_ID",
  "flow": "authorize",
  "input": {
    "tenant": "${TENANT}"
  }
}
EOF
)

EXECUTION_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/authflow/executions" \
  -H "Content-Type: application/json" \
  -d "$START_REQUEST")

EXECUTION_ID=$(echo "$EXECUTION_RESPONSE" | jq -r '.execution_id // empty')

if [ -n "$EXECUTION_ID" ]; then
    echo "✅ OAuth flow started with execution ID: $EXECUTION_ID"
else
    echo "❌ Failed to start OAuth flow:"
    echo "$EXECUTION_RESPONSE" | jq '.' 2>/dev/null || echo "$EXECUTION_RESPONSE"
    exit 1
fi

# Step 5: Get Authorization URL
echo ""
echo "📝 Step 5: Getting Authorization URL..."

sleep 2  # Wait for execution to process

EXECUTION_STATUS=$(curl -s "${BASE_URL}/api/v1/authflow/executions/${EXECUTION_ID}")
AUTH_URL=$(echo "$EXECUTION_STATUS" | jq -r '.pending_info.authorize_url // empty')
AUTH_STATE=$(echo "$EXECUTION_STATUS" | jq -r '.pending_info.state // empty')

if [ -n "$AUTH_URL" ]; then
    echo "✅ Authorization URL generated:"
    echo "🔗 $AUTH_URL"
    echo ""
    echo "📋 Please:"
    echo "1. Open the above URL in your browser"
    echo "2. Authorize the application"
    echo "3. You will be redirected to the callback URL"
    echo "4. Copy the 'code' parameter from the callback URL"
    echo ""
    echo -n "Enter the authorization code: "
    read -r AUTH_CODE
else
    echo "❌ Failed to get authorization URL:"
    echo "$EXECUTION_STATUS" | jq '.' 2>/dev/null || echo "$EXECUTION_STATUS"
    exit 1
fi

# Step 6: Resume with Authorization Code
echo ""
echo "📝 Step 6: Resuming flow with authorization code..."

RESUME_REQUEST=$(cat << EOF
{
  "input": {
    "code": "$AUTH_CODE",
    "state": "$AUTH_STATE"
  }
}
EOF
)

RESUME_RESPONSE=$(curl -s -X POST "${BASE_URL}/api/v1/authflow/executions/${EXECUTION_ID}/resume" \
  -H "Content-Type: application/json" \
  -d "$RESUME_REQUEST")

echo "Resume response:"
echo "$RESUME_RESPONSE" | jq '.' 2>/dev/null || echo "$RESUME_RESPONSE"

# Step 7: Check Final Status
echo ""
echo "📝 Step 7: Checking final execution status..."

sleep 3  # Wait for completion

FINAL_STATUS=$(curl -s "${BASE_URL}/api/v1/authflow/executions/${EXECUTION_ID}")
STATUS=$(echo "$FINAL_STATUS" | jq -r '.status // empty')

echo "Final execution status: $STATUS"

if [ "$STATUS" = "completed" ]; then
    echo "✅ OAuth flow completed successfully!"
    
    # Step 8: Test the connection by executing the task
    echo ""
    echo "📝 Step 8: Testing the OAuth connection..."
    
    TASK_RESULT=$(curl -s -X POST "${BASE_URL}/api/v1/tasks/$(echo "${TASK_TRN}" | sed 's|:|%3A|g' | sed 's|/|%2F|g' | sed 's|@|%40|g')/execute" \
      -H "Content-Type: application/json" \
      -d '{}')
    
    if echo "$TASK_RESULT" | jq -e '.status' > /dev/null 2>&1; then
        echo "✅ Task execution successful!"
        echo "GitHub user info:"
        echo "$TASK_RESULT" | jq '.body' 2>/dev/null || echo "$TASK_RESULT"
    else
        echo "❌ Task execution failed:"
        echo "$TASK_RESULT"
    fi
else
    echo "❌ OAuth flow failed or is still running"
    echo "Execution details:"
    echo "$FINAL_STATUS" | jq '.' 2>/dev/null || echo "$FINAL_STATUS"
fi

# Cleanup
echo ""
echo "🧹 Cleaning up temporary files..."
rm -f /tmp/github_connection.json /tmp/github_task.json /tmp/github_workflow.json

echo ""
echo "🎉 GitHub OAuth example completed!"
echo ""
echo "📊 To check system stats:"
echo "  curl ${BASE_URL}/api/v1/system/stats"
echo ""
echo "📝 To list connections:"
echo "  curl ${BASE_URL}/api/v1/connections"
echo ""
echo "📝 To list tasks:"
echo "  curl ${BASE_URL}/api/v1/tasks"
