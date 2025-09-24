#!/bin/bash

# GitHub OAuth2 Complete Flow Test Script
# End-to-end test from generating authorization URL to database entry

set -e

BASE_URL="http://localhost:8080/api/v1/authflow"

echo "🚀 GitHub OAuth2 Complete Flow Test"
echo "=============================="

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
echo "🔍 Checking server status..."
if ! curl -s "$BASE_URL/health" > /dev/null; then
    echo "❌ Error: Server is not running, please start the openact server"
    echo "💡 How to start: cargo run --bin openact-server"
    exit 1
fi
echo "✅ Server is running"

# 1. Create workflow
echo ""
echo "📋 Step 1: Create GitHub OAuth2 workflow..."

# Create a temporary workflow definition file
TEMP_WORKFLOW="/tmp/github_oauth_workflow_$$.json"
cat > "$TEMP_WORKFLOW" << 'EOF'
{
  "name": "GitHub OAuth2 Complete Test",
  "description": "Complete GitHub OAuth2 authentication flow test",
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
                "trace": true,
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
                "refresh_token": "{% $refresh_token %}",
                "token_type": "{% $token_type %}",
                "scope": "{% $scope %}"
              },
              "next": "GetUser"
            },
            "GetUser": {
              "type": "task",
              "resource": "http.request",
              "parameters": {
                "url": "https://api.github.com/user",
                "method": "GET",
                "headers": {
                  "Authorization": "{% 'Bearer ' & $access_token %}",
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
                "connection_ref": "{% \"trn:openact:\" & input.tenant & \":auth_connection/github_\" & $user_login %}",
                "access_token": "{% $access_token %}",
                "refresh_token": "{% $refresh_token %}"
              },
              "end": true
            }
          }
        }
      }
    }
  }
}
EOF

WORKFLOW_RESPONSE=$(curl -s -X POST "$BASE_URL/workflows" \
  -H "Content-Type: application/json" \
  -d @"$TEMP_WORKFLOW")

# Clean up temporary file
rm -f "$TEMP_WORKFLOW"

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
    \"workflow_id\": \"$WORKFLOW_ID\",
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

EXECUTION_ID=$(echo "$EXECUTION_RESPONSE" | jq -r '.execution_id')
if [ "$EXECUTION_ID" = "null" ] || [ -z "$EXECUTION_ID" ]; then
    echo "❌ Failed to start execution:"
    echo "$EXECUTION_RESPONSE" | jq '.'
    exit 1
fi

echo "✅ Execution started successfully: $EXECUTION_ID"

# 3. Check execution status
echo ""
echo "⏳ Step 3: Check execution status..."
sleep 2

STATUS_RESPONSE=$(curl -s "$BASE_URL/executions/$EXECUTION_ID")
STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.status')

echo "📊 Current status: $STATUS"

if [ "$STATUS" = "pending" ]; then
    echo "✅ Flow is paused, waiting for user authorization"
    
    # Get authorization URL
    AUTHORIZE_URL=$(echo "$STATUS_RESPONSE" | jq -r '.pending_info.authorize_url')
    if [ "$AUTHORIZE_URL" != "null" ] && [ -n "$AUTHORIZE_URL" ]; then
        echo ""
        echo "🔗 Authorization URL:"
        echo "$AUTHORIZE_URL"
        echo ""
        echo "📝 Next steps:"
        echo "   1. Visit the authorization URL above in your browser"
        echo "   2. Log in to GitHub and authorize the application"
        echo "   3. GitHub will redirect to the callback URL"
        echo "   4. Run the following command to continue the flow:"
        echo "      curl -X POST \"$BASE_URL/executions/$EXECUTION_ID/resume\" \\"
        echo "        -H \"Content-Type: application/json\" \\"
        echo "        -d '{\"code\": \"<code from callback URL>\"}'"
        echo ""
        echo "💡 Or use the simulate callback to continue testing:"
        echo "   ./scripts/simulate_callback.sh $EXECUTION_ID"
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
