#!/bin/bash

# OpenAct Provider 测试 - 服务管理器

# 全局变量
AUTHFLOW_PID=""
MANIFEST_PID=""
AUTHFLOW_PORT=8080
MANIFEST_PORT=8081

# 设置环境变量
setup_environment_variables() {
    log_info "设置环境变量..."
    
    # 设置统一的OpenAct环境变量
    if [ -z "$OPENACT_MASTER_KEY" ]; then
        export OPENACT_MASTER_KEY="test-master-key-32-bytes-long!!"
    fi
    
    if [ -z "$OPENACT_DATABASE_URL" ]; then
        export OPENACT_DATABASE_URL="sqlite:/Users/sryu/projects/aionixone/openact/manifest/data/openact.db"
    fi
    
    # 兼容旧的AuthFlow环境变量
    export AUTHFLOW_MASTER_KEY="$OPENACT_MASTER_KEY"
    export AUTHFLOW_SQLITE_URL="$OPENACT_DATABASE_URL"
    export AUTHFLOW_STORE=sqlite
    export REDIRECT_URI="http://localhost:$AUTHFLOW_PORT/oauth/callback"
    
    # 设置Provider特定环境变量
    setup_provider_environment_variables
    
    log_debug "OPENACT_MASTER_KEY: ${OPENACT_MASTER_KEY:0:16}..."
    log_debug "OPENACT_DATABASE_URL: $OPENACT_DATABASE_URL"
    log_debug "AUTHFLOW_MASTER_KEY: ${AUTHFLOW_MASTER_KEY:0:16}... (兼容)"
    log_debug "AUTHFLOW_SQLITE_URL: $AUTHFLOW_SQLITE_URL (兼容)"
    log_debug "REDIRECT_URI: $REDIRECT_URI"
}

# 设置Provider特定环境变量
setup_provider_environment_variables() {
    case "$PROVIDER" in
        "github")
            if [ -n "$GITHUB_CLIENT_ID" ]; then
                export GITHUB_CLIENT_ID="$GITHUB_CLIENT_ID"
            fi
            if [ -n "$GITHUB_CLIENT_SECRET" ]; then
                export GITHUB_CLIENT_SECRET="$GITHUB_CLIENT_SECRET"
            fi
            if [ -n "$GITHUB_TOKEN" ]; then
                export GITHUB_TOKEN="$GITHUB_TOKEN"
            fi
            ;;
        "slack")
            if [ -n "$SLACK_CLIENT_ID" ]; then
                export SLACK_CLIENT_ID="$SLACK_CLIENT_ID"
            fi
            if [ -n "$SLACK_CLIENT_SECRET" ]; then
                export SLACK_CLIENT_SECRET="$SLACK_CLIENT_SECRET"
            fi
            if [ -n "$SLACK_BOT_TOKEN" ]; then
                export SLACK_BOT_TOKEN="$SLACK_BOT_TOKEN"
            fi
            ;;
        "notion")
            if [ -n "$NOTION_API_KEY" ]; then
                export NOTION_API_KEY="$NOTION_API_KEY"
            fi
            ;;
    esac
}

# 启动AuthFlow服务器
start_authflow_server() {
    log_info "启动AuthFlow服务器..."
    
    # 检查端口
    if ! check_port $AUTHFLOW_PORT; then
        log_warn "端口 $AUTHFLOW_PORT 被占用，尝试停止现有进程..."
        kill_port $AUTHFLOW_PORT
    fi
    
    # 切换到authflow目录
    cd "$PROJECT_ROOT/authflow"
    
    # 启动服务器
    log_debug "执行: RUST_LOG=info cargo run --features server"
    RUST_LOG=info cargo run --features server >/dev/null 2>&1 &
    AUTHFLOW_PID=$!
    
    # 等待服务器启动
    if wait_for_service "http://localhost:$AUTHFLOW_PORT/api/v1/health" 30; then
        log_success "AuthFlow服务器启动成功 (PID: $AUTHFLOW_PID)"
        return 0
    else
        log_error "AuthFlow服务器启动失败"
        stop_authflow_server
        return 1
    fi
}

# 停止AuthFlow服务器
stop_authflow_server() {
    if [ -n "$AUTHFLOW_PID" ]; then
        log_info "停止AuthFlow服务器 (PID: $AUTHFLOW_PID)..."
        kill "$AUTHFLOW_PID" 2>/dev/null || true
        wait "$AUTHFLOW_PID" 2>/dev/null || true
        AUTHFLOW_PID=""
    fi
    
    # 确保端口被释放
    kill_port $AUTHFLOW_PORT
}

