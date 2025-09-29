#!/usr/bin/env bash
set -euo pipefail

HOST=${1:-127.0.0.1}
PORT=${2:-3001}
URL="http://$HOST:$PORT/mcp"

echo "# tools/list"
curl -sS -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":"1","method":"tools/list"}' \
  "$URL" | jq .

echo
echo "# tools/call (openact.execute) example"
curl -sS -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc":"2.0",
    "id":"2",
    "method":"tools/call",
    "params":{
      "name":"openact.execute",
      "arguments":{
        "connector":"postgres",
        "action":"search-users",
        "input":{"name":"alice"}
      }
    }
  }' \
  "$URL" | jq .

