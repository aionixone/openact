#!/bin/bash

# GitHub OAuth2 Real Complete Flow Test Script
# Includes real user authorization and database persistence

set -e

BASE_URL="http://localhost:8080/api/v1"

echo "🚀 GitHub OAuth2 Real Complete Flow Test"
echo "========================================"

# Check environment variables
if [ -z "$GITHUB_CLIENT_ID" ]; then
    echo "❌ Error: Please set the GITHUB_CLIENT_ID environment variable"
    echo "💡 How to set: export GITHUB_CLIENT_ID=your_client_id"
    exit 1
fi

if [ -z "$GITHUB_CLIENT_SECRET" ]; then
    echo "❌ Error: Please set the GITHUB_CLIENT_SECRET environment variable"
    echo "💡 How to set: export GITHUB_CLIENT_SECRET=your_client_secret"
    exit 1
fi

echo "✅ Environment variables check passed"
echo "   Client ID: ${GITHUB_CLIENT_ID:0:8}..."

# Check if the server is running
echo ""
echo "🔍 Checking openact server status..."
if ! curl -s "$BASE_URL/health" > /dev/null; then
    echo "❌ Error: openact server is not running"
    echo "💡 Please start the server: cargo run --features server"
    exit 1
fi
echo "✅ openact server is running"

# 1. Create workflow
echo ""
echo "📋 Step 1: Create GitHub OAuth2 workflow..."
WORKFLOW_RESPONSE=$(curl -s -X POST "$BASE_URL/workflows" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "GitHub OAuth2 Real Test",
    "description": "Real GitHub OAuth2 authentication flow test",
    "dsl": {
      "version": "1.0",
      "provider": {
        "name": "github",
        "providerType": "oauth2",
        "flows": {
          "OAuth": {
            "startAt": "Config",
            "states": {
              "Config": {
                "type": "pass",
                "assign": {
                  "config": {
                    "authorizeUrl": "https://github.com/login/oauth/authorize",
                    "tokenUrl": "https://github.com/login/oauth/access_token",
                    "redirectUri": "http://localhost:8080/oauth/callback",
                    "defaultScope": "user:email"
                  },
                  "creds": {
                    "client_id": "{% vars.secrets.github_client_id %}",
                    "client_secret": "{% vars.secrets.github_client_secret %}"
                  }
                },
                "next": "StartAuth"
              },
              "StartAuth": {
                "type": "task",
                "resource": "oauth2.authorize_redirect",
                "parameters": {
                  "authorizeUrl": "{% $config.authorizeUrl %}",
                  "clientId": "{% $creds.client_id %}",
                  "redirectUri": "{% $config.redirectUri %}",
                  "scope": "{% $config.defaultScope %}",
                  "usePKCE": true
                },
                "assign": {
                  "auth_state": "{% result.state %}",
                  "code_verifier": "{% result.code_verifier %}"
                },
                "next": "AwaitCallback"
              },
              "AwaitCallback": {
                "type": "task",
                "resource": "oauth2.await_callback",
                "assign": {
                  "callback_code": "{% result.code %}"
                },
                "next": "ExchangeToken"
              },
              "ExchangeToken": {
                "type": "task",
                "resource": "http.request",
                "parameters": {
                  "method": "POST",
                  "url": "{% $config.tokenUrl %}",
                  "headers": {
                    "Content-Type": "application/x-www-form-urlencoded",
                    "Accept": "application/json"
                  },
                  "body": {
                    "grant_type": "authorization_code",
                    "client_id": "{% $creds.client_id %}",
                    "client_secret": "{% $creds.client_secret %}",
                    "redirect_uri": "{% $config.redirectUri %}",
                    "code": "{% $callback_code %}",
                    "code_verifier": "{% $code_verifier %}"
                  }
                },
                "assign": {
                  "access_token": "{% result.body.access_token %}",
                  "refresh_token": "{% result.body.refresh_token %}",
                  "token_type": "{% result.body.token_type %}",
                  "scope": "{% result.body.scope %}"
                },
                "output": {
                  "access_token": "{% $access_token %}",
                  "refresh_token": "{% $refresh_token ? $refresh_token : null %}",
                  "token_type": "{% $token_type ? $token_type : '\''bearer'\'' %}",
                  "scope": "{% $scope ? $scope : '\'''\'' %}"
                },
                "next": "GetUser"
              },
              "GetUser": {
                "type": "task",
                "resource": "http.request",
                "parameters": {
                  "method": "GET",
                  "url": "https://api.github.com/user",
                  "headers": {
                    "Authorization": "{% '\''Bearer '\'' & $access_token %}",
                    "Accept": "application/vnd.github+json",
                    "User-Agent": "openact/0.1"
                  }
                },
                "assign": {
                  "user_login": "{% result.body.login %}"
                },
                "next": "PersistConnection"
              },
              "PersistConnection": {
                "type": "task",
                "resource": "connection.update",
                "parameters": {
                  "tenant": "{% input.tenant %}",
                  "provider": "github",
                  "user_id": "{% $user_login %}",
                  "access_token": "{% $access_token %}",
                  "refresh_token": "{% $refresh_token %}",
                  "token_type": "{% $token_type %}",
                  "scope": "{% $scope %}"
                },
                "end": true
              }
            }
          }
        }
      }
    }
  }')

WORKFLOW_ID=$(echo "$WORKFLOW_RESPONSE" | jq -r '.id')
if [ "$WORKFLOW_ID" = "null" ] || [ -z "$WORKFLOW_ID" ]; then
    echo "❌ Failed to create workflow:"
    echo "$WORKFLOW_RESPONSE" | jq '.'
    exit 1
fi

echo "✅ Workflow created successfully: $WORKFLOW_ID"

