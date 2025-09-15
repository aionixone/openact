#!/usr/bin/env bash
set -euo pipefail

BASE_DIR="/Users/sryu/projects/aionixone/openact"
API_ADDR="127.0.0.1:8098"
export OPENACT_HTTP_ADDR="$API_ADDR"
export OPENACT_DATABASE_URL="sqlite:${BASE_DIR}/manifest/data/openact.db"
export OPENACT_MASTER_KEY="your-32-byte-key-here-for-testing"
export RUST_LOG=info

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
action_trn="trn:openact:${tenant}:action/${provider}/${name}@v1"
yaml_path="${BASE_DIR}/providers/github/actions/get-user.openapi.yaml"
user_id="user1"
auth_trn="trn:authflow:${tenant}:connection/${provider}-${user_id}"

echo "Ensure clean action state"
cargo run -q -p openact-cli -- action-delete "$action_trn" || true

echo "Register action via CLI (OpenAPI)"
cargo run -q -p openact-cli -- action-register "$tenant" "$provider" "$name" "$action_trn" "$yaml_path" | cat

echo "Create PAT via CLI"
export GITHUB_TOKEN="dummy"
cargo run -q -p openact-cli -- auth-create-pat "$tenant" "$provider" "$user_id" | cat

echo "Rebind (ensure idempotent)"
cargo run -q -p openact-cli -- binding-unbind "$tenant" "$auth_trn" "$action_trn" || true
cargo run -q -p openact-cli -- binding-bind "$tenant" "$auth_trn" "$action_trn" | cat

echo "Run via CLI (json envelope)"
exec_trn=$(python3 - <<'PY'
import uuid
print(f"trn:exec:test:cli:{uuid.uuid4()}")
PY
)
run_out=$(OPENACT_DATABASE_URL="$OPENACT_DATABASE_URL" OPENACT_MASTER_KEY="$OPENACT_MASTER_KEY" RUST_LOG=off cargo run -q -p openact-cli -- --json-only run "$tenant" "$action_trn" "$exec_trn" --output json 2>/dev/null)
echo "CLI stdout length: ${#run_out}"
echo "CLI stdout preview (first 200 chars):"; printf '%s\n' "$run_out" | head -c 200; echo

# Parse JSON and validate envelope
tmpjson=$(mktemp)
printf '%s\n' "$run_out" > "$tmpjson"
ok=$(python3 - "$tmpjson" <<'PY'
import sys,json
with open(sys.argv[1]) as f:
  d=json.load(f)
print(d.get("ok"))
PY
)

test "$ok" = "True"

data_exec=$(python3 - "$tmpjson" <<'PY'
import sys,json
with open(sys.argv[1]) as f:
  d=json.load(f)
print((d.get("data") or {}).get("exec_trn",""))
PY
)

test "$data_exec" = "$exec_trn"

echo "Verify CLI JSON envelope"
test "$ok" = "True" && echo "✓ CLI JSON envelope format validated"

echo "CLI E2E OK"

