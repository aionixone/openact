#!/bin/bash

# GitHub OAuth2 真实授权演示脚本（更新为 CLI 流程）

set -euo pipefail

echo "🚀 GitHub OAuth2 真实授权演示 (CLI)"
echo "===================================="

if ! command -v jq >/dev/null 2>&1; then
  echo "ℹ️ 未检测到 jq，将以纯文本方式展示结果"
fi

if [ -z "${GITHUB_CLIENT_ID:-}" ] || [ -z "${GITHUB_CLIENT_SECRET:-}" ]; then
  echo "❌ 请设置 GITHUB_CLIENT_ID / GITHUB_CLIENT_SECRET 环境变量"
  exit 1
fi

TMPDIR=$(mktemp -d)
DSL="$TMPDIR/github_oauth.yaml"
# 使用占位符，避免 $config 在 shell 中被展开
cat > "$DSL" <<'YAML'
comment: "GitHub OAuth AC (CLI demo)"
startAt: "Auth"
states:
  Auth:
    type: task
    resource: "oauth2.authorize_redirect"
    parameters:
      authorizeUrl: "https://github.com/login/oauth/authorize"
      clientId: "CLIENT_ID"
      redirectUri: "http://localhost:8080/oauth/callback"
      scope: "read:user"
      usePKCE: true
    next: "Await"
  Await:
    type: task
    resource: "oauth2.await_callback"
    next: "Exchange"
  Exchange:
    type: task
    resource: "http.request"
    parameters:
      method: "POST"
      url: "https://github.com/login/oauth/access_token"
      headers:
        Content-Type: "application/x-www-form-urlencoded"
        Accept: "application/json"
      body:
        grant_type: "authorization_code"
        client_id: "CLIENT_ID"
        client_secret: "CLIENT_SECRET"
        redirect_uri: "http://localhost:8080/oauth/callback"
        code: "{% vars.cb.code %}"
        code_verifier: "{% vars.cb.code_verifier ? vars.cb.code_verifier : '' %}"
    end: true
YAML
# 注入实际的 client_id/secret
sed -i '' -e "s/CLIENT_ID/${GITHUB_CLIENT_ID}/g" -e "s/CLIENT_SECRET/${GITHUB_CLIENT_SECRET}/g" "$DSL"

python3 scripts/callback_server.py >/dev/null 2>&1 &
CB_PID=$!
trap 'kill $CB_PID 2>/dev/null || true' EXIT
sleep 0.3

echo "🟢 回调服务器: http://localhost:8080/oauth/callback (pid=$CB_PID)"
# 使用纯文本输出，便于兼容
OUT=$(RUST_LOG=error cargo run -q --features server --bin openact-cli -- oauth start --dsl "$DSL")
# 兼容 JSON 或纯文本两种输出
if echo "$OUT" | grep -q '^{'; then
  RUN_ID=$(echo "$OUT" | jq -r .run_id)
  AUTH_URL=$(echo "$OUT" | jq -r .authorize_url)
  STATE=$(echo "$OUT" | jq -r .state)
else
  RUN_ID=$(echo "$OUT" | sed -n 's/^run_id: \(.*\)$/\1/p' | head -1)
  AUTH_URL=$(echo "$OUT" | sed -n 's/^authorize_url: \(.*\)$/\1/p' | head -1)
  STATE=$(echo "$OUT" | sed -n 's/^state: \(.*\)$/\1/p' | head -1)
fi
if [ -z "${AUTH_URL:-}" ] || [ -z "${RUN_ID:-}" ] || [ -z "${STATE:-}" ]; then
  echo "❌ 无法解析授权输出:"; echo "$OUT"; exit 1
fi

echo "🔗 授权 URL: $AUTH_URL"
if command -v open >/dev/null 2>&1; then open "$AUTH_URL"; fi

echo "⏳ 等待 GitHub 回调 (最多180s)..."
for i in {1..180}; do
  if [ -f /tmp/github_auth_code.txt ]; then break; fi
  sleep 1
done
if [ ! -f /tmp/github_auth_code.txt ]; then
  echo "❌ 超时未收到回调"
  exit 1
fi
CODE=$(cat /tmp/github_auth_code.txt)
echo "✅ 获取授权码"

echo "➡️  交换 token..."
RES=$(RUST_LOG=error cargo run -q --features server --bin openact-cli -- oauth resume --dsl "$DSL" --run-id "$RUN_ID" --code "$CODE" --state "$STATE")
echo "$RES"

echo "🎉 完成 GitHub OAuth2 授权演示"