# 2. Start execution
echo ""
echo "🚀 Step 2: Start OAuth2 flow execution..."
EXECUTION_RESPONSE=$(curl -s -X POST "$BASE_URL/executions" \
  -H "Content-Type: application/json" \
  -d "{
    \"workflowId\": \"$WORKFLOW_ID\",
    \"flow\": \"OAuth\",
    \"input\": {
      \"tenant\": \"test-tenant\",
      \"redirectUri\": \"http://localhost:8080/oauth/callback\"
    },
    \"context\": {
      \"secrets\": {
        \"github_client_id\": \"$GITHUB_CLIENT_ID\",
        \"github_client_secret\": \"$GITHUB_CLIENT_SECRET\"
      }
    }
  }")

EXECUTION_ID=$(echo "$EXECUTION_RESPONSE" | jq -r '.executionId')
if [ "$EXECUTION_ID" = "null" ] || [ -z "$EXECUTION_ID" ]; then
    echo "❌ Failed to start execution:"
    echo "$EXECUTION_RESPONSE" | jq '.'
    exit 1
fi

echo "✅ Execution started successfully: $EXECUTION_ID"

# 3. Check execution status and get authorization URL
echo ""
echo "⏳ Step 3: Check execution status..."
sleep 2

STATUS_RESPONSE=$(curl -s "$BASE_URL/executions/$EXECUTION_ID")
STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.status')

echo "📊 Current status: $STATUS"

if [ "$STATUS" = "paused" ]; then
    echo "✅ Flow paused, waiting for user authorization"
    
    # Get authorization URL
    AUTHORIZE_URL=$(echo "$STATUS_RESPONSE" | jq -r '.context.states.StartAuth.result.authorize_url')
    if [ "$AUTHORIZE_URL" != "null" ] && [ -n "$AUTHORIZE_URL" ]; then
        echo ""
        echo "🔗 Authorization URL:"
        echo "$AUTHORIZE_URL"
        echo ""
        echo "📝 Next steps:"
        echo "   1. Visit the authorization URL above in your browser"
        echo "   2. Log in to GitHub and authorize the application"
        echo "   3. GitHub will redirect to the callback URL"
        echo "   4. After authorization is complete, press any key to continue..."
        echo ""
        read -p "Press Enter to continue (ensure authorization is complete)..."
        
        # 4. Simulate obtaining the authorization code (in a real scenario, this comes from the callback)
        echo ""
        echo "🔄 Step 4: Obtain authorization code..."
        
        # Here we need to obtain the real authorization code from the callback
        # In a real scenario, this should come from the callback server's handling
        echo "💡 In real use, the authorization code is automatically obtained via the callback URL"
        echo "💡 Now we will use a simulated authorization code to demonstrate the complete flow"
        
        # Prompt user to enter the authorization code
        echo ""
        read -p "Enter the authorization code obtained from the GitHub callback: " AUTH_CODE
        
        if [ -z "$AUTH_CODE" ]; then
            echo "❌ No authorization code provided, using simulated authorization code"
            AUTH_CODE="mock_auth_code_$(date +%s)"
        fi
        
        echo "🔑 Using authorization code: $AUTH_CODE"
        
        # 5. Resume execution
        echo ""
        echo "🚀 Step 5: Resume execution flow..."
        RESUME_RESPONSE=$(curl -s -X POST "$BASE_URL/executions/$EXECUTION_ID/resume" \
          -H "Content-Type: application/json" \
          -d "{\"code\": \"$AUTH_CODE\"}")
        
        echo "📊 Resume response:"
        echo "$RESUME_RESPONSE" | jq '.'
        
        # 6. Wait for processing to complete
        echo ""
        echo "⏳ Step 6: Wait for flow processing to complete..."
        sleep 5
        
        # 7. Check final status
        echo ""
        echo "🔍 Step 7: Check final execution status..."
        FINAL_STATUS=$(curl -s "$BASE_URL/executions/$EXECUTION_ID")
        FINAL_STATUS_VALUE=$(echo "$FINAL_STATUS" | jq -r '.status')
        
        echo "📊 Final status: $FINAL_STATUS_VALUE"
        
        if [ "$FINAL_STATUS_VALUE" = "completed" ]; then
            echo "🎉 Flow execution completed!"
            echo ""
            echo "📋 Execution result:"
            echo "$FINAL_STATUS" | jq '.'
            
            # 8. Check connection records in the database
            echo ""
            echo "🔍 Step 8: Check connection records in the database..."
            CONNECTIONS_RESPONSE=$(curl -s "$BASE_URL/connections?tenant=test-tenant&provider=github")
            echo "📊 Connection records:"
            echo "$CONNECTIONS_RESPONSE" | jq '.'
            
            echo ""
            echo "🎯 GitHub OAuth2 Real Complete Flow Test successfully completed!"
            echo "✅ All steps executed:"
            echo "   ✓ Configuration initialization"
            echo "   ✓ Authorization URL generation"
            echo "   ✓ User authorization"
            echo "   ✓ Authorization code exchange"
            echo "   ✓ User information retrieval"
            echo "   ✓ Connection persistence to database"
            
        else
            echo "⚠️  Flow status: $FINAL_STATUS_VALUE"
            echo "📋 Detailed information:"
            echo "$FINAL_STATUS" | jq '.'
        fi
        
    else
        echo "⚠️  Authorization URL not found"
    fi
else
    echo "📊 Execution status: $STATUS"
    echo "📋 Execution details:"
    echo "$STATUS_RESPONSE" | jq '.'
fi

echo ""
echo "🎯 Test completed!"
echo "📋 Workflow ID: $WORKFLOW_ID"
echo "📋 Execution ID: $EXECUTION_ID"
