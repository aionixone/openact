#!/bin/bash
# OpenAct SDK 生成脚本

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "📦 OpenAct SDK 生成工具"
echo "========================"

# 检查依赖
if ! command -v openapi-generator-cli &> /dev/null; then
    echo "❌ openapi-generator-cli 未安装"
    echo "请使用以下命令安装:"
    echo "  npm install -g @openapitools/openapi-generator-cli"
    exit 1
fi

# 确保服务器正在运行 (或生成静态文件)
echo "🔧 生成 OpenAPI 规范..."
cd "$PROJECT_ROOT"

# 使用测试生成 OpenAPI JSON
OPENAPI_JSON=$(mktemp)
cargo test openapi_json_generation --features openapi,server -- --nocapture --exact 2>/dev/null | \
    grep -A 1000 "Generated OpenAPI spec" | tail -n +2 > "$OPENAPI_JSON" || {
    echo "❌ 无法生成 OpenAPI 规范"
    exit 1
}

echo "✅ OpenAPI 规范已生成"

# 生成 TypeScript SDK
echo "🚀 生成 TypeScript SDK..."
SDK_DIR="$PROJECT_ROOT/sdk/typescript"
mkdir -p "$SDK_DIR"

openapi-generator-cli generate \
    -i "$OPENAPI_JSON" \
    -g typescript-axios \
    -o "$SDK_DIR" \
    --additional-properties=npmName=openact-client,withSeparateModelsAndApi=true,modelPackage=models,apiPackage=api

echo "✅ TypeScript SDK 已生成到: $SDK_DIR"

# 清理临时文件
rm "$OPENAPI_JSON"

# 验证生成的 SDK
echo "🧪 验证 SDK 结构..."
if [ -f "$SDK_DIR/package.json" ] && [ -d "$SDK_DIR/api" ] && [ -d "$SDK_DIR/models" ]; then
    echo "✅ SDK 结构验证通过"
    
    # 显示生成的 API 数量
    API_COUNT=$(find "$SDK_DIR/api" -name "*.ts" | wc -l)
    MODEL_COUNT=$(find "$SDK_DIR/models" -name "*.ts" | wc -l)
    
    echo "📊 生成统计:"
    echo "  - API 文件: $API_COUNT"
    echo "  - Model 文件: $MODEL_COUNT"
    
    echo ""
    echo "🎉 SDK 生成完成！"
    echo "使用方法:"
    echo "  cd $SDK_DIR"
    echo "  npm install"
    echo "  npm run build"
else
    echo "❌ SDK 结构验证失败"
    exit 1
fi
