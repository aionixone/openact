#!/bin/bash

# GitHub OAuth2 Quick Test Script

set -e

echo "🚀 GitHub OAuth2 Quick Test"
echo "=========================="

# Check environment variables
if [ -z "$GITHUB_CLIENT_ID" ]; then
    echo "❌ Error: Please set the GITHUB_CLIENT_ID environment variable"
    echo "💡 How to set: export GITHUB_CLIENT_ID=your_client_id"
    exit 1
fi

if [ -z "$GITHUB_CLIENT_SECRET" ]; then
    echo "❌ Error: Please set the GITHUB_CLIENT_SECRET environment variable"
    echo "💡 How to set: export GITHUB_CLIENT_SECRET=your_client_secret"
    exit 1
fi

echo "✅ Environment variables check passed"
echo "   Client ID: ${GITHUB_CLIENT_ID:0:8}..."

# Check necessary files
if [ ! -f "examples/github_oauth2.yaml" ]; then
    echo "❌ Error: Cannot find examples/github_oauth2.yaml configuration file"
    exit 1
fi

if [ ! -f "examples/github_real_test.rs" ]; then
    echo "❌ Error: Cannot find examples/github_real_test.rs test file"
    exit 1
fi

echo "✅ Configuration files check passed"

# Compile the project
echo "🔨 Compiling the project..."
if ! cargo build --example github_real_test --features callback; then
    echo "❌ Compilation failed"
    exit 1
fi

echo "✅ Compilation successful"

# Run the test
echo ""
echo "🧪 Starting GitHub OAuth2 Real Test..."
echo "📝 Notes:"
echo "   1. The browser will automatically open the GitHub authorization page"
echo "   2. Please log in and authorize the application"
echo "   3. After authorization, the test results will be returned automatically"
echo ""
echo "🚀 Launching the test..."

# Run the real test
cargo run --example github_real_test --features callback

echo ""
echo "🎉 Test completed!"