# 启动Manifest服务
start_manifest_service() {
    log_info "准备Manifest服务环境..."
    
    # 切换到manifest目录
    cd "$PROJECT_ROOT/manifest"
    
    # 设置Manifest环境变量  
    export CONNECTION_TRN="$CONNECTION_TRN"
    export PROVIDER_BASE_URL="$PROVIDER_BASE_URL"
    export OPENACT_MASTER_KEY="$OPENACT_MASTER_KEY"
    export OPENACT_DATABASE_URL="$OPENACT_DATABASE_URL"
    # 兼容变量
    export AUTHFLOW_MASTER_KEY="$AUTHFLOW_MASTER_KEY"  
    export AUTHFLOW_SQLITE_URL="$AUTHFLOW_SQLITE_URL"
    
    log_success "Manifest服务环境准备完成"
    log_debug "CONNECTION_TRN: $CONNECTION_TRN"
    log_debug "PROVIDER_BASE_URL: $PROVIDER_BASE_URL"
}

# 停止Manifest服务
stop_manifest_service() {
    if [ -n "$MANIFEST_PID" ]; then
        log_info "停止Manifest服务 (PID: $MANIFEST_PID)..."
        kill "$MANIFEST_PID" 2>/dev/null || true
        wait "$MANIFEST_PID" 2>/dev/null || true
        MANIFEST_PID=""
    fi
}

# 检查AuthFlow服务健康状态
check_authflow_health() {
    local health_url="http://localhost:$AUTHFLOW_PORT/api/v1/health"
    
    if curl -s "$health_url" >/dev/null 2>&1; then
        local response=$(curl -s "$health_url")
        log_debug "AuthFlow健康检查: $response"
        return 0
    else
        log_error "AuthFlow服务不健康"
        return 1
    fi
}

# 检查Manifest服务健康状态
check_manifest_health() {
    # Manifest目前没有健康检查端点，检查进程是否存在
    if [ -n "$MANIFEST_PID" ] && kill -0 "$MANIFEST_PID" 2>/dev/null; then
        return 0
    else
        return 1
    fi
}

# 启动所有服务
start_services() {
    log_info "启动所有服务..."
    
    # 启动AuthFlow
    if ! start_authflow_server; then
        log_error "AuthFlow服务启动失败"
        return 1
    fi
    
    # 启动Manifest (如果需要)
    start_manifest_service
    
    log_success "所有服务启动完成"
    return 0
}

# 停止所有服务
stop_services() {
    log_info "停止所有服务..."
    
    stop_manifest_service
    stop_authflow_server
    
    log_success "所有服务已停止"
}

# 重启服务
restart_services() {
    log_info "重启服务..."
    
    stop_services
    sleep 2
    start_services
}

# 清理服务
cleanup_services() {
    log_info "清理服务..."
    
    # 停止所有服务
    stop_services
    
    # 清理端口
    kill_port $AUTHFLOW_PORT
    kill_port $MANIFEST_PORT
    
    # 清理进程
    pkill -f "authflow.*server" 2>/dev/null || true
    pkill -f "manifest.*server" 2>/dev/null || true
    
    log_success "服务清理完成"
}

# 获取服务状态
get_services_status() {
    local authflow_status="stopped"
    local manifest_status="stopped"
    
    if check_authflow_health; then
        authflow_status="running"
    fi
    
    if check_manifest_health; then
        manifest_status="running"
    fi
    
    echo "AuthFlow: $authflow_status, Manifest: $manifest_status"
}

# 等待所有服务就绪
wait_for_services() {
    local timeout="${1:-60}"
    
    log_info "等待所有服务就绪..."
    
    local start_time=$(date +%s)
    while true; do
        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))
        
        if [ $elapsed -ge $timeout ]; then
            log_error "等待服务超时 (${timeout}s)"
            return 1
        fi
        
        if check_authflow_health; then
            log_success "所有服务就绪"
            return 0
        fi
        
        sleep 2
    done
}

# 显示服务信息
show_services_info() {
    echo ""
    echo "📋 服务信息"
    echo "============"
    echo "AuthFlow:"
    echo "  - URL: http://localhost:$AUTHFLOW_PORT"
    echo "  - Health: http://localhost:$AUTHFLOW_PORT/api/v1/health"
    echo "  - PID: ${AUTHFLOW_PID:-未运行}"
    echo ""
    echo "Manifest:"
    echo "  - 环境: 已配置"
    echo "  - PID: ${MANIFEST_PID:-未运行}"
    echo ""
    echo "状态: $(get_services_status)"
    echo ""
}
