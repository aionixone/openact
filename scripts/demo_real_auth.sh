#!/bin/bash

# GitHub OAuth2 Real Authorization Demo Script (Updated for CLI Process)

set -euo pipefail

echo "🚀 GitHub OAuth2 Real Authorization Demo (CLI)"
echo "===================================="

if ! command -v jq >/dev/null 2>&1; then
  echo "ℹ️ jq not detected, results will be displayed in plain text"
fi

if [ -z "${GITHUB_CLIENT_ID:-}" ] || [ -z "${GITHUB_CLIENT_SECRET:-}" ]; then
  echo "❌ Please set GITHUB_CLIENT_ID / GITHUB_CLIENT_SECRET environment variables"
  exit 1
fi

TMPDIR=$(mktemp -d)
DSL="$TMPDIR/github_oauth.yaml"
# Use placeholders to prevent $config from being expanded in the shell
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
# Inject actual client_id/secret
sed -i '' -e "s/CLIENT_ID/${GITHUB_CLIENT_ID}/g" -e "s/CLIENT_SECRET/${GITHUB_CLIENT_SECRET}/g" "$DSL"

python3 scripts/callback_server.py >/dev/null 2>&1 &
CB_PID=$!
trap 'kill $CB_PID 2>/dev/null || true' EXIT
sleep 0.3

echo "🟢 Callback server: http://localhost:8080/oauth/callback (pid=$CB_PID)"
# Use plain text output for compatibility
OUT=$(RUST_LOG=error cargo run -q --features server --bin openact-cli -- oauth start --dsl "$DSL")
# Compatible with both JSON and plain text output
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
  echo "❌ Unable to parse authorization output:"; echo "$OUT"; exit 1
fi

echo "🔗 Authorization URL: $AUTH_URL"
if command -v open >/dev/null 2>&1; then open "$AUTH_URL"; fi

echo "⏳ Waiting for GitHub callback (up to 180s)..."
for i in {1..180}; do
  if [ -f /tmp/github_auth_code.txt ]; then break; fi
  sleep 1
done
if [ ! -f /tmp/github_auth_code.txt ]; then
  echo "❌ Timeout waiting for callback"
  exit 1
fi
CODE=$(cat /tmp/github_auth_code.txt)
echo "✅ Authorization code received"

echo "➡️  Exchanging token..."
RES=$(RUST_LOG=error cargo run -q --features server --bin openact-cli -- oauth resume --dsl "$DSL" --run-id "$RUN_ID" --code "$CODE" --state "$STATE")
echo "$RES"

echo "🎉 GitHub OAuth2 Authorization Demo Completed"
