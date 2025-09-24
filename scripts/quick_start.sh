#!/bin/bash

# openact quick start script (updated to use CLI and local SQLite storage)

set -euo pipefail

echo "🚀 openact Quick Start (CLI Mode)"
echo "=============================="

if [ ! -f "Cargo.toml" ]; then
  echo "❌ Please run in the project root directory"
  exit 1
fi

# 1) Prepare temporary database and master key
TMPDIR=$(mktemp -d)
export OPENACT_DB_URL="sqlite:$TMPDIR/quickstart.db?mode=rwc"
export OPENACT_MASTER_KEY=00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff
echo "📦 DB: $OPENACT_DB_URL"

# 2) Create an API Key connection and task
echo "🧩 Creating connection and task..."
cat > "$TMPDIR/conn.json" <<'JSON'
{
  "trn": "trn:openact:default:connection/qs",
  "name": "quickstart-conn",
  "authorization_type": "api_key",
  "auth_parameters": {
    "api_key_auth_parameters": { "api_key_name": "X-API-Key", "api_key_value": "demo" }
  },
  "created_at": "2025-01-01T00:00:00Z",
  "updated_at": "2025-01-01T00:00:00Z",
  "version": 1
}
JSON

cat > "$TMPDIR/task.json" <<'JSON'
{
  "trn": "trn:openact:default:task/qs",
  "name": "quickstart-task",
  "connection_trn": "trn:openact:default:connection/qs",
  "api_endpoint": "https://postman-echo.com/get",
  "method": "GET",
  "created_at": "2025-01-01T00:00:00Z",
  "updated_at": "2025-01-01T00:00:00Z",
  "version": 1
}
JSON

cargo run -q --features server --bin openact-cli -- connection upsert --file "$TMPDIR/conn.json"
cargo run -q --features server --bin openact-cli -- task upsert --file "$TMPDIR/task.json"

# 3) Execute and output results
echo "🏃 Executing task..."
cargo run -q --features server --bin openact-cli -- execute trn:openact:default:task/qs --json | sed -n '1,60p'

# 4) View statistics
echo "📊 System statistics:"
cargo run -q --features server --bin openact-cli -- system stats --json | sed -n '1,80p'

echo ""
echo "🎉 Quick start complete! (Temporary directory: $TMPDIR)"
