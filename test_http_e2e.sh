#!/usr/bin/env bash
set -euo pipefail

BASE_DIR="/Users/sryu/projects/aionixone/openact"
API_ADDR="127.0.0.1:8099"
export OPENACT_HTTP_ADDR="$API_ADDR"
export OPENACT_DATABASE_URL="sqlite:${BASE_DIR}/manifest/data/openact.db"
export OPENACT_MASTER_KEY="your-32-byte-key-here-for-testing"

echo "Starting server at $API_ADDR..."
cargo run -q -p openact-server &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null || true' EXIT

# wait for health
for i in $(seq 1 60); do
  if curl -sS "http://${API_ADDR}/api/v1/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

tenant="test"
provider="github"
name="get-user"
trn="trn:openact:${tenant}:action/${provider}/${name}@v1"

echo "Register action (OpenAPI)"
yaml_path="${BASE_DIR}/providers/github/actions/get-user.openapi.yaml"
yaml_content=$(cat "$yaml_path")

# ensure clean state
curl -sS -X DELETE "http://${API_ADDR}/api/v1/actions/${trn}" | cat || true

curl -sS -X POST "http://${API_ADDR}/api/v1/actions" \
  -H 'Content-Type: application/json' \
  -d @- <<JSON | cat
$(python3 - <<'PY'
import json, os
print(json.dumps({
  "tenant": os.environ.get("tenant","test"),
  "provider": os.environ.get("provider","github"),
  "name": os.environ.get("name","get-user"),
  "trn": os.environ.get("trn","trn:openact:test:action/github/get-user@v1"),
  "yaml": os.environ.get("yaml_content","")
}))
PY
)
JSON

echo "Bind requires an auth, create PAT"
resp=$(curl -sS -X POST "http://${API_ADDR}/api/v1/auth/pat" -H 'Content-Type: application/json' -d '{"tenant":"test","provider":"github","user_id":"user1","token":"dummy"}')
echo "$resp" | cat
auth_trn=$(echo "$resp" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("auth_trn",""))')

echo "Bind auth to action"
curl -sS -X POST "http://${API_ADDR}/api/v1/bindings" -H 'Content-Type: application/json' -d @- <<JSON | cat
{"tenant":"${tenant}","auth_trn":"${auth_trn}","action_trn":"${trn}"}
JSON

echo "Run action"
run=$(
  curl -sS -X POST "http://${API_ADDR}/api/v1/run" \
    -H 'Content-Type: application/json' \
    -d @- <<JSON
{"tenant":"${tenant}","action_trn":"${trn}"}
JSON
)
echo "$run" | cat
exec_trn=$(echo "$run" | python3 -c 'import sys,json; d=json.load(sys.stdin); print((d.get("data") or {}).get("exec_trn",""))')

echo "Get execution"
curl -sS "http://${API_ADDR}/api/v1/executions/${exec_trn}" | cat

echo "HTTP E2E OK"

