#!/bin/bash
# OpenAct SDK Generation Script

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "📦 OpenAct SDK Generation Tool"
echo "=============================="

# Check dependencies
if ! command -v openapi-generator-cli &> /dev/null; then
    echo "❌ openapi-generator-cli is not installed"
    echo "Please install it using the following command:"
    echo "  npm install -g @openapitools/openapi-generator-cli"
    exit 1
fi

# Ensure the server is running (or generate static files)
echo "🔧 Generating OpenAPI specification..."
cd "$PROJECT_ROOT"

# Generate OpenAPI JSON using tests
OPENAPI_JSON=$(mktemp)
cargo test openapi_json_generation --features openapi,server -- --nocapture --exact 2>/dev/null | \
    grep -A 1000 "Generated OpenAPI spec" | tail -n +2 > "$OPENAPI_JSON" || {
    echo "❌ Failed to generate OpenAPI specification"
    exit 1
}

echo "✅ OpenAPI specification generated"

# Generate TypeScript SDK
echo "🚀 Generating TypeScript SDK..."
SDK_DIR="$PROJECT_ROOT/sdk/typescript"
mkdir -p "$SDK_DIR"

openapi-generator-cli generate \
    -i "$OPENAPI_JSON" \
    -g typescript-axios \
    -o "$SDK_DIR" \
    --additional-properties=npmName=openact-client,withSeparateModelsAndApi=true,modelPackage=models,apiPackage=api

echo "✅ TypeScript SDK generated at: $SDK_DIR"

# Clean up temporary files
rm "$OPENAPI_JSON"

# Validate the generated SDK
echo "🧪 Validating SDK structure..."
if [ -f "$SDK_DIR/package.json" ] && [ -d "$SDK_DIR/api" ] && [ -d "$SDK_DIR/models" ]; then
    echo "✅ SDK structure validation passed"
    
    # Display the number of generated API files
    API_COUNT=$(find "$SDK_DIR/api" -name "*.ts" | wc -l)
    MODEL_COUNT=$(find "$SDK_DIR/models" -name "*.ts" | wc -l)
    
    echo "📊 Generation statistics:"
    echo "  - API files: $API_COUNT"
    echo "  - Model files: $MODEL_COUNT"
    
    echo ""
    echo "🎉 SDK generation completed!"
    echo "Usage:"
    echo "  cd $SDK_DIR"
    echo "  npm install"
    echo "  npm run build"
else
    echo "❌ SDK structure validation failed"
    exit 1
fi
