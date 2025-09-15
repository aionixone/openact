#!/usr/bin/env bash
set -euo pipefail

BASE_DIR="/Users/sryu/projects/aionixone/openact"
export OPENACT_DATABASE_URL="sqlite:${BASE_DIR}/manifest/data/openact.db"
export OPENACT_MASTER_KEY="your-32-byte-key-here-for-testing"
export RUST_LOG=info

tenant="test"
provider="github"
name="get-user"
action_trn="trn:openact:${tenant}:action/${provider}/${name}@v1"
yaml_path="${BASE_DIR}/providers/github/actions/get-user.openapi.yaml"

echo "=== All Interfaces E2E Test ==="
echo "1. Setup (CLI)"
cargo run -q -p openact-cli -- action-delete "$action_trn" >/dev/null 2>&1 || true
cargo run -q -p openact-cli -- action-register "$tenant" "$provider" "$name" "$action_trn" "$yaml_path" >/dev/null
export GITHUB_TOKEN="dummy"
cargo run -q -p openact-cli -- auth-create-pat "$tenant" "$provider" "user1" >/dev/null
auth_trn="trn:authflow:${tenant}:connection/${provider}-user1"
cargo run -q -p openact-cli -- binding-bind "$tenant" "$auth_trn" "$action_trn" >/dev/null
echo "✓ Setup complete (action, auth, binding)"

echo "2. Test STDIO-RPC"
stdio_exec_trn="trn:exec:${tenant}:stdio:$(python3 -c 'import uuid; print(uuid.uuid4())')"
OPENACT_DATABASE_URL="$OPENACT_DATABASE_URL" OPENACT_MASTER_KEY="$OPENACT_MASTER_KEY" python3 - <<PY
import json, subprocess, time
proc = subprocess.Popen(['cargo', 'run', '-q', '-p', 'openact-stdio'], 
                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=False)
time.sleep(1)

def send_rpc(method, params=None):
    req = {"jsonrpc": "2.0", "method": method, "id": 1}
    if params: req["params"] = params
    line = json.dumps(req) + "\n"
    proc.stdin.write(line.encode())
    proc.stdin.flush()
    resp_line = proc.stdout.readline().decode().strip()
    return json.loads(resp_line)

resp = send_rpc("run", {"tenant": "$tenant", "action_trn": "$action_trn"})
assert resp.get("result", {}).get("ok") == True, resp
print("✓ STDIO run ok")

proc.terminate()
proc.wait()
PY

echo "3. Test HTTP API"
API_ADDR="127.0.0.1:8097"
export OPENACT_HTTP_ADDR="$API_ADDR"
cargo run -q -p openact-server &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null || true' EXIT

# wait for health
for i in $(seq 1 30); do
  if curl -sS "http://${API_ADDR}/api/v1/health" >/dev/null 2>&1; then break; fi
  sleep 0.5
done

http_exec_trn="trn:exec:${tenant}:http:$(python3 -c 'import uuid; print(uuid.uuid4())')"
run_resp=$(curl -sS -X POST "http://${API_ADDR}/api/v1/run" -H 'Content-Type: application/json' -d "{\"tenant\":\"$tenant\",\"action_trn\":\"$action_trn\",\"exec_trn\":\"$http_exec_trn\"}")
echo "$run_resp" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d.get("ok") == True, d'
echo "✓ HTTP run ok"

echo "4. Test CLI"
cli_exec_trn="trn:exec:${tenant}:cli:$(python3 -c 'import uuid; print(uuid.uuid4())')"
cli_output=$(OPENACT_DATABASE_URL="$OPENACT_DATABASE_URL" OPENACT_MASTER_KEY="$OPENACT_MASTER_KEY" RUST_LOG=off cargo run -q -p openact-cli -- --json-only run "$tenant" "$action_trn" "$cli_exec_trn" --output json 2>/dev/null)
echo "$cli_output" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d.get("ok") == True, d'
echo "✓ CLI run ok"

echo "5. Cross-validate executions via HTTP"
for exec_trn in "$http_exec_trn" "$cli_exec_trn"; do
  exec_resp=$(curl -sS "http://${API_ADDR}/api/v1/executions/${exec_trn}")
  echo "$exec_resp" | python3 -c 'import sys,json; d=json.load(sys.stdin); assert d.get("ok") == True, d'
done
echo "✓ All executions verified"

echo "=== ALL INTERFACES E2E OK ==="
