#!/bin/bash

# OpenAct Provider 测试 - 配置加载器

# 全局配置变量
PROVIDER_CONFIG_FILE=""
AUTH_CONFIG_FILE=""
PROVIDER_BASE_URL=""
PROVIDER_SUPPORTED_AUTH=()
PROVIDER_DEFAULT_ACTIONS=()

# 加载Provider配置
load_provider_configuration() {
    local provider="$1"
    local auth_type="$2"
    
    log_info "加载Provider配置: $provider"
    
    # 设置配置文件路径
    local provider_dir="$PROJECT_ROOT/providers/$provider"
    PROVIDER_CONFIG_FILE="$provider_dir/provider.yaml"
    AUTH_CONFIG_FILE="$provider_dir/auth/$auth_type.yaml"
    
    # 检查配置文件是否存在
    if ! check_file_readable "$PROVIDER_CONFIG_FILE"; then
        log_error "Provider配置文件不存在: $PROVIDER_CONFIG_FILE"
        return 1
    fi
    
    if ! check_file_readable "$AUTH_CONFIG_FILE"; then
        log_error "认证配置文件不存在: $AUTH_CONFIG_FILE"
        return 1
    fi
    
    # 加载Provider基础配置
    load_provider_basic_config "$provider"
    
    # 验证认证类型支持
    validate_auth_type_support "$auth_type"
    
    # 加载认证配置
    load_auth_configuration "$auth_type"
    
    log_success "配置加载完成"
    return 0
}

