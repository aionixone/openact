#!/bin/bash

# openact API 测试脚本

BASE_URL="http://localhost:8080/api/v1"

echo "🧪 openact API 测试"
echo "==================="

# 健康检查
echo "1. 健康检查..."
curl -s "$BASE_URL/health" | jq '.'
echo ""

# 创建工作流
echo "2. 创建 GitHub OAuth2 工作流..."
WORKFLOW_RESPONSE=$(curl -s -X POST "$BASE_URL/workflows" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "GitHub OAuth2 Test",
    "description": "测试 GitHub OAuth2 认证流程",
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
echo "工作流 ID: $WORKFLOW_ID"
echo ""

# 获取工作流列表
echo "3. 获取工作流列表..."
curl -s "$BASE_URL/workflows" | jq '.'
echo ""

# 获取工作流图结构
echo "4. 获取工作流图结构..."
curl -s "$BASE_URL/workflows/$WORKFLOW_ID/graph" | jq '.'
echo ""

# 验证工作流
echo "5. 验证工作流..."
curl -s -X POST "$BASE_URL/workflows/$WORKFLOW_ID/validate" | jq '.'
echo ""

# 启动执行
echo "6. 启动工作流执行..."
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
echo "执行 ID: $EXECUTION_ID"
echo ""

# 等待一下让执行完成
sleep 2

# 获取执行状态
echo "7. 获取执行状态..."
curl -s "$BASE_URL/executions/$EXECUTION_ID" | jq '.'
echo ""

# 获取执行轨迹
echo "8. 获取执行轨迹..."
curl -s "$BASE_URL/executions/$EXECUTION_ID/trace" | jq '.'
echo ""

# 获取执行列表
echo "9. 获取执行列表..."
curl -s "$BASE_URL/executions" | jq '.'
echo ""

echo "✅ API 测试完成！"
echo ""
echo "💡 提示:"
echo "  - 使用 'cargo run --example workflow_server_demo --features server' 启动服务器"
echo "  - 使用 WebSocket 客户端连接 ws://localhost:8080/api/v1/ws/executions 获取实时更新"
