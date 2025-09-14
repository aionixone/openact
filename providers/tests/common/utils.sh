#!/bin/bash

# OpenAct Provider 测试 - 通用工具函数库

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# 日志级别
LOG_DEBUG=0
LOG_INFO=1
LOG_WARN=2
LOG_ERROR=3

# 当前日志级别
CURRENT_LOG_LEVEL=${LOG_LEVEL:-$LOG_INFO}

# 打印函数
print_header() {
    local title="$1"
    local width=50
    local padding=$(( (width - ${#title}) / 2 ))
    
    echo ""
    echo -e "${BLUE}$(printf '=%.0s' $(seq 1 $width))${NC}"
    echo -e "${BLUE}$(printf '%*s' $padding)${title}$(printf '%*s' $padding)${NC}"
    echo -e "${BLUE}$(printf '=%.0s' $(seq 1 $width))${NC}"
    echo ""
}

# 日志函数
log_debug() {
    if [ $CURRENT_LOG_LEVEL -le $LOG_DEBUG ]; then
        echo -e "${PURPLE}[DEBUG]${NC} $1" >&2
    fi
}

log_info() {
    if [ $CURRENT_LOG_LEVEL -le $LOG_INFO ]; then
        echo -e "${CYAN}[INFO]${NC} $1"
    fi
}

log_warn() {
    if [ $CURRENT_LOG_LEVEL -le $LOG_WARN ]; then
        echo -e "${YELLOW}[WARN]${NC} $1" >&2
    fi
}

log_error() {
    if [ $CURRENT_LOG_LEVEL -le $LOG_ERROR ]; then
        echo -e "${RED}[ERROR]${NC} $1" >&2
    fi
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_result() {
    local test_name="$1"
    local result="$2"
    
    if [ "$result" = "success" ]; then
        echo -e "${GREEN}✅${NC} $test_name: ${GREEN}通过${NC}"
    else
        echo -e "${RED}❌${NC} $test_name: ${RED}失败${NC}"
    fi
}

# 格式化状态
format_status() {
    local status="$1"
    case "$status" in
        "success")
            echo -e "${GREEN}✅ 成功${NC}"
            ;;
        "failed")
            echo -e "${RED}❌ 失败${NC}"
            ;;
        "pending")
            echo -e "${YELLOW}⏳ 等待${NC}"
            ;;
        "running")
            echo -e "${BLUE}🔄 运行中${NC}"
            ;;
        "skipped")
            echo -e "${CYAN}⏭️ 跳过${NC}"
            ;;
        *)
            echo -e "${PURPLE}❓ 未知${NC}"
            ;;
    esac
}

# 检查必需工具
check_required_tools() {
    local tools=("curl" "jq" "yq" "cargo" "sqlite3")
    local missing_tools=()
    
    for tool in "${tools[@]}"; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            missing_tools+=("$tool")
        fi
    done
    
    if [ ${#missing_tools[@]} -gt 0 ]; then
        log_error "缺少必需工具: ${missing_tools[*]}"
        log_info "请安装缺少的工具:"
        for tool in "${missing_tools[@]}"; do
            case "$tool" in
                "yq")
                    echo "  - yq: brew install yq 或 pip install yq"
                    ;;
                "jq")
                    echo "  - jq: brew install jq 或 apt install jq"
                    ;;
                *)
                    echo "  - $tool"
                    ;;
            esac
        done
        return 1
    fi
    
    return 0
}

# 检查项目结构
check_project_structure() {
    local required_dirs=(
        "$PROJECT_ROOT/authflow"
        "$PROJECT_ROOT/manifest"
        "$PROJECT_ROOT/providers"
    )
    
    for dir in "${required_dirs[@]}"; do
        if [ ! -d "$dir" ]; then
            log_error "项目目录不存在: $dir"
            return 1
        fi
    done
    
    return 0
}