# 加载Provider基础配置
load_provider_basic_config() {
    local provider="$1"
    
    log_debug "加载Provider基础配置..."
    
    # 验证YAML格式
    if ! validate_yaml "$PROVIDER_CONFIG_FILE"; then
        log_error "Provider配置文件格式错误: $PROVIDER_CONFIG_FILE"
        return 1
    fi
    
    # 提取配置信息
    PROVIDER_BASE_URL=$(yq eval '.base_url' "$PROVIDER_CONFIG_FILE")
    
    # 读取支持的认证类型
    local auth_types=$(yq eval '.supported_auth[]' "$PROVIDER_CONFIG_FILE")
    PROVIDER_SUPPORTED_AUTH=()
    while IFS= read -r auth; do
        if [ -n "$auth" ]; then
            PROVIDER_SUPPORTED_AUTH+=("$auth")
        fi
    done <<< "$auth_types"
    
    # 读取默认测试Actions
    local actions=$(yq eval '.test_actions[]?' "$PROVIDER_CONFIG_FILE" 2>/dev/null || echo "")
    PROVIDER_DEFAULT_ACTIONS=()
    while IFS= read -r action; do
        if [ -n "$action" ]; then
            PROVIDER_DEFAULT_ACTIONS+=("$action")
        fi
    done <<< "$actions"
    
    # 如果没有配置test_actions，尝试从actions目录读取
    if [ ${#PROVIDER_DEFAULT_ACTIONS[@]} -eq 0 ]; then
        load_actions_from_directory "$provider"
    fi
    
    log_debug "Base URL: $PROVIDER_BASE_URL"
    log_debug "支持的认证: ${PROVIDER_SUPPORTED_AUTH[*]}"
    log_debug "默认Actions: ${PROVIDER_DEFAULT_ACTIONS[*]}"
    
    return 0
}

# 从actions目录加载Actions
load_actions_from_directory() {
    local provider="$1"
    local actions_dir="$PROJECT_ROOT/providers/$provider/actions"
    
    if [ -d "$actions_dir" ]; then
        log_debug "从目录加载Actions: $actions_dir"
        
        for action_file in "$actions_dir"/*.yaml; do
            if [ -f "$action_file" ]; then
                local action_name=$(basename "$action_file" .yaml)
                PROVIDER_DEFAULT_ACTIONS+=("$action_name")
            fi
        done
    fi
}

# 验证认证类型支持
validate_auth_type_support() {
    local auth_type="$1"
    
    log_debug "验证认证类型支持: $auth_type"
    
    local supported=false
    for supported_auth in "${PROVIDER_SUPPORTED_AUTH[@]}"; do
        if [ "$supported_auth" = "$auth_type" ]; then
            supported=true
            break
        fi
    done
    
    if [ "$supported" = false ]; then
        log_error "Provider $PROVIDER 不支持认证类型: $auth_type"
        log_error "支持的认证类型: ${PROVIDER_SUPPORTED_AUTH[*]}"
        return 1
    fi
    
    return 0
}

# 加载认证配置
load_auth_configuration() {
    local auth_type="$1"
    
    log_debug "加载认证配置: $auth_type"
    
    # 验证YAML格式
    if ! validate_yaml "$AUTH_CONFIG_FILE"; then
        log_error "认证配置文件格式错误: $AUTH_CONFIG_FILE"
        return 1
    fi
    
    # 根据认证类型加载特定配置
    case "$auth_type" in
        "oauth2")
            load_oauth2_config
            ;;
        "pat"|"api_key")
            load_token_config "$auth_type"
            ;;
        *)
            log_error "未知的认证类型: $auth_type"
            return 1
            ;;
    esac
    
    return 0
}

# 加载OAuth2配置
load_oauth2_config() {
    log_debug "加载OAuth2配置..."
    
    # 验证OAuth2工作流格式
    local provider_name=$(yq eval '.provider.name' "$AUTH_CONFIG_FILE")
    local provider_type=$(yq eval '.provider.providerType' "$AUTH_CONFIG_FILE")
    
    if [ "$provider_name" != "$PROVIDER" ]; then
        log_warn "配置文件中的Provider名称不匹配: $provider_name != $PROVIDER"
    fi
    
    if [ "$provider_type" != "oauth2" ]; then
        log_error "认证配置类型不匹配: $provider_type != oauth2"
        return 1
    fi
    
    # 验证工作流结构
    local flows=$(yq eval '.provider.flows | keys | .[]' "$AUTH_CONFIG_FILE")
    if [ -z "$flows" ]; then
        log_error "OAuth2配置中没有找到工作流定义"
        return 1
    fi
    
    log_debug "OAuth2工作流: $flows"
    return 0
}

# 加载Token配置
load_token_config() {
    local auth_type="$1"
    
    log_debug "加载Token配置: $auth_type"
    
    # 验证Token工作流格式
    local provider_name=$(yq eval '.provider.name' "$AUTH_CONFIG_FILE")
    local provider_type=$(yq eval '.provider.providerType' "$AUTH_CONFIG_FILE")
    
    if [ "$provider_name" != "$PROVIDER" ]; then
        log_warn "配置文件中的Provider名称不匹配: $provider_name != $PROVIDER"
    fi
    
    if [ "$provider_type" != "$auth_type" ]; then
        log_error "认证配置类型不匹配: $provider_type != $auth_type"
        return 1
    fi
    
    return 0
}

# 加载Action配置
load_action_config() {
    local provider="$1"
    local action_name="$2"
    
    local action_file="$PROJECT_ROOT/providers/$provider/actions/$action_name.yaml"
    
    if ! check_file_readable "$action_file"; then
        log_error "Action配置文件不存在: $action_file"
        return 1
    fi
    
    if ! validate_yaml "$action_file"; then
        log_error "Action配置文件格式错误: $action_file"
        return 1
    fi
    
    log_debug "Action配置加载成功: $action_name"
    return 0
}

# 获取默认Actions
get_default_actions() {
    local provider="$1"
    
    if [ ${#PROVIDER_DEFAULT_ACTIONS[@]} -eq 0 ]; then
        echo "get-user"  # 默认Action
    else
        local actions_str=""
        for action in "${PROVIDER_DEFAULT_ACTIONS[@]}"; do
            if [ -z "$actions_str" ]; then
                actions_str="$action"
            else
                actions_str="$actions_str,$action"
            fi
        done
        echo "$actions_str"
    fi
}

# 验证Provider配置
validate_provider_config() {
    log_debug "验证Provider配置..."
    
    # 检查必需字段
    local required_fields=("name" "base_url" "supported_auth")
    
    for field in "${required_fields[@]}"; do
        local value=$(yq eval ".$field" "$PROVIDER_CONFIG_FILE")
        if [ "$value" = "null" ] || [ -z "$value" ]; then
            log_error "Provider配置缺少必需字段: $field"
            return 1
        fi
    done
    
    # 验证URL格式
    if [[ ! "$PROVIDER_BASE_URL" =~ ^https?:// ]]; then
        log_error "Provider base_url格式错误: $PROVIDER_BASE_URL"
        return 1
    fi
    
    log_debug "Provider配置验证通过"
    return 0
}

# 验证认证配置
validate_auth_config() {
    log_debug "验证认证配置..."
    
    # 检查基本结构
    local provider_section=$(yq eval '.provider' "$AUTH_CONFIG_FILE")
    if [ "$provider_section" = "null" ]; then
        log_error "认证配置缺少provider部分"
        return 1
    fi
    
    # 检查工作流定义
    local flows=$(yq eval '.provider.flows' "$AUTH_CONFIG_FILE")
    if [ "$flows" = "null" ]; then
        log_error "认证配置缺少flows定义"
        return 1
    fi
    
    log_debug "认证配置验证通过"
    return 0
}

# 验证Actions配置
validate_actions_config() {
    log_debug "验证Actions配置..."
    
    local validation_failed=false
    
    # 验证每个Action配置
    IFS=',' read -ra ACTION_LIST <<< "$ACTIONS"
    for action in "${ACTION_LIST[@]}"; do
        action=$(echo "$action" | xargs)  # 去除空格
        
        if ! load_action_config "$PROVIDER" "$action"; then
            validation_failed=true
        fi
    done
    
    if [ "$validation_failed" = true ]; then
        log_error "部分Action配置验证失败"
        return 1
    fi
    
    log_debug "Actions配置验证通过"
    return 0
}

# 获取配置信息
get_config_info() {
    echo ""
    echo "📋 配置信息"
    echo "============"
    echo "Provider: $PROVIDER"
    echo "Base URL: $PROVIDER_BASE_URL"
    echo "认证类型: $AUTH_TYPE"
    echo "支持的认证: ${PROVIDER_SUPPORTED_AUTH[*]}"
    echo "测试Actions: $ACTIONS"
    echo ""
    echo "配置文件:"
    echo "  - Provider: $PROVIDER_CONFIG_FILE"
    echo "  - 认证: $AUTH_CONFIG_FILE"
    echo ""
}

# 导出配置到环境变量
export_config_to_env() {
    export PROVIDER_BASE_URL="$PROVIDER_BASE_URL"
    export PROVIDER_CONFIG_FILE="$PROVIDER_CONFIG_FILE"
    export AUTH_CONFIG_FILE="$AUTH_CONFIG_FILE"
    
    log_debug "配置已导出到环境变量"
}
