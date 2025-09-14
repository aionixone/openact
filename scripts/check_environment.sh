#!/bin/bash

# 环境检查脚本
# 验证运行OpenAct脚本所需的所有依赖

echo "🔍 OpenAct 环境检查"
echo "=================="

# 检查必需的命令
commands=("curl" "jq" "python3" "cargo" "sqlite3")
missing_commands=()

for cmd in "${commands[@]}"; do
    if command -v "$cmd" >/dev/null 2>&1; then
        echo "✅ $cmd: $(command -v $cmd)"
    else
        echo "❌ $cmd: 未找到"
        missing_commands+=("$cmd")
    fi
done

# 检查Python模块
echo ""
echo "🐍 Python模块检查:"
if python3 -c "import os,binascii" 2>/dev/null; then
    echo "✅ Python os,binascii 模块可用"
else
    echo "❌ Python os,binascii 模块不可用"
    missing_commands+=("python3-modules")
fi

# 检查Rust工具链
echo ""
echo "🦀 Rust工具链检查:"
if cargo --version >/dev/null 2>&1; then
    echo "✅ Cargo: $(cargo --version)"
    
    # 检查项目编译
    echo "🔧 检查项目编译..."
    if cargo check --workspace --features server,sqlite,encryption >/dev/null 2>&1; then
        echo "✅ 项目编译检查通过"
    else
        echo "❌ 项目编译检查失败"
        echo "💡 请运行: cargo build --workspace --features server,sqlite,encryption"
    fi
else
    echo "❌ Cargo 不可用"
    missing_commands+=("cargo")
fi

# 检查端口可用性
echo ""
echo "🌐 网络检查:"
if lsof -i :8080 -sTCP:LISTEN >/dev/null 2>&1; then
    echo "⚠️  端口8080已被占用"
    echo "💡 请运行: pkill -f 'authflow.*server' 或使用其他端口"
else
    echo "✅ 端口8080可用"
fi

# 检查数据库目录
echo ""
echo "💾 数据库检查:"
db_dir="authflow/data"
if [ -d "$db_dir" ]; then
    echo "✅ 数据库目录存在: $db_dir"
    if [ -w "$db_dir" ]; then
        echo "✅ 数据库目录可写"
    else
        echo "⚠️  数据库目录不可写"
        echo "💡 请运行: chmod 755 $db_dir"
    fi
else
    echo "⚠️  数据库目录不存在: $db_dir"
    echo "💡 将自动创建"
fi

# 检查脚本权限
echo ""
echo "📜 脚本权限检查:"
scripts=("scripts/complete_github_flow.sh" "scripts/quick_github_auth.sh")
for script in "${scripts[@]}"; do
    if [ -x "$script" ]; then
        echo "✅ $script: 可执行"
    elif [ -f "$script" ]; then
        echo "⚠️  $script: 存在但不可执行"
        echo "💡 请运行: chmod +x $script"
    else
        echo "❌ $script: 不存在"
    fi
done

# 总结
echo ""
echo "📊 检查总结:"
if [ ${#missing_commands[@]} -eq 0 ]; then
    echo "🎉 环境检查通过！可以运行OpenAct脚本"
    echo ""
    echo "💡 使用方法:"
    echo "   export GITHUB_CLIENT_ID='your_client_id'"
    echo "   export GITHUB_CLIENT_SECRET='your_client_secret'"
    echo "   ./scripts/complete_github_flow.sh"
else
    echo "❌ 环境检查失败，缺少以下依赖:"
    for cmd in "${missing_commands[@]}"; do
        echo "   - $cmd"
    done
    echo ""
    echo "💡 安装建议:"
    echo "   macOS: brew install curl jq python3 sqlite"
    echo "   Ubuntu: sudo apt install curl jq python3 sqlite3"
    echo "   Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi
