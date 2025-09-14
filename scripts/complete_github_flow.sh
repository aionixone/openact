#!/bin/bash

# 完整的GitHub OAuth认证到Action调用流程
# 包含：1) AuthFlow OAuth认证 2) Manifest Action执行

set -e

echo "🚀 完整的GitHub OAuth认证到Action调用流程"
echo "=============================================="

# 检查必需的环境变量
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

# 设置环境变量
export AUTHFLOW_MASTER_KEY=$(python3 -c "import os,binascii;print(binascii.hexlify(os.urandom(32)).decode())")
export AUTHFLOW_STORE=sqlite
export AUTHFLOW_SQLITE_URL=sqlite:$(pwd)/authflow/data/authflow.db
export REDIRECT_URI=http://localhost:8080/oauth/callback

echo "✅ 环境变量设置完成"
echo "   GITHUB_CLIENT_ID: ${GITHUB_CLIENT_ID:0:8}..."
echo "   AUTHFLOW_MASTER_KEY: ${AUTHFLOW_MASTER_KEY:0:16}..."
echo "   AUTHFLOW_SQLITE_URL: $AUTHFLOW_SQLITE_URL"

# ============================================
# 第一部分：AuthFlow OAuth认证
# ============================================

echo ""
echo "📋 第一部分：AuthFlow OAuth认证"
echo "================================"

# 启动AuthFlow服务器
echo "🔧 启动AuthFlow服务器..."
cd authflow

# 检查端口是否被占用
if lsof -i :8080 -sTCP:LISTEN >/dev/null 2>&1; then
    echo "⚠️  端口8080已被占用，尝试停止现有进程..."
    pkill -f "authflow.*server" || true
    sleep 2
fi

# 启动服务器
RUST_LOG=info cargo run --features server,sqlite,encryption &
SERVER_PID=$!

# 等待服务器启动
echo "⏳ 等待服务器启动..."
for i in {1..20}; do
    if curl -s http://localhost:8080/api/v1/health >/dev/null 2>&1; then
        echo "✅ AuthFlow服务器启动成功"
        break
    fi
    if [ $i -eq 20 ]; then
        echo "❌ 服务器启动超时"
        kill $SERVER_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

# 创建GitHub OAuth2工作流
echo "📋 创建GitHub OAuth2工作流..."
WORKFLOW_RESPONSE=$(curl -s -X POST "http://localhost:8080/api/v1/workflows" \
    -H "Content-Type: application/json" \
    -d "{\"name\": \"GitHub OAuth2 Complete Flow\", \"description\": \"Complete GitHub OAuth2 authentication\", \"dsl\": $(cat templates/providers/github/oauth2.json)}")

WORKFLOW_ID=$(echo "$WORKFLOW_RESPONSE" | jq -r '.id')
if [ "$WORKFLOW_ID" = "null" ] || [ -z "$WORKFLOW_ID" ]; then
    echo "❌ 创建工作流失败:"
    echo "$WORKFLOW_RESPONSE" | jq '.'
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi

echo "✅ 工作流创建成功: $WORKFLOW_ID"

# 启动OAuth执行
echo "🚀 启动OAuth执行..."
EXECUTION_RESPONSE=$(curl -s -X POST "http://localhost:8080/api/v1/executions" \
    -H "Content-Type: application/json" \
    -d "{
        \"workflowId\": \"$WORKFLOW_ID\",
        \"flow\": \"OAuth\",
        \"input\": {
            \"tenant\": \"demo-tenant\",
            \"redirectUri\": \"$REDIRECT_URI\"
        },
        \"context\": {
            \"vars\": {
                \"secrets\": {
                    \"github_client_id\": \"$GITHUB_CLIENT_ID\",
                    \"github_client_secret\": \"$GITHUB_CLIENT_SECRET\"
                }
            }
        }
    }")

EXECUTION_ID=$(echo "$EXECUTION_RESPONSE" | jq -r '.executionId')
if [ "$EXECUTION_ID" = "null" ] || [ -z "$EXECUTION_ID" ]; then
    echo "❌ 启动执行失败:"
    echo "$EXECUTION_RESPONSE" | jq '.'
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi

echo "✅ 执行启动成功: $EXECUTION_ID"

# 获取授权URL并打开浏览器
echo "⏳ 获取授权URL..."
sleep 3

STATUS_RESPONSE=$(curl -s "http://localhost:8080/api/v1/executions/$EXECUTION_ID")
AUTHORIZE_URL=$(echo "$STATUS_RESPONSE" | jq -r '.context.states.StartAuth.result.authorize_url // empty')

