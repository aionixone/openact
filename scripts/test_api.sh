#!/bin/bash

# openact API test script

BASE_URL="http://localhost:8080/api/v1"

echo "🧪 openact API Test"
echo "==================="

# Health check
echo "1. Health check..."
curl -s "$BASE_URL/health" | jq '.'
echo ""

# Create workflow
echo "2. Create GitHub OAuth2 workflow..."
WORKFLOW_RESPONSE=$(curl -s -X POST "$BASE_URL/workflows" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "GitHub OAuth2 Test",
    "description": "Test GitHub OAuth2 authentication flow",
    "dsl": {
      "version": "1.0",
      "provider": {
        "name": "github",
        "providerType": "oauth2",
        "config": {
          "authorizeUrl": "https://github.com/login/oauth/authorize",
          "tokenUrl": "https://github.com/login/oauth/access_token"
        }
      },
      "flows": {
        "obtain": {
          "startAt": "StartAuth",
          "states": {
            "StartAuth": {
              "type": "task",
              "resource": "oauth2.authorize_redirect",
              "parameters": {
                "clientId": "test_client_id",
                "scope": "user:email"
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
  }')

echo "$WORKFLOW_RESPONSE" | jq '.'
WORKFLOW_ID=$(echo "$WORKFLOW_RESPONSE" | jq -r '.id')
echo "Workflow ID: $WORKFLOW_ID"
echo ""

# Get workflow list
echo "3. Get workflow list..."
curl -s "$BASE_URL/workflows" | jq '.'
echo ""

# Get workflow graph
echo "4. Get workflow graph..."
curl -s "$BASE_URL/workflows/$WORKFLOW_ID/graph" | jq '.'
echo ""

# Validate workflow
echo "5. Validate workflow..."
curl -s -X POST "$BASE_URL/workflows/$WORKFLOW_ID/validate" | jq '.'
echo ""

# Start execution
echo "6. Start workflow execution..."
EXECUTION_RESPONSE=$(curl -s -X POST "$BASE_URL/executions" \
  -H "Content-Type: application/json" \
  -d "{
    \"workflowId\": \"$WORKFLOW_ID\",
    \"flow\": \"obtain\",
    \"input\": {
      \"userId\": \"test_user_123\",
      \"redirectUrl\": \"http://localhost:3000/callback\"
    }
  }")

echo "$EXECUTION_RESPONSE" | jq '.'
EXECUTION_ID=$(echo "$EXECUTION_RESPONSE" | jq -r '.executionId')
echo "Execution ID: $EXECUTION_ID"
echo ""

# Wait for execution to complete
sleep 2

# Get execution status
echo "7. Get execution status..."
curl -s "$BASE_URL/executions/$EXECUTION_ID" | jq '.'
echo ""

# Get execution trace
echo "8. Get execution trace..."
curl -s "$BASE_URL/executions/$EXECUTION_ID/trace" | jq '.'
echo ""

# Get execution list
echo "9. Get execution list..."
curl -s "$BASE_URL/executions" | jq '.'
echo ""

echo "✅ API test completed!"
echo ""
echo "💡 Tips:"
echo "  - Use 'cargo run --example workflow_server_demo --features server' to start the server"
echo "  - Use a WebSocket client to connect to ws://localhost:8080/api/v1/ws/executions for real-time updates"
