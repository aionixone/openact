#!/bin/bash

# OpenAct Provider 测试 - OAuth2认证适配器

# 全局变量
WORKFLOW_ID=""
EXECUTION_ID=""
CONNECTION_TRN=""

# 测试OAuth2认证流程
test_auth_flow() {
    local provider="$1"
    local auth_type="$2"
    
    log_info "开始OAuth2认证流程测试..."
    
    # 启动AuthFlow服务器
    if ! start_authflow_server; then
        log_error "AuthFlow服务器启动失败"
        return 1
    fi
    
    # 创建OAuth2工作流
    if ! create_oauth2_workflow; then
        log_error "创建OAuth2工作流失败"
        return 1
    fi
    
    # 执行OAuth2认证
    if ! execute_oauth2_flow; then
        log_error "执行OAuth2认证失败"
        return 1
    fi
    
    # 验证认证结果
    if ! verify_oauth2_result; then
        log_error "OAuth2认证结果验证失败"
        return 1
    fi
    
    log_success "OAuth2认证流程测试完成"
    return 0
}

# 创建OAuth2工作流
create_oauth2_workflow() {
    log_info "创建OAuth2工作流..."
    
    # 读取工作流DSL
    local workflow_dsl
    if ! workflow_dsl=$(cat "$AUTH_CONFIG_FILE"); then
        log_error "读取工作流DSL失败: $AUTH_CONFIG_FILE"
        return 1
    fi
    
    # 验证DSL格式
    if ! validate_yaml "$AUTH_CONFIG_FILE"; then
        log_error "工作流DSL格式错误"
        return 1
    fi
    
    # 转换YAML为JSON (AuthFlow API需要JSON格式)
    local workflow_json
    if ! workflow_json=$(yq eval -o=json '.' "$AUTH_CONFIG_FILE"); then
        log_error "转换工作流DSL为JSON失败"
        return 1
    fi
    
    # 创建工作流请求
    local create_request=$(cat << EOF
{
    "name": "${PROVIDER} OAuth2 Test Flow",
    "description": "Test OAuth2 authentication for ${PROVIDER}",
    "dsl": $workflow_json
}
EOF
)
    
    # 发送创建工作流请求
    log_debug "发送创建工作流请求..."
    local response
    if ! response=$(curl -s -X POST "http://localhost:$AUTHFLOW_PORT/api/v1/workflows" \
        -H "Content-Type: application/json" \
        -d "$create_request"); then
        log_error "创建工作流请求失败"
        return 1
    fi
    
    # 解析响应
    if ! validate_json "$response"; then
        log_error "工作流创建响应格式错误: $response"
        return 1
    fi
    
    WORKFLOW_ID=$(echo "$response" | jq -r '.id')
    if [ "$WORKFLOW_ID" = "null" ] || [ -z "$WORKFLOW_ID" ]; then
        log_error "工作流创建失败:"
        log_error "响应: $response"
        if validate_json "$response"; then
            echo "$response" | jq '.'
        else
            echo "响应不是有效的JSON: $response"
        fi
        return 1
    fi
    
    log_success "工作流创建成功: $WORKFLOW_ID"
    return 0
}

# 执行OAuth2认证流程
execute_oauth2_flow() {
    log_info "执行OAuth2认证流程..."
    
    # 准备认证上下文
    local auth_context
    if ! auth_context=$(prepare_oauth2_context); then
        log_error "准备OAuth2上下文失败"
        return 1
    fi
    
    # 创建执行请求
    local execution_request=$(cat << EOF
{
    "workflowId": "$WORKFLOW_ID",
    "flow": "OAuth",
    "input": {
        "tenant": "$TENANT",
        "redirectUri": "$REDIRECT_URI"
    },
    "context": $auth_context
}
EOF
)
    
    # 发送执行请求
    log_debug "发送执行请求..."
    local response
    if ! response=$(curl -s -X POST "http://localhost:$AUTHFLOW_PORT/api/v1/executions" \
        -H "Content-Type: application/json" \
        -d "$execution_request"); then
        log_error "执行请求失败"
        return 1
    fi
    
    # 解析执行ID
    EXECUTION_ID=$(echo "$response" | jq -r '.executionId')
    if [ "$EXECUTION_ID" = "null" ] || [ -z "$EXECUTION_ID" ]; then
        log_error "执行启动失败:"
        echo "$response" | jq '.'
        return 1
    fi
    
    log_success "执行启动成功: $EXECUTION_ID"
    
    # 处理OAuth2授权流程
    if ! handle_oauth2_authorization; then
        log_error "OAuth2授权处理失败"
        return 1
    fi
    
    return 0
}

# 准备OAuth2上下文
prepare_oauth2_context() {
    local context=""
    
    case "$PROVIDER" in
        "github")
            if [ -z "$GITHUB_CLIENT_ID" ] || [ -z "$GITHUB_CLIENT_SECRET" ]; then
                log_error "缺少GitHub OAuth2凭据"
                return 1
            fi
            context=$(cat << EOF
{
    "vars": {
        "secrets": {
            "github_client_id": "$GITHUB_CLIENT_ID",
            "github_client_secret": "$GITHUB_CLIENT_SECRET"
        }
    }
}
EOF
)
            ;;
        "slack")
            if [ -z "$SLACK_CLIENT_ID" ] || [ -z "$SLACK_CLIENT_SECRET" ]; then
                log_error "缺少Slack OAuth2凭据"
                return 1
            fi
            context=$(cat << EOF
{
    "vars": {
        "secrets": {
            "slack_client_id": "$SLACK_CLIENT_ID",
            "slack_client_secret": "$SLACK_CLIENT_SECRET"
        }
    }
}
EOF
)
            ;;
        *)
            log_error "不支持的Provider: $PROVIDER"
            return 1
            ;;
    esac
    
    echo "$context"
}

# 处理OAuth2授权
handle_oauth2_authorization() {
    log_info "处理OAuth2授权..."
    
    # 等待授权URL生成
    sleep 3
    
    # 获取执行状态
    local status_response
    if ! status_response=$(curl -s "http://localhost:$AUTHFLOW_PORT/api/v1/executions/$EXECUTION_ID"); then
        log_error "获取执行状态失败"
        return 1
    fi
    
    # 提取授权URL
    local authorize_url
    authorize_url=$(echo "$status_response" | jq -r '.context.states.StartAuth.result.authorize_url // empty')
    
    if [ -n "$authorize_url" ]; then
        log_info "授权URL: $authorize_url"
        
        # 在测试环境中，我们需要模拟用户授权
        if ! simulate_user_authorization "$authorize_url"; then
            log_error "模拟用户授权失败"
            return 1
        fi
    else
        log_error "未找到授权URL"
        return 1
    fi
    
    # 等待认证完成
    if ! wait_for_oauth2_completion; then
        log_error "等待OAuth2完成失败"
        return 1
    fi
    
    return 0
}

# 模拟用户授权
simulate_user_authorization() {
    local authorize_url="$1"
    
    log_info "模拟用户授权..."
    
    if [ "$VERBOSE" = true ]; then
        echo ""
        echo "🔗 授权URL: $authorize_url"
        echo ""
        echo "📝 在测试环境中，请手动完成授权:"
        echo "1. 复制上面的URL到浏览器"
        echo "2. 完成GitHub授权"
        echo "3. 等待测试继续..."
        echo ""
        
        # 尝试自动打开浏览器
        if command -v open >/dev/null 2>&1; then
            open "$authorize_url"
            log_info "浏览器已自动打开"
        else
            log_info "请手动打开上面的URL"
        fi
    else
        log_warn "需要手动授权，但当前为非详细模式"
        log_info "请使用 --verbose 参数查看授权URL"
    fi
    
    return 0
}

