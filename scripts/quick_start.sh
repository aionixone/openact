#!/bin/bash

# openact 快速入门脚本（已更新为使用 CLI 与本地 SQLite 存储）

set -euo pipefail

echo "🚀 openact 快速入门 (CLI 模式)"
echo "=============================="

if [ ! -f "Cargo.toml" ]; then
  echo "❌ 请在项目根目录运行"
  exit 1
fi

# 1) 准备临时数据库与主密钥
TMPDIR=$(mktemp -d)
export OPENACT_DB_URL="sqlite:$TMPDIR/quickstart.db?mode=rwc"
export OPENACT_MASTER_KEY=00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff
echo "📦 DB: $OPENACT_DB_URL"

# 2) 创建一个 API Key 连接与任务
echo "🧩 创建连接与任务..."
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

# 3) 执行并输出结果
echo "🏃 执行任务..."
cargo run -q --features server --bin openact-cli -- execute trn:openact:default:task/qs --json | sed -n '1,60p'

# 4) 查看统计
echo "📊 系统统计:"
cargo run -q --features server --bin openact-cli -- system stats --json | sed -n '1,80p'

echo ""
echo "🎉 快速入门完成！(临时目录: $TMPDIR)"
