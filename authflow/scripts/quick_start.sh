#!/bin/bash

# AuthFlow 快速入门脚本

echo "🚀 AuthFlow 快速入门"
echo "==================="

# 检查是否在正确的目录
if [ ! -f "Cargo.toml" ] || [ ! -d "examples" ]; then
    echo "❌ 请在 AuthFlow 项目根目录运行此脚本"
    exit 1
fi

echo ""
echo "📋 步骤 1: 基础验证"
echo "运行基础配置验证..."
if cargo run --example simple_github_test; then
    echo "✅ 基础验证通过"
else
    echo "❌ 基础验证失败"
    exit 1
fi

echo ""
echo "📋 步骤 2: 检查环境变量"
if [ -n "$GITHUB_CLIENT_ID" ] && [ -n "$GITHUB_CLIENT_SECRET" ]; then
    echo "✅ GitHub OAuth 环境变量已设置"
    echo "🚀 可以运行完整的 OAuth2 测试:"
    echo "   cargo run --example oauth2_callback_server --features callback"
else
    echo "⚠️  GitHub OAuth 环境变量未设置"
    echo ""
    echo "📝 要进行实际的 GitHub OAuth2 测试，请:"
    echo "   1. 创建 GitHub OAuth App:"
    echo "      https://github.com/settings/developers"
    echo ""
    echo "   2. 设置应用信息:"
    echo "      Application name: AuthFlow Test"
    echo "      Homepage URL: http://localhost:8080"
    echo "      Authorization callback URL: http://localhost:8080/oauth/callback"
    echo ""
    echo "   3. 设置环境变量:"
    echo "      export GITHUB_CLIENT_ID=your_client_id"
    echo "      export GITHUB_CLIENT_SECRET=your_client_secret"
    echo ""
    echo "   4. 运行完整测试:"
    echo "      cargo run --example oauth2_callback_server --features callback"
fi

echo ""
echo "📚 更多信息:"
echo "   - 使用指南: docs/how_to_use.md"
echo "   - GitHub 设置: docs/github_real_setup.md"
echo "   - 配置示例: examples/github_oauth2.yaml"

echo ""
echo "🎉 快速入门完成！"