# 检查环境变量
check_environment_variables() {
    local required_vars=()
    
    # 根据认证类型检查不同的环境变量
    case "$AUTH_TYPE" in
        "oauth2")
            case "$PROVIDER" in
                "github")
                    required_vars=("GITHUB_CLIENT_ID" "GITHUB_CLIENT_SECRET")
                    ;;
                "slack")
                    required_vars=("SLACK_CLIENT_ID" "SLACK_CLIENT_SECRET")
                    ;;
            esac
            ;;
        "pat")
            case "$PROVIDER" in
                "github")
                    required_vars=("GITHUB_TOKEN")
                    ;;
            esac
            ;;
        "api_key")
            case "$PROVIDER" in
                "notion")
                    required_vars=("NOTION_API_KEY")
                    ;;
            esac
            ;;
    esac
    
    local missing_vars=()
    for var in "${required_vars[@]}"; do
        if [ -z "${!var}" ]; then
            missing_vars+=("$var")
        fi
    done
    
    if [ ${#missing_vars[@]} -gt 0 ]; then
        log_warn "缺少环境变量: ${missing_vars[*]}"
        log_info "请设置必需的环境变量:"
        for var in "${missing_vars[@]}"; do
            echo "  export $var=\"your_value\""
        done
        log_info "或者在测试过程中会提示输入"
    fi
    
    return 0
}

# 等待服务启动
wait_for_service() {
    local url="$1"
    local timeout="${2:-30}"
    local interval="${3:-1}"
    
    log_info "等待服务启动: $url"
    
    for ((i=1; i<=timeout; i++)); do
        if curl -s "$url" >/dev/null 2>&1; then
            log_success "服务启动成功"
            return 0
        fi
        
        if [ $((i % 10)) -eq 0 ]; then
            log_info "等待中... ($i/$timeout)"
        fi
        
        sleep "$interval"
    done
    
    log_error "服务启动超时: $url"
    return 1
}

# 检查端口是否被占用
check_port() {
    local port="$1"
    
    if lsof -i ":$port" -sTCP:LISTEN >/dev/null 2>&1; then
        log_warn "端口 $port 已被占用"
        return 1
    fi
    
    return 0
}

# 停止端口上的进程
kill_port() {
    local port="$1"
    
    if check_port "$port"; then
        log_info "停止端口 $port 上的进程..."
        lsof -ti ":$port" | xargs kill -9 2>/dev/null || true
        sleep 2
    fi
}

# 生成随机字符串
generate_random_string() {
    local length="${1:-32}"
    python3 -c "import os,binascii;print(binascii.hexlify(os.urandom($length//2)).decode())"
}

# 获取时间戳
get_timestamp() {
    date -u +%Y-%m-%dT%H:%M:%SZ
}

# 计算持续时间
calculate_duration() {
    local start_time="$1"
    local end_time="$2"
    echo $((end_time - start_time))
}

# JSON 安全输出
json_escape() {
    local input="$1"
    echo "$input" | sed 's/\\/\\\\/g; s/"/\\"/g; s/\t/\\t/g; s/\r/\\r/g; s/\n/\\n/g'
}

# 验证JSON格式
validate_json() {
    local json_string="$1"
    echo "$json_string" | jq . >/dev/null 2>&1
}

# 验证YAML格式
validate_yaml() {
    local yaml_file="$1"
    yq eval . "$yaml_file" >/dev/null 2>&1
}

# 创建临时文件
create_temp_file() {
    local prefix="${1:-openact_test}"
    mktemp "/tmp/${prefix}_XXXXXX"
}

# 清理临时文件
cleanup_temp_files() {
    local pattern="${1:-openact_test_*}"
    find /tmp -name "$pattern" -type f -mtime +1 -delete 2>/dev/null || true
}

# 重试执行函数
retry_command() {
    local max_attempts="$1"
    local delay="$2"
    shift 2
    local command=("$@")
    
    local attempt=1
    while [ $attempt -le $max_attempts ]; do
        if "${command[@]}"; then
            return 0
        fi
        
        if [ $attempt -lt $max_attempts ]; then
            log_warn "命令失败，重试 $attempt/$max_attempts，等待 ${delay}s..."
            sleep "$delay"
        fi
        
        ((attempt++))
    done
    
    log_error "命令执行失败，已重试 $max_attempts 次"
    return 1
}

# 显示进度条
show_progress() {
    local current="$1"
    local total="$2"
    local width=50
    
    local percentage=$((current * 100 / total))
    local filled=$((current * width / total))
    local empty=$((width - filled))
    
    printf "\r["
    printf "%*s" $filled | tr ' ' '='
    printf "%*s" $empty | tr ' ' '-'
    printf "] %d%% (%d/%d)" $percentage $current $total
}

# 完成进度条
finish_progress() {
    echo ""
}

# 确认用户输入
confirm() {
    local message="$1"
    local default="${2:-n}"
    
    while true; do
        if [ "$default" = "y" ]; then
            read -p "$message [Y/n]: " yn
            yn=${yn:-y}
        else
            read -p "$message [y/N]: " yn
            yn=${yn:-n}
        fi
        
        case $yn in
            [Yy]* ) return 0;;
            [Nn]* ) return 1;;
            * ) echo "请输入 y 或 n";;
        esac
    done
}

# 安全读取密码
read_password() {
    local prompt="$1"
    local password
    
    echo -n "$prompt"
    read -s password
    echo ""
    echo "$password"
}

# 检查文件是否存在且可读
check_file_readable() {
    local file="$1"
    
    if [ ! -f "$file" ]; then
        log_error "文件不存在: $file"
        return 1
    fi
    
    if [ ! -r "$file" ]; then
        log_error "文件不可读: $file"
        return 1
    fi
    
    return 0
}
