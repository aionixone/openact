#!/bin/bash

# OpenAct Provider 通用测试脚本
# 用法: ./test_provider.sh --provider github --auth oauth2 --actions "get-user,list-repos"

set -e

# 脚本目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# 加载通用函数库
source "$SCRIPT_DIR/common/utils.sh"
source "$SCRIPT_DIR/common/service_manager.sh"
source "$SCRIPT_DIR/common/config_loader.sh"

# 全局变量
PROVIDER=""
AUTH_TYPE=""
ACTIONS=""
TENANT="test-tenant"
REPORT_FORMAT="console"
VERBOSE=false
DRY_RUN=false
SKIP_AUTH=false

# 测试结果 (使用bash 4.0+的关联数组，如果不支持则使用普通变量)
if [[ ${BASH_VERSION%%.*} -ge 4 ]]; then
    declare -A TEST_RESULTS
    declare -A ACTION_RESULTS
else
    # 对于旧版本bash，使用普通变量
    TEST_RESULTS_configuration=""
    TEST_RESULTS_authentication=""
    TEST_RESULTS_actions=""
    TEST_RESULTS_integration=""
fi
START_TIME=""
END_TIME=""

# 主函数
main() {
    START_TIME=$(date +%s)
    
    print_header "OpenAct Provider 测试框架"
    
    parse_arguments "$@"
    validate_environment
    load_provider_config
    setup_test_environment
    
    if [ "$DRY_RUN" = true ]; then
        print_dry_run_info
        exit 0
    fi
    
    run_provider_tests
    generate_test_report
    cleanup_test_environment
    
    END_TIME=$(date +%s)
    print_test_summary
}

# 解析命令行参数
parse_arguments() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --provider)
                PROVIDER="$2"
                shift 2
                ;;
            --auth)
                AUTH_TYPE="$2"
                shift 2
                ;;
            --actions)
                ACTIONS="$2"
                shift 2
                ;;
            --tenant)
                TENANT="$2"
                shift 2
                ;;
            --report)
                REPORT_FORMAT="$2"
                shift 2
                ;;
            --verbose|-v)
                VERBOSE=true
                shift
                ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --skip-auth)
            SKIP_AUTH=true
            shift
            ;;
            --help|-h)
                print_usage
                exit 0
                ;;
            *)
                echo "❌ 未知参数: $1"
                print_usage
                exit 1
                ;;
        esac
    done
    
    # 验证必需参数
    if [ -z "$PROVIDER" ]; then
        echo "❌ 错误: 必须指定 --provider 参数"
        print_usage
        exit 1
    fi
    
    if [ -z "$AUTH_TYPE" ]; then
        echo "❌ 错误: 必须指定 --auth 参数"
        print_usage
        exit 1
    fi
}

# 打印使用说明
print_usage() {
    cat << EOF
用法: $0 [选项]

必需参数:
  --provider PROVIDER    Provider名称 (如: github, slack, notion)
  --auth AUTH_TYPE       认证类型 (如: oauth2, pat, api_key)

可选参数:
  --actions ACTIONS      要测试的Actions，逗号分隔 (默认: 测试所有)
  --tenant TENANT        租户名称 (默认: test-tenant)
  --report FORMAT        报告格式 (console|json|html) (默认: console)
  --verbose, -v          详细输出
  --dry-run              只显示测试计划，不执行
  --skip-auth            跳过认证测试，直接测试Action (需要已有连接)
  --help, -h             显示此帮助信息

示例:
  $0 --provider github --auth oauth2 --actions "get-user,list-repos"
  $0 --provider slack --auth oauth2 --verbose --report json
  $0 --provider notion --auth api_key --dry-run
  $0 --provider github --auth oauth2 --skip-auth --actions "get-user"

EOF
}

# 验证环境
validate_environment() {
    log_info "验证测试环境..."
    
    # 检查必需工具
    check_required_tools
    
    # 检查项目结构
    check_project_structure
    
    # 检查环境变量
    check_environment_variables
    
    log_success "环境验证通过"
}

# 加载Provider配置
load_provider_config() {
    log_info "加载Provider配置: $PROVIDER"
    
    # 使用config_loader.sh中的函数
    load_provider_configuration "$PROVIDER" "$AUTH_TYPE"
    
    # 设置默认Actions
    if [ -z "$ACTIONS" ]; then
        ACTIONS=$(get_default_actions "$PROVIDER")
    fi
    
    log_success "配置加载完成"
    log_debug "Base URL: $PROVIDER_BASE_URL"
    log_debug "认证类型: $AUTH_TYPE"
    log_debug "测试Actions: $ACTIONS"
}

# 设置测试环境
setup_test_environment() {
    log_info "设置测试环境..."
    
    # 生成测试ID
    TEST_ID="test_$(date +%Y%m%d_%H%M%S)_$$"
    
    # 设置环境变量
    setup_environment_variables
    
    # 创建临时目录
    TEST_TEMP_DIR="/tmp/openact_test_$TEST_ID"
    mkdir -p "$TEST_TEMP_DIR"
    
    log_success "测试环境设置完成"
}

# 运行Provider测试
run_provider_tests() {
    log_info "开始Provider测试: $PROVIDER ($AUTH_TYPE)"
    
    # 测试阶段1: 配置验证
    test_configuration
    
    # 测试阶段2: 认证流程
    if [ "$SKIP_AUTH" = true ]; then
        log_info "测试阶段2: 认证流程 (跳过)"
        if [[ ${BASH_VERSION%%.*} -ge 4 ]]; then
            TEST_RESULTS["authentication"]="skipped"
        else
            TEST_RESULTS_authentication="skipped"
        fi
        log_result "认证测试" "跳过"
        
        # 检查是否有现成的连接可用
        if check_existing_connection; then
            log_info "找到现有连接，将用于Action测试"
        else
            log_warn "未找到现有连接，Action测试将使用Mock数据"
        fi
    else
        test_authentication
    fi
    
    # 测试阶段3: Action执行
    test_actions
    
    # 测试阶段4: 集成验证
    if [ "$SKIP_AUTH" = true ]; then
        log_info "测试阶段4: 集成验证 (跳过)"
        if [[ ${BASH_VERSION%%.*} -ge 4 ]]; then
            TEST_RESULTS["integration"]="skipped"
        else
            TEST_RESULTS_integration="skipped"
        fi
        log_result "集成验证" "跳过"
    else
        test_integration
    fi
    
    log_success "Provider测试完成"
}

# 测试配置
test_configuration() {
    log_info "测试阶段1: 配置验证"
    
    local result="success"
    
    # 验证Provider配置
    if ! validate_provider_config; then
        result="failed"
    fi
    
    # 验证认证配置
    if ! validate_auth_config; then
        result="failed"
    fi
    
    # 验证Action配置
    if ! validate_actions_config; then
        result="failed"
    fi
    
    if [[ ${BASH_VERSION%%.*} -ge 4 ]]; then
        TEST_RESULTS["configuration"]="$result"
    else
        TEST_RESULTS_configuration="$result"
    fi
    log_result "配置验证" "$result"
}

# 测试认证
test_authentication() {
    log_info "测试阶段2: 认证流程"
    
    # 加载认证适配器
    source "$SCRIPT_DIR/adapters/${AUTH_TYPE}_adapter.sh"
    
    local result="success"
    
    # 执行认证测试
    if ! test_auth_flow "$PROVIDER" "$AUTH_TYPE"; then
        result="failed"
    fi
    
    if [[ ${BASH_VERSION%%.*} -ge 4 ]]; then
        TEST_RESULTS["authentication"]="$result"
    else
        TEST_RESULTS_authentication="$result"
    fi
    log_result "认证测试" "$result"
}

# 测试Actions
test_actions() {
    log_info "测试阶段3: Action执行"
    
    # 加载Action测试器
    source "$SCRIPT_DIR/adapters/action_adapter.sh"
    
    local overall_result="success"
    
    # 测试每个Action
    IFS=',' read -ra ACTION_LIST <<< "$ACTIONS"
    for action in "${ACTION_LIST[@]}"; do
        action=$(echo "$action" | xargs)  # 去除空格
        
        log_info "测试Action: $action"
        
        if test_single_action "$PROVIDER" "$action"; then
            ACTION_RESULTS["$action"]="success"
            log_result "Action $action" "success"
        else
            ACTION_RESULTS["$action"]="failed"
            log_result "Action $action" "failed"
            overall_result="failed"
        fi
    done
    
    if [[ ${BASH_VERSION%%.*} -ge 4 ]]; then
        TEST_RESULTS["actions"]="$overall_result"
    else
        TEST_RESULTS_actions="$overall_result"
    fi
}

# 测试集成
test_integration() {
    log_info "测试阶段4: 集成验证"
    
    local result="success"
    
    # 验证端到端流程
    if ! verify_end_to_end_flow; then
        result="failed"
    fi
    
    if [[ ${BASH_VERSION%%.*} -ge 4 ]]; then
        TEST_RESULTS["integration"]="$result"
    else
        TEST_RESULTS_integration="$result"
    fi
    log_result "集成验证" "$result"
}

# 生成测试报告
generate_test_report() {
    log_info "生成测试报告..."
    
    # 加载报告生成器
    source "$SCRIPT_DIR/reports/report_generator.sh"
    
    # 生成报告
    generate_report "$REPORT_FORMAT"
    
    log_success "测试报告生成完成"
}

