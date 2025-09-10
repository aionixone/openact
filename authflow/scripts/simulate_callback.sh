#!/bin/bash

# 模拟 GitHub OAuth2 回调脚本
# 用于测试完整的 OAuth2 流程

set -e

if [ $# -ne 1 ]; then
    echo "用法: $0 <execution_id>"
    echo "示例: $0 exec_123456"
    exit 1
fi

EXECUTION_ID="$1"
BASE_URL="http://localhost:8080/api/v1"

echo "🔄 模拟 GitHub OAuth2 回调"
echo "=========================="
echo "📋 执行 ID: $EXECUTION_ID"

# 模拟授权码（在实际场景中，这来自 GitHub 的回调）
MOCK_CODE="mock_auth_code_$(date +%s)"

echo "🔑 模拟授权码: $MOCK_CODE"

# 恢复执行
echo ""
echo "🚀 恢复执行流程..."
RESUME_RESPONSE=$(curl -s -X POST "$BASE_URL/executions/$EXECUTION_ID/resume" \
  -H "Content-Type: application/json" \
  -d "{
    \"code\": \"$MOCK_CODE\"
  }")

echo "📊 恢复响应:"
echo "$RESUME_RESPONSE" | jq '.'

# 等待处理完成
echo ""
echo "⏳ 等待流程处理完成..."
sleep 3

# 检查最终状态
echo ""
echo "🔍 检查最终执行状态..."
FINAL_STATUS=$(curl -s "$BASE_URL/executions/$EXECUTION_ID")
STATUS=$(echo "$FINAL_STATUS" | jq -r '.status')

echo "📊 最终状态: $STATUS"

if [ "$STATUS" = "completed" ]; then
    echo "✅ 流程执行完成！"
    echo ""
    echo "📋 执行结果:"
    echo "$FINAL_STATUS" | jq '.'
    
    # 检查是否有连接记录
    echo ""
    echo "🔍 检查数据库中的连接记录..."
    CONNECTIONS_RESPONSE=$(curl -s "$BASE_URL/connections?tenant=test-tenant&provider=github")
    echo "📊 连接记录:"
    echo "$CONNECTIONS_RESPONSE" | jq '.'
    
else
    echo "⚠️  流程状态: $STATUS"
    echo "📋 详细信息:"
    echo "$FINAL_STATUS" | jq '.'
fi

echo ""
echo "🎯 模拟回调完成！"