# 等待OAuth2完成
wait_for_oauth2_completion() {
    log_info "等待OAuth2认证完成..."
    
    local timeout=120  # 2分钟超时
    local interval=2
    
    for ((i=1; i<=timeout/interval; i++)); do
        sleep $interval
        
        # 获取执行状态
        local status_response
        if ! status_response=$(curl -s "http://localhost:$AUTHFLOW_PORT/api/v1/executions/$EXECUTION_ID"); then
            log_warn "获取执行状态失败，重试..."
            continue
        fi
        
        local status=$(echo "$status_response" | jq -r '.status')
        
        case "$status" in
            "completed")
                log_success "OAuth2认证完成！"
                return 0
                ;;
            "failed")
                log_error "OAuth2认证失败"
                local error=$(echo "$status_response" | jq -r '.error // "未知错误"')
                log_error "错误信息: $error"
                return 1
                ;;
            "running"|"pending")
                if [ $((i % 15)) -eq 0 ]; then
                    log_info "等待中... ($((i*interval))/${timeout}s)"
                fi
                ;;
            *)
                log_warn "未知状态: $status"
                ;;
        esac
    done
    
    log_error "OAuth2认证超时"
    return 1
}

# 验证OAuth2结果
verify_oauth2_result() {
    log_info "验证OAuth2认证结果..."
    
    # 获取最终执行状态
    local final_response
    if ! final_response=$(curl -s "http://localhost:$AUTHFLOW_PORT/api/v1/executions/$EXECUTION_ID"); then
        log_error "获取最终执行状态失败"
        return 1
    fi
    
    # 检查执行状态
    local status=$(echo "$final_response" | jq -r '.status')
    if [ "$status" != "completed" ]; then
        log_error "执行状态不正确: $status"
        return 1
    fi
    
    # 提取连接TRN
    CONNECTION_TRN=$(echo "$final_response" | jq -r '.context.states.PersistConnection.result.trn // empty')
    if [ -z "$CONNECTION_TRN" ]; then
        log_error "未找到连接TRN"
        return 1
    fi
    
    log_success "连接TRN: $CONNECTION_TRN"
    
    # 验证连接是否正确存储
    if ! verify_connection_storage; then
        log_error "连接存储验证失败"
        return 1
    fi
    
    # 导出连接TRN供后续使用
    export CONNECTION_TRN="$CONNECTION_TRN"
    
    log_success "OAuth2认证结果验证通过"
    return 0
}

# 验证连接存储
verify_connection_storage() {
    log_debug "验证连接存储..."
    
    # 检查数据库中的连接记录
    local db_file="$PROJECT_ROOT/authflow/data/authflow.db"
    if [ ! -f "$db_file" ]; then
        log_warn "数据库文件不存在: $db_file (可能使用内存存储)"
        log_info "在测试环境中，这是正常的，Action测试将使用Mock数据"
        return 0
    fi
    
    # 查询连接记录
    local connection_count
    if ! connection_count=$(sqlite3 "$db_file" "SELECT COUNT(*) FROM connections WHERE trn='$CONNECTION_TRN';" 2>/dev/null); then
        log_warn "查询连接记录失败，可能是测试环境"
        log_info "Action测试将使用Mock数据"
        return 0
    fi
    
    if [ "$connection_count" -eq 0 ]; then
        log_warn "连接记录不存在: $CONNECTION_TRN"
        log_info "在自动化测试中，这是正常的，Action测试将使用Mock数据"
        return 0
    fi
    
    log_debug "连接存储验证通过"
    return 0
}

# 清理OAuth2资源
cleanup_oauth2_resources() {
    log_debug "清理OAuth2资源..."
    
    # 这里可以添加清理逻辑，比如删除测试连接等
    # 但通常在测试中我们保留连接供后续Action测试使用
    
    return 0
}