if [ -n "$AUTHORIZE_URL" ]; then
    echo "🔗 授权URL: $AUTHORIZE_URL"
    echo ""
    echo "📝 请在浏览器中完成GitHub授权..."
    
    # 尝试自动打开浏览器
    if command -v open >/dev/null 2>&1; then
        open "$AUTHORIZE_URL"
        echo "✅ 浏览器已自动打开"
    else
        echo "💡 请手动复制上面的URL到浏览器中打开"
    fi
    
    # 监控执行状态
    echo "⏳ 等待授权完成..."
    for i in {1..120}; do
        sleep 2
        STATUS=$(curl -s "http://localhost:8080/api/v1/executions/$EXECUTION_ID" | jq -r '.status')
        
        if [ "$STATUS" = "completed" ]; then
            echo "🎉 OAuth认证完成！"
            break
        elif [ "$STATUS" = "failed" ]; then
            echo "❌ OAuth认证失败"
            curl -s "http://localhost:8080/api/v1/executions/$EXECUTION_ID" | jq '.error'
            kill $SERVER_PID 2>/dev/null || true
            exit 1
        fi
        
        if [ $i -eq 120 ]; then
            echo "❌ 授权超时"
            kill $SERVER_PID 2>/dev/null || true
            exit 1
        fi
        
        # 每30秒显示一次状态
        if [ $((i % 15)) -eq 0 ]; then
            echo "   状态: $STATUS (等待中...)"
        fi
    done
    
    # 获取最终结果和连接TRN
    FINAL_STATUS=$(curl -s "http://localhost:8080/api/v1/executions/$EXECUTION_ID")
    CONNECTION_TRN=$(echo "$FINAL_STATUS" | jq -r '.context.states.PersistConnection.result.trn')
    
    echo "✅ 连接TRN: $CONNECTION_TRN"
    
else
    echo "❌ 未找到授权URL"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi

# ============================================
# 第二部分：Manifest Action执行
# ============================================

echo ""
echo "📋 第二部分：Manifest Action执行"
echo "==============================="

cd ../manifest

# 设置Manifest环境变量
export CONNECTION_TRN="$CONNECTION_TRN"
export GITHUB_BASE_URL="https://api.github.com"
export AUTHFLOW_MASTER_KEY="$AUTHFLOW_MASTER_KEY"
export AUTHFLOW_SQLITE_URL="$AUTHFLOW_SQLITE_URL"

echo "✅ Manifest环境变量设置完成"
echo "   CONNECTION_TRN: $CONNECTION_TRN"
echo "   GITHUB_BASE_URL: $GITHUB_BASE_URL"
echo "   AUTHFLOW_MASTER_KEY: ${AUTHFLOW_MASTER_KEY:0:16}..."
echo "   AUTHFLOW_SQLITE_URL: $AUTHFLOW_SQLITE_URL"

# 运行E2E测试（实际的Action调用）
echo "🚀 执行GitHub Get User Action..."
echo "⏳ 这将使用真实的GitHub API调用..."

# 运行E2E测试
if cargo test e2e_github_get_user --test e2e_github -- --ignored --nocapture; then
    echo "🎉 Action执行成功！"
    echo ""
    echo "✅ 完整流程验证："
    echo "   ✓ GitHub OAuth2认证完成"
    echo "   ✓ 访问令牌安全存储到数据库"
    echo "   ✓ Manifest成功读取认证信息"
    echo "   ✓ HTTP请求成功注入Authorization头"
    echo "   ✓ GitHub API调用成功执行"
    echo "   ✓ 响应数据正确处理和返回"
else
    echo "❌ Action执行失败"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi

# ============================================
# 清理工作
# ============================================

echo ""
echo "🧹 清理工作..."

# 停止AuthFlow服务器
if kill $SERVER_PID 2>/dev/null; then
    echo "✅ AuthFlow服务器已停止"
else
    echo "⚠️  AuthFlow服务器可能已经停止"
fi

echo ""
echo "🎯 完整流程执行成功！"
echo "========================"
echo ""
echo "📊 流程总结："
echo "1. ✅ 生成并设置加密主密钥"
echo "2. ✅ 启动AuthFlow服务器"
echo "3. ✅ 创建GitHub OAuth2工作流"
echo "4. ✅ 执行OAuth认证流程"
echo "5. ✅ 用户浏览器授权完成"
echo "6. ✅ 访问令牌加密存储到SQLite"
echo "7. ✅ Manifest读取并解密认证信息"
echo "8. ✅ 执行真实的GitHub API调用"
echo "9. ✅ 验证端到端集成成功"
echo ""
echo "🔐 认证信息已安全存储在: $AUTHFLOW_SQLITE_URL"
echo "🔑 连接TRN: $CONNECTION_TRN"
echo ""
echo "💡 提示: 你现在可以使用这个TRN在其他Action中调用GitHub API"
