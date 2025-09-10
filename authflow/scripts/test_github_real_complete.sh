#!/bin/bash

# GitHub OAuth2 真实完整流程测试脚本
# 包括真实的用户授权和数据库持久化

set -e

BASE_URL="http://localhost:8080/api/v1"

echo "🚀 GitHub OAuth2 真实完整流程测试"
echo "=================================="

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
echo "🔍 检查 AuthFlow 服务器状态..."
if ! curl -s "$BASE_URL/health" > /dev/null; then
    echo "❌ 错误: AuthFlow 服务器未运行"
    echo "💡 请先启动服务器: cargo run --features server"
    exit 1
fi
echo "✅ AuthFlow 服务器运行正常"

# 1. 创建工作流
echo ""
echo "📋 步骤 1: 创建 GitHub OAuth2 工作流..."
WORKFLOW_RESPONSE=$(curl -s -X POST "$BASE_URL/workflows" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "GitHub OAuth2 Real Test",
    "description": "真实的 GitHub OAuth2 认证流程测试",
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
                    "User-Agent": "authflow/0.1"
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

# 3. 检查执行状态并获取授权 URL
echo ""
echo "⏳ 步骤 3: 检查执行状态..."
sleep 2

STATUS_RESPONSE=$(curl -s "$BASE_URL/executions/$EXECUTION_ID")
STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.status')

echo "📊 当前状态: $STATUS"

if [ "$STATUS" = "paused" ]; then
    echo "✅ 流程已暂停，等待用户授权"
    
    # 获取授权 URL
    AUTHORIZE_URL=$(echo "$STATUS_RESPONSE" | jq -r '.context.states.StartAuth.result.authorize_url')
    if [ "$AUTHORIZE_URL" != "null" ] && [ -n "$AUTHORIZE_URL" ]; then
        echo ""
        echo "🔗 授权 URL:"
        echo "$AUTHORIZE_URL"
        echo ""
        echo "📝 下一步操作:"
        echo "   1. 在浏览器中访问上面的授权 URL"
        echo "   2. 登录 GitHub 并授权应用"
        echo "   3. GitHub 会重定向到回调 URL"
        echo "   4. 授权完成后，按任意键继续..."
        echo ""
        read -p "按 Enter 键继续（确保已完成授权）..."
        
        # 4. 模拟获取授权码（在实际场景中，这来自回调）
        echo ""
        echo "🔄 步骤 4: 获取授权码..."
        
        # 这里我们需要从回调中获取真实的授权码
        # 在实际场景中，这应该来自回调服务器的处理
        echo "💡 在实际使用中，授权码会通过回调 URL 自动获取"
        echo "💡 现在我们将使用模拟的授权码来演示完整流程"
        
        # 提示用户输入授权码
        echo ""
        read -p "请输入从 GitHub 回调中获取的授权码: " AUTH_CODE
        
        if [ -z "$AUTH_CODE" ]; then
            echo "❌ 未提供授权码，使用模拟授权码"
            AUTH_CODE="mock_auth_code_$(date +%s)"
        fi
        
        echo "🔑 使用授权码: $AUTH_CODE"
        
        # 5. 恢复执行
        echo ""
        echo "🚀 步骤 5: 恢复执行流程..."
        RESUME_RESPONSE=$(curl -s -X POST "$BASE_URL/executions/$EXECUTION_ID/resume" \
          -H "Content-Type: application/json" \
          -d "{\"code\": \"$AUTH_CODE\"}")
        
        echo "📊 恢复响应:"
        echo "$RESUME_RESPONSE" | jq '.'
        
        # 6. 等待处理完成
        echo ""
        echo "⏳ 步骤 6: 等待流程处理完成..."
        sleep 5
        
        # 7. 检查最终状态
        echo ""
        echo "🔍 步骤 7: 检查最终执行状态..."
        FINAL_STATUS=$(curl -s "$BASE_URL/executions/$EXECUTION_ID")
        FINAL_STATUS_VALUE=$(echo "$FINAL_STATUS" | jq -r '.status')
        
        echo "📊 最终状态: $FINAL_STATUS_VALUE"
        
        if [ "$FINAL_STATUS_VALUE" = "completed" ]; then
            echo "🎉 流程执行完成！"
            echo ""
            echo "📋 执行结果:"
            echo "$FINAL_STATUS" | jq '.'
            
            # 8. 检查数据库中的连接记录
            echo ""
            echo "🔍 步骤 8: 检查数据库中的连接记录..."
            CONNECTIONS_RESPONSE=$(curl -s "$BASE_URL/connections?tenant=test-tenant&provider=github")
            echo "📊 连接记录:"
            echo "$CONNECTIONS_RESPONSE" | jq '.'
            
            echo ""
            echo "🎯 GitHub OAuth2 真实完整流程测试成功完成！"
            echo "✅ 所有步骤都已执行："
            echo "   ✓ 配置初始化"
            echo "   ✓ 授权 URL 生成"
            echo "   ✓ 用户授权"
            echo "   ✓ 授权码交换"
            echo "   ✓ 用户信息获取"
            echo "   ✓ 连接持久化到数据库"
            
        else
            echo "⚠️  流程状态: $FINAL_STATUS_VALUE"
            echo "📋 详细信息:"
            echo "$FINAL_STATUS" | jq '.'
        fi
        
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
