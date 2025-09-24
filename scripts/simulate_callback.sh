#!/bin/bash

# Simulate GitHub OAuth2 callback script
# Used to test the complete OAuth2 process

set -e

if [ $# -ne 1 ]; then
    echo "Usage: $0 <execution_id>"
    echo "Example: $0 exec_123456"
    exit 1
fi

EXECUTION_ID="$1"
BASE_URL="http://localhost:8080/api/v1"

echo "🔄 Simulating GitHub OAuth2 callback"
echo "=========================="
echo "📋 Execution ID: $EXECUTION_ID"

# Simulate authorization code (in a real scenario, this comes from GitHub's callback)
MOCK_CODE="mock_auth_code_$(date +%s)"

echo "🔑 Simulated authorization code: $MOCK_CODE"

# Resume execution
echo ""
echo "🚀 Resuming execution process..."
RESUME_RESPONSE=$(curl -s -X POST "$BASE_URL/executions/$EXECUTION_ID/resume" \
  -H "Content-Type: application/json" \
  -d "{
    \"code\": \"$MOCK_CODE\"
  }")

echo "📊 Resume response:"
echo "$RESUME_RESPONSE" | jq '.'

# Wait for processing to complete
echo ""
echo "⏳ Waiting for process to complete..."
sleep 3

# Check final status
echo ""
echo "🔍 Checking final execution status..."
FINAL_STATUS=$(curl -s "$BASE_URL/executions/$EXECUTION_ID")
STATUS=$(echo "$FINAL_STATUS" | jq -r '.status')

echo "📊 Final status: $STATUS"

if [ "$STATUS" = "completed" ]; then
    echo "✅ Process execution completed!"
    echo ""
    echo "📋 Execution result:"
    echo "$FINAL_STATUS" | jq '.'
    
    # Check for connection records
    echo ""
    echo "🔍 Checking connection records in the database..."
    CONNECTIONS_RESPONSE=$(curl -s "$BASE_URL/connections?tenant=test-tenant&provider=github")
    echo "📊 Connection records:"
    echo "$CONNECTIONS_RESPONSE" | jq '.'
    
else
    echo "⚠️  Process status: $STATUS"
    echo "📋 Detailed information:"
    echo "$FINAL_STATUS" | jq '.'
fi

echo ""
echo "🎯 Callback simulation completed!"
