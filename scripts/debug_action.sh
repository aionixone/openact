#!/bin/bash

# Action 调试脚本 - 展示详细的输入输出

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${BLUE}🔍 Action 调试模式 - 详细输入输出分析${NC}"
echo "=================================================="

# 设置环境变量
export GITHUB_CLIENT_ID="Ov23lihVkExosE0hR0Bh"
export GITHUB_CLIENT_SECRET="9c704ca863eb45c8175d5d6bd9f367b1d17d8afc"
export AUTHFLOW_MASTER_KEY="test-master-key-32-bytes-long!!"
export AUTHFLOW_SQLITE_URL="sqlite:authflow/data/authflow.db"
export CONNECTION_TRN="trn:authflow:test-tenant:connection/github-mock"
export GITHUB_BASE_URL="https://api.github.com"

echo -e "${CYAN}📋 环境变量设置:${NC}"
echo "GITHUB_CLIENT_ID: ${GITHUB_CLIENT_ID:0:16}..."
echo "GITHUB_CLIENT_SECRET: ${GITHUB_CLIENT_SECRET:0:16}..."
echo "AUTHFLOW_MASTER_KEY: ${AUTHFLOW_MASTER_KEY:0:16}..."
echo "AUTHFLOW_SQLITE_URL: $AUTHFLOW_SQLITE_URL"
echo "CONNECTION_TRN: $CONNECTION_TRN"
echo "GITHUB_BASE_URL: $GITHUB_BASE_URL"
echo ""

echo -e "${CYAN}🏗️ Action构建过程:${NC}"
echo "1. 基础Action信息:"
echo "   - 名称: getGithubUser"
echo "   - 方法: GET"
echo "   - 路径: /user"
echo "   - Provider: github"
echo "   - 租户: tenant1"
echo ""

echo "2. 扩展配置:"
echo "   - timeout_ms: 5000"
echo "   - ok_path: \$status >= 200 and \$status < 300"
echo "   - output_pick: \$body"
echo "   - x-real-http: true"
echo "   - x-base-url: https://api.github.com"
echo ""

echo "3. 认证配置:"
echo "   - connection_trn: $CONNECTION_TRN"
echo "   - scheme: oauth2"
echo "   - injection type: jsonada"
echo "   - injection mapping:"
echo "     headers:"
echo "       Authorization: {% 'Bearer ' & \$access_token %}"
echo "       Accept: {% 'application/vnd.github+json' %}"
echo "       User-Agent: {% 'openact-test/1.0' %}"
echo ""

echo -e "${CYAN}🚀 执行Action测试...${NC}"
echo ""

# 切换到manifest目录并运行测试
cd manifest

# 运行E2E测试并捕获详细输出
echo -e "${YELLOW}运行命令: cargo test e2e_github_get_user -- --ignored --nocapture${NC}"
echo ""

# 执行测试并保存输出
TEST_OUTPUT=$(cargo test e2e_github_get_user -- --ignored --nocapture 2>&1) || true

echo -e "${CYAN}📊 测试输出分析:${NC}"
echo "----------------------------------------"
echo "$TEST_OUTPUT"
echo "----------------------------------------"
echo ""

# 解析和分析输出
if echo "$TEST_OUTPUT" | grep -q "Execution result:"; then
    echo -e "${GREEN}✅ 成功捕获Action执行结果${NC}"
    echo ""
    
    # 提取关键信息
    echo -e "${CYAN}🔍 关键输出字段分析:${NC}"
    
    if echo "$TEST_OUTPUT" | grep -q "Status: Success"; then
        echo -e "${GREEN}✅ 执行状态: Success${NC}"
    elif echo "$TEST_OUTPUT" | grep -q "Status: Failed"; then
        echo -e "${RED}❌ 执行状态: Failed${NC}"
    fi
    
    # 提取响应数据结构
    echo ""
    echo -e "${CYAN}📋 响应数据结构:${NC}"
    if echo "$TEST_OUTPUT" | grep -q "response_data: Some"; then
        echo "✅ 包含响应数据"
        echo "   - method: HTTP方法"
        echo "   - path: 请求路径" 
        echo "   - headers: 注入的HTTP头"
        echo "   - query: 查询参数"
        echo "   - timeout_ms: 超时设置"
        echo "   - retry: 重试配置"
        echo "   - final_status: 最终HTTP状态码"
        echo "   - ok: 执行是否成功"
        echo "   - output: HTTP响应内容"
        echo "   - http.url: 实际请求URL"
        echo "   - http.status: HTTP状态码"
        echo "   - http.body: HTTP响应体"
    else
        echo "❌ 无响应数据"
    fi
    
    echo ""
    echo -e "${CYAN}🔧 Headers注入分析:${NC}"
    if echo "$TEST_OUTPUT" | grep -q "Authorization.*Bearer"; then
        echo "✅ Authorization头已注入"
        if echo "$TEST_OUTPUT" | grep -q "ghp_mock_token"; then
            echo "   - 使用: Mock Token (ghp_mock_token_12345)"
        else
            echo "   - 使用: 真实Token"
        fi
    fi
    
    if echo "$TEST_OUTPUT" | grep -q "User-Agent.*openact-test"; then
        echo "✅ User-Agent头已注入: openact-test/1.0"
    fi
    
    if echo "$TEST_OUTPUT" | grep -q "Accept.*github"; then
        echo "✅ Accept头已注入: application/vnd.github+json"
    fi
    
    echo ""
    echo -e "${CYAN}🌐 HTTP请求分析:${NC}"
    if echo "$TEST_OUTPUT" | grep -q "url.*api.github.com/user"; then
        echo "✅ 请求URL: https://api.github.com/user"
    fi
    
    if echo "$TEST_OUTPUT" | grep -q "status.*40[13]"; then
        echo "⚠️ HTTP状态: 401/403 (认证相关，使用Mock token时正常)"
    elif echo "$TEST_OUTPUT" | grep -q "status.*200"; then
        echo "✅ HTTP状态: 200 (成功)"
    fi
    
else
    echo -e "${RED}❌ 未捕获到Action执行结果${NC}"
fi

echo ""
echo -e "${BLUE}📝 总结:${NC}"
echo "- Action通过E2E测试执行"
echo "- 输入包括: Action配置、认证配置、执行上下文"
echo "- 输出包括: 执行状态、响应数据、HTTP详情、错误信息"
echo "- 认证信息通过jsonada表达式动态注入"
echo "- 支持Mock数据测试和真实API调用"
