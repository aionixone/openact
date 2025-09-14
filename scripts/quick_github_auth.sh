#!/bin/bash

# 快速GitHub OAuth认证脚本
# 用法: ./quick_github_auth.sh <client_id> <client_secret>

set -e

if [ $# -ne 2 ]; then
    echo "用法: $0 <github_client_id> <github_client_secret>"
    echo "示例: $0 Ov23lihVkExosE0hR0Bh 9c704ca863eb45c8175d5d6bd9f367b1d17d8afc"
    exit 1
fi

export GITHUB_CLIENT_ID="$1"
export GITHUB_CLIENT_SECRET="$2"

echo "🚀 快速GitHub OAuth认证"
echo "======================"
echo "Client ID: ${GITHUB_CLIENT_ID:0:8}..."

# 设置环境变量
export AUTHFLOW_MASTER_KEY=$(python3 -c "import os,binascii;print(binascii.hexlify(os.urandom(32)).decode())")
export AUTHFLOW_STORE=sqlite
export AUTHFLOW_SQLITE_URL=sqlite:$(pwd)/authflow/data/authflow.db

echo "✅ 环境变量设置完成"

# 启动服务器并执行认证
cd authflow

# 停止现有服务器
pkill -f "authflow.*server" 2>/dev/null || true
sleep 1

# 启动服务器
echo "🔧 启动AuthFlow服务器..."
RUST_LOG=info cargo run --features server,sqlite,encryption &
SERVER_PID=$!

# 等待服务器启动
for i in {1..10}; do
    if curl -s http://localhost:8080/api/v1/health >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

# 创建并执行工作流
echo "🚀 执行OAuth认证..."
WORKFLOW_RESPONSE=$(curl -s -X POST "http://localhost:8080/api/v1/workflows" \
    -H "Content-Type: application/json" \
    -d "{\"name\": \"Quick GitHub Auth\", \"description\": \"Quick auth\", \"dsl\": $(cat templates/providers/github/oauth2.json)}")

WORKFLOW_ID=$(echo "$WORKFLOW_RESPONSE" | jq -r '.id')

EXECUTION_RESPONSE=$(curl -s -X POST "http://localhost:8080/api/v1/executions" \
    -H "Content-Type: application/json" \
    -d "{
        \"workflowId\": \"$WORKFLOW_ID\",
        \"flow\": \"OAuth\",
        \"input\": {\"tenant\": \"demo-tenant\", \"redirectUri\": \"http://localhost:8080/oauth/callback\"},
        \"context\": {\"vars\": {\"secrets\": {\"github_client_id\": \"$GITHUB_CLIENT_ID\", \"github_client_secret\": \"$GITHUB_CLIENT_SECRET\"}}}
    }")

EXECUTION_ID=$(echo "$EXECUTION_RESPONSE" | jq -r '.executionId')

sleep 3
STATUS_RESPONSE=$(curl -s "http://localhost:8080/api/v1/executions/$EXECUTION_ID")
AUTHORIZE_URL=$(echo "$STATUS_RESPONSE" | jq -r '.context.states.StartAuth.result.authorize_url // empty')

echo "🔗 授权URL: $AUTHORIZE_URL"
open "$AUTHORIZE_URL" 2>/dev/null || echo "请手动打开上面的URL"

# 等待完成
echo "⏳ 等待授权完成..."
for i in {1..60}; do
    sleep 2
    STATUS=$(curl -s "http://localhost:8080/api/v1/executions/$EXECUTION_ID" | jq -r '.status')
    if [ "$STATUS" = "completed" ]; then
        FINAL_STATUS=$(curl -s "http://localhost:8080/api/v1/executions/$EXECUTION_ID")
        CONNECTION_TRN=$(echo "$FINAL_STATUS" | jq -r '.context.states.PersistConnection.result.trn')
        echo "🎉 认证完成！"
        echo "🔑 连接TRN: $CONNECTION_TRN"
        echo ""
        echo "💡 现在可以运行Action测试:"
        echo "   cd ../manifest"
        echo "   export CONNECTION_TRN=\"$CONNECTION_TRN\""
        echo "   export GITHUB_BASE_URL=\"https://api.github.com\""
        echo "   cargo test e2e_github_get_user --test e2e_github -- --ignored --nocapture"
        break
    elif [ "$STATUS" = "failed" ]; then
        echo "❌ 认证失败"
        break
    fi
done

# 保持服务器运行
echo ""
echo "🔧 AuthFlow服务器继续运行 (PID: $SERVER_PID)"
echo "💡 使用 'kill $SERVER_PID' 停止服务器"
