#!/bin/bash

# GitHub OAuth2 快速测试脚本

set -e

echo "🚀 GitHub OAuth2 快速测试"
echo "=========================="

# 检查环境变量
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

echo "✅ 环境变量检查通过"
echo "   Client ID: ${GITHUB_CLIENT_ID:0:8}..."

# 检查必要文件
if [ ! -f "examples/github_oauth2.yaml" ]; then
    echo "❌ 错误: 找不到 examples/github_oauth2.yaml 配置文件"
    exit 1
fi

if [ ! -f "examples/github_real_test.rs" ]; then
    echo "❌ 错误: 找不到 examples/github_real_test.rs 测试文件"
    exit 1
fi

echo "✅ 配置文件检查通过"

# 编译项目
echo "🔨 编译项目..."
if ! cargo build --example github_real_test --features callback; then
    echo "❌ 编译失败"
    exit 1
fi

echo "✅ 编译成功"

# 运行测试
echo ""
echo "🧪 开始 GitHub OAuth2 实际测试..."
echo "📝 注意事项:"
echo "   1. 浏览器将自动打开 GitHub 授权页面"
echo "   2. 请登录并授权应用"
echo "   3. 授权后会自动返回测试结果"
echo ""
echo "🚀 启动测试..."

# 运行实际测试
cargo run --example github_real_test --features callback

echo ""
echo "🎉 测试完成!"