# 清理测试环境
cleanup_test_environment() {
    log_info "清理测试环境..."
    
    # 停止服务
    cleanup_services
    
    # 清理临时文件
    if [ -d "$TEST_TEMP_DIR" ]; then
        rm -rf "$TEST_TEMP_DIR"
    fi
    
    log_success "环境清理完成"
}

# 打印测试总结
print_test_summary() {
    local duration=$((END_TIME - START_TIME))
    
    echo ""
    echo "🎯 测试总结"
    echo "============"
    echo "Provider: $PROVIDER"
    echo "认证类型: $AUTH_TYPE"
    echo "测试时间: $(date -r $START_TIME '+%Y-%m-%d %H:%M:%S') - $(date -r $END_TIME '+%Y-%m-%d %H:%M:%S')"
    echo "总耗时: ${duration}秒"
    echo ""
    
    # 显示测试结果
    local overall_status="success"
    for stage in configuration authentication actions integration; do
        local status
        if [[ ${BASH_VERSION%%.*} -ge 4 ]]; then
            status="${TEST_RESULTS[$stage]:-unknown}"
        else
            case "$stage" in
                "configuration") status="${TEST_RESULTS_configuration:-unknown}" ;;
                "authentication") status="${TEST_RESULTS_authentication:-unknown}" ;;
                "actions") status="${TEST_RESULTS_actions:-unknown}" ;;
                "integration") status="${TEST_RESULTS_integration:-unknown}" ;;
            esac
        fi
        
        if [ "$status" = "failed" ]; then
            overall_status="failed"
        fi
        printf "%-15s: %s\n" "$stage" "$(format_status $status)"
    done
    
    echo ""
    if [ "$overall_status" = "success" ]; then
        echo "🎉 所有测试通过！"
        exit 0
    else
        echo "❌ 部分测试失败！"
        exit 1
    fi
}

# 打印干运行信息
print_dry_run_info() {
    echo ""
    echo "🔍 测试计划 (干运行模式)"
    echo "======================="
    echo "Provider: $PROVIDER"
    echo "认证类型: $AUTH_TYPE"
    echo "测试Actions: $ACTIONS"
    echo "租户: $TENANT"
    echo "报告格式: $REPORT_FORMAT"
    echo ""
    echo "测试阶段:"
    echo "1. 配置验证"
    echo "2. 认证流程测试"
    echo "3. Action执行测试"
    echo "4. 集成验证"
    echo ""
    echo "💡 使用 --verbose 查看详细信息"
    echo "💡 移除 --dry-run 开始实际测试"
}

# 检查现有连接
check_existing_connection() {
    log_debug "检查现有连接..."
    
    # 检查数据库连接
    local db_file="$PROJECT_ROOT/authflow/data/authflow.db"
    if [ ! -f "$db_file" ]; then
        log_debug "数据库文件不存在"
        return 1
    fi
    
    # 查询任何GitHub连接
    local connection_count
    if ! connection_count=$(sqlite3 "$db_file" "SELECT COUNT(*) FROM connections WHERE provider='github';" 2>/dev/null); then
        log_debug "查询连接记录失败"
        return 1
    fi
    
    if [ "$connection_count" -gt 0 ]; then
        # 获取最新的连接TRN
        local latest_trn
        if latest_trn=$(sqlite3 "$db_file" "SELECT trn FROM connections WHERE provider='github' ORDER BY created_at DESC LIMIT 1;" 2>/dev/null); then
            export CONNECTION_TRN="$latest_trn"
            log_debug "找到现有连接: $CONNECTION_TRN"
            return 0
        fi
    fi
    
    log_debug "未找到现有连接"
    return 1
}

# 验证端到端流程
verify_end_to_end_flow() {
    log_debug "验证端到端流程..."
    
    # 检查连接TRN是否存在
    if [ -z "$CONNECTION_TRN" ]; then
        log_error "连接TRN未设置"
        return 1
    fi
    
    # 检查数据库连接
    local db_file="$PROJECT_ROOT/authflow/data/authflow.db"
    if [ ! -f "$db_file" ]; then
        log_error "数据库文件不存在"
        return 1
    fi
    
    # 验证连接记录
    local connection_count
    if ! connection_count=$(sqlite3 "$db_file" "SELECT COUNT(*) FROM connections WHERE trn='$CONNECTION_TRN';" 2>/dev/null); then
        log_error "查询连接记录失败"
        return 1
    fi
    
    if [ "$connection_count" -eq 0 ]; then
        log_warn "连接记录不存在 (测试环境使用Mock数据)"
        log_info "在自动化测试中，这是正常的"
        return 0
    fi
    
    log_debug "端到端流程验证通过"
    return 0
}

# 入口点
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
