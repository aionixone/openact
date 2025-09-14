#!/bin/bash

# OpenAct Action 快速测试脚本
# 用于在已有认证连接的情况下快速测试Action

set -e

# 默认配置
PROVIDER="github"
AUTH_TYPE="oauth2"
ACTIONS=""
VERBOSE=false

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 帮助信息
print_usage() {
    cat << EOF
用法: $0 [选项]

OpenAct Action 快速测试脚本 - 跳过认证，直接测试Action

选项:
  --provider PROVIDER    Provider名称 (默认: github)
  --auth AUTH_TYPE       认证类型 (默认: oauth2)
  --actions ACTIONS      要测试的Actions，逗号分隔 (默认: get-user)
  --verbose, -v          详细输出
  --help, -h             显示此帮助信息

示例:
  $0                                          # 测试默认的github/get-user
  $0 --actions "get-user,list-repos"         # 测试多个Actions
  $0 --provider slack --auth oauth2 -v       # 测试Slack Provider
  $0 --actions "get-user" --verbose          # 详细模式

说明:
  - 此脚本会跳过认证测试，直接测试Action执行
  - 如果有现成的认证连接，会使用真实token
  - 没有认证连接时，会使用Mock数据进行测试
  - 适用于频繁测试Action功能的场景

EOF
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
            --verbose|-v)
                VERBOSE=true
                shift
                ;;
            --help|-h)
                print_usage
                exit 0
                ;;
            *)
                echo -e "${RED}❌ 未知参数: $1${NC}"
                print_usage
                exit 1
                ;;
        esac
    done
    
    # 设置默认Actions
    if [ -z "$ACTIONS" ]; then
        ACTIONS="get-user"
    fi
}

# 主函数
main() {
    parse_arguments "$@"
    
    echo -e "${BLUE}🚀 OpenAct Action 快速测试${NC}"
    echo "=================================="
    echo "Provider: $PROVIDER"
    echo "认证类型: $AUTH_TYPE"
    echo "测试Actions: $ACTIONS"
    echo ""
    
    # 设置必要的环境变量
    if [ "$PROVIDER" = "github" ]; then
        if [ -z "$GITHUB_CLIENT_ID" ] || [ -z "$GITHUB_CLIENT_SECRET" ]; then
            echo -e "${YELLOW}⚠️ 请设置GitHub环境变量:${NC}"
            echo "export GITHUB_CLIENT_ID=\"your_client_id\""
            echo "export GITHUB_CLIENT_SECRET=\"your_client_secret\""
            echo ""
            echo -e "${BLUE}💡 提示: 没有真实token时，测试会使用Mock数据${NC}"
            echo ""
        fi
    fi
    
    # 构建命令参数
    local cmd_args=(
        "--provider" "$PROVIDER"
        "--auth" "$AUTH_TYPE"
        "--skip-auth"
        "--actions" "$ACTIONS"
    )
    
    if [ "$VERBOSE" = true ]; then
        cmd_args+=("--verbose")
    fi
    
    # 获取脚本目录
    local script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local project_root="$(dirname "$script_dir")"
    local test_script="$project_root/providers/tests/test_provider.sh"
    
    if [ ! -f "$test_script" ]; then
        echo -e "${RED}❌ 测试脚本不存在: $test_script${NC}"
        exit 1
    fi
    
    echo -e "${BLUE}🔧 执行测试命令...${NC}"
    echo "Command: $test_script ${cmd_args[*]}"
    echo ""
    
    # 执行测试
    "$test_script" "${cmd_args[@]}"
    
    local exit_code=$?
    
    echo ""
    if [ $exit_code -eq 0 ]; then
        echo -e "${GREEN}🎉 测试完成！${NC}"
    else
        echo -e "${RED}❌ 测试失败，退出码: $exit_code${NC}"
    fi
    
    return $exit_code
}

# 入口点
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
