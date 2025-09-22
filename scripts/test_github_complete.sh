#!/bin/bash

# GitHub OAuth2 完整流程测试脚本
# 从授权 URL 生成到数据库写入的端到端测试

set -e

BASE_URL="http://localhost:8080/api/v1"

echo "🚀 GitHub OAuth2 完整流程测试"
echo "=============================="

# 检查环境变量
if [ -z "$GITHUB_CLIENT_ID" ]; then
    echo "❌ 错误: 请设置 GITHUB_CLIENT_ID 环境变量"
    echo "💡 设置方法: export GITHUB_CLIENT_ID=your_client_id"
    exit 1
fi

if [ -z "$GITHUB_CLIENT_SECRET" ]; then
    echo "❌ 错误: 请设置 GITHUB_CLIENT_SECRET 环境变量"
    echo "💡 设置方法: export GITHUB_CLIENT_SECRET=your_client_secret"
    exit 1
fi

echo "✅ 环境变量检查通过"
echo "   Client ID: ${GITHUB_CLIENT_ID:0:8}..."

# 检查服务器是否运行
echo ""
echo "🔍 检查服务器状态..."
if ! curl -s "$BASE_URL/health" > /dev/null; then
    echo "❌ 错误: 服务器未运行，请先启动 openact 服务器"
    echo "💡 启动方法: cargo run --bin openact-server"
    exit 1
fi
echo "✅ 服务器运行正常"

# 1. 创建工作流
echo ""
echo "📋 步骤 1: 创建 GitHub OAuth2 工作流..."

# 创建临时的工作流定义文件
TEMP_WORKFLOW="/tmp/github_oauth_workflow_$$.json"
cat > "$TEMP_WORKFLOW" << 'EOF'
{
  "name": "GitHub OAuth2 Complete Test",
  "description": "完整的 GitHub OAuth2 认证流程测试",
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

# 清理临时文件
rm -f "$TEMP_WORKFLOW"

WORKFLOW_ID=$(echo "$WORKFLOW_RESPONSE" | jq -r '.id')
if [ "$WORKFLOW_ID" = "null" ] || [ -z "$WORKFLOW_ID" ]; then
    echo "❌ 创建工作流失败:"
    echo "$WORKFLOW_RESPONSE" | jq '.'
    exit 1
fi

echo "✅ 工作流创建成功: $WORKFLOW_ID"

# 2. 启动执行
echo ""
echo "🚀 步骤 2: 启动 OAuth2 流程执行..."
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
    echo "❌ 启动执行失败:"
    echo "$EXECUTION_RESPONSE" | jq '.'
    exit 1
fi

echo "✅ 执行启动成功: $EXECUTION_ID"

# 3. 检查执行状态
echo ""
echo "⏳ 步骤 3: 检查执行状态..."
sleep 2

STATUS_RESPONSE=$(curl -s "$BASE_URL/executions/$EXECUTION_ID")
STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.status')

echo "📊 当前状态: $STATUS"

if [ "$STATUS" = "pending" ]; then
    echo "✅ 流程已暂停，等待用户授权"
    
    # 获取授权 URL
    AUTHORIZE_URL=$(echo "$STATUS_RESPONSE" | jq -r '.pending_info.authorize_url')
    if [ "$AUTHORIZE_URL" != "null" ] && [ -n "$AUTHORIZE_URL" ]; then
        echo ""
        echo "🔗 授权 URL:"
        echo "$AUTHORIZE_URL"
        echo ""
        echo "📝 下一步操作:"
        echo "   1. 在浏览器中访问上面的授权 URL"
        echo "   2. 登录 GitHub 并授权应用"
        echo "   3. GitHub 会重定向到回调 URL"
        echo "   4. 运行以下命令继续流程:"
        echo "      curl -X POST \"$BASE_URL/executions/$EXECUTION_ID/resume\" \\"
        echo "        -H \"Content-Type: application/json\" \\"
        echo "        -d '{\"code\": \"<从回调URL获取的code>\"}'"
        echo ""
        echo "💡 或者使用模拟回调继续测试:"
        echo "   ./scripts/simulate_callback.sh $EXECUTION_ID"
    else
        echo "⚠️  未找到授权 URL"
    fi
else
    echo "📊 执行状态: $STATUS"
    echo "📋 执行详情:"
    echo "$STATUS_RESPONSE" | jq '.'
fi

echo ""
echo "🎯 测试完成！"
echo "📋 工作流 ID: $WORKFLOW_ID"
echo "📋 执行 ID: $EXECUTION_ID"
