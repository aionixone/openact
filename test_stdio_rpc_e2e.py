#!/usr/bin/env python3
"""
STDIO-RPC end-to-end test: register → auth.pat → bind → run → execution.get/list
"""

import json
import os
import shutil
import subprocess
import sys
import time


def send_rpc_request(process, method, params=None, request_id=1):
    req = {"jsonrpc": "2.0", "method": method, "id": request_id}
    if params is not None:
        req["params"] = params
    line = json.dumps(req) + "\n"
    process.stdin.write(line.encode())
    process.stdin.flush()
    resp_line = process.stdout.readline().decode().strip()
    try:
        return json.loads(resp_line)
    except Exception as e:
        print(f"Failed to parse response for {method}: {resp_line} ({e})")
        return None


def main():
    # Environment
    env = os.environ.copy()
    env.update({
        # Use unified DB path under manifest
        "OPENACT_DATABASE_URL": "sqlite:/Users/sryu/projects/aionixone/openact/manifest/data/openact.db",
        # Dev-only key
        "OPENACT_MASTER_KEY": "your-32-byte-key-here-for-testing",
        "RUST_LOG": "info",
    })

    # Choose command: cargo run or direct binary
    if shutil.which("cargo"):
        cmd = ["cargo", "run", "-q", "-p", "openact-stdio"]
    else:
        binary_path = os.path.abspath(os.path.join(os.path.dirname(__file__), "target", "debug", "openact-stdio"))
        if not os.path.exists(binary_path):
            print(f"Binary not found at {binary_path}. Please build it first (cargo build -p openact-stdio).")
            sys.exit(1)
        cmd = [binary_path]

    print("Starting openact-stdio...", flush=True)
    process = subprocess.Popen(
        cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        text=False,
    )

    try:
        time.sleep(1.5)

        # 1) Health
        r = send_rpc_request(process, "health")
        assert r and "result" in r, f"health failed: {r}"

        tenant = "test"
        provider = "github"
        action_yaml = "./providers/github/actions/get-user.openapi.yaml"
        action_name = "get-user"

        # 2) Ensure a clean action state (delete if exists)
        expected_action_trn = f"trn:openact:{tenant}:action/{provider}/{action_name}@v1"
        send_rpc_request(process, "action.delete", {"trn": expected_action_trn})

        # 3) Register action
        r = send_rpc_request(process, "action.register", {
            "config_path": action_yaml,
            "tenant": tenant,
            "provider": provider,
            "name": action_name,
        })
        expected_action_trn = f"trn:openact:{tenant}:action/{provider}/{action_name}@v1"
        if r and "result" in r:
            action_trn = r["result"]["action_trn"]
        else:
            # Allow existing action
            msg = (r or {}).get("error", {}).get("message", "")
            if "UNIQUE constraint failed: actions.trn" in msg or "already exists" in msg or "Invalid input" in msg:
                action_trn = expected_action_trn
            else:
                raise AssertionError(f"action.register failed: {r}")

        # Ensure OpenAPI content is stored (update regardless)
        ru = send_rpc_request(process, "action.update", {
            "trn": action_trn,
            "config_path": action_yaml,
        })
        assert ru and ("result" in ru or "error" not in ru), f"action.update failed: {ru}"

        # 4) Create PAT connection
        r = send_rpc_request(process, "auth.pat", {
            "tenant": tenant,
            "provider": provider,
            "user_id": "user1",
            "access_token": "dummy-token-for-test"
        })
        if r and "result" in r:
            auth_trn = r["result"]["connection_trn"]
        else:
            # Maybe already exists: list and pick a matching connection
            rl = send_rpc_request(process, "auth.list")
            assert rl and "result" in rl, f"auth.list failed: {rl}"
            conns = rl["result"].get("connections", [])
            # pick one containing tenant and provider
            candidates = [c for c in conns if isinstance(c, str) and tenant in c and provider in c]
            assert candidates, f"no existing auth connection found for tenant={tenant}, provider={provider}: {conns}"
            auth_trn = candidates[0]

        # 5) Bind
        r = send_rpc_request(process, "binding.create", {
            "tenant": tenant,
            "action_trn": action_trn,
            "auth_trn": auth_trn,
        })
        if not (r and "result" in r):
            # If binding exists already, allow pass by checking get
            rg = send_rpc_request(process, "binding.get", {
                "tenant": tenant,
                "action_trn": action_trn,
            })
            assert rg and "result" in rg, f"binding.get after create failed: {rg}"

        # 6) Run
        r = send_rpc_request(process, "run", {
            "tenant": tenant,
            "action_trn": action_trn,
        })
        assert r and "result" in r, f"run failed: {r}"
        execution_trn = r["result"]["execution_trn"]

        # 7) execution.get
        r = send_rpc_request(process, "execution.get", {
            "execution_trn": execution_trn,
        })
        assert r and "result" in r and "execution" in r["result"], f"execution.get failed: {r}"

        # 8) execution.list
        r = send_rpc_request(process, "execution.list", {
            "tenant": tenant,
            "limit": 10,
            "offset": 0,
        })
        assert r and "result" in r and "executions" in r["result"], f"execution.list failed: {r}"

        print("E2E STDIO-RPC OK", flush=True)
    finally:
        try:
            process.terminate()
            process.wait(timeout=5)
        except Exception:
            process.kill()


if __name__ == "__main__":
    main()


