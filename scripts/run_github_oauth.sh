#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# GitHub OAuth2 + HTTP connector bridge demo
# ---------------------------------------------------------------------------
# Usage:
#   ./scripts/run_github_oauth.sh \
#       --client-id <github_client_id> \
#       --client-secret <github_client_secret> \
#       [--tenant default] \
#       [--redirect-uri http://localhost:8080/oauth/callback] \
#       [--db ./openact.db]
#
# The script will:
#   1. Run the GitHub OAuth2 StepFlow template.
#   2. Persist the resulting token into auth_connections.
#   3. Generate a sample HTTP connection/action config referencing the auth_ref.
#   4. Print follow-up commands to execute the sample action.
# ============================================================================

TENANT="default"
REDIRECT_URI="http://localhost:8080/oauth/callback"
DB_PATH="./openact.db"
FLOW_INPUT="/tmp/github-flow-input.json"
FLOW_OUTPUT="/tmp/github-flow-output.json"
CONN_CONFIG="/tmp/github-connection.yaml"

usage() {
  cat <<'USAGE'
Usage: run_github_oauth.sh --client-id ID --client-secret SECRET [options]

Options:
  --tenant TENANT               Tenant name (default: default)
  --redirect-uri URI            OAuth redirect URI (default: http://localhost:8080/oauth/callback)
  --db PATH                     Sqlite database path (default: ./openact.db)
  --flow-input PATH             Temp file for flow input (default: /tmp/github-flow-input.json)
  --flow-output PATH            Temp file for flow output (default: /tmp/github-flow-output.json)
  --connection-config PATH      Output YAML for HTTP connection (default: /tmp/github-connection.yaml)
  -h, --help                    Show this help message

Example:
  ./scripts/run_github_oauth.sh \
    --client-id Ov23... \
    --client-secret 869a... \
    --tenant demo
USAGE
}

CLIENT_ID=""
CLIENT_SECRET=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --client-id)
      CLIENT_ID="$2"; shift 2 ;;
    --client-secret)
      CLIENT_SECRET="$2"; shift 2 ;;
    --tenant)
      TENANT="$2"; shift 2 ;;
    --redirect-uri)
      REDIRECT_URI="$2"; shift 2 ;;
    --db)
      DB_PATH="$2"; shift 2 ;;
    --flow-input)
      FLOW_INPUT="$2"; shift 2 ;;
    --flow-output)
      FLOW_OUTPUT="$2"; shift 2 ;;
    --connection-config)
      CONN_CONFIG="$2"; shift 2 ;;
    -h|--help)
      usage; exit 0 ;;
    *)
      echo "Unknown argument: $1" >&2
      usage; exit 1 ;;
  esac
  done

if [[ -z "$CLIENT_ID" || -z "$CLIENT_SECRET" ]]; then
  echo "Error: --client-id and --client-secret are required." >&2
  usage
  exit 1
fi

cat <<JSON > "$FLOW_INPUT"
{
  "clientId":    "$CLIENT_ID",
  "clientSecret":"$CLIENT_SECRET",
  "tenant":      "$TENANT",
  "redirectUri": "$REDIRECT_URI"
}
JSON

echo "[1/6] Flow input written to $FLOW_INPUT"

echo "[2/6] Running OAuth flow. Follow console instructions to authorize." \
     "(Ctrl+C to abort)"
cargo run -p openact-cli -- \
  --db-path "$DB_PATH" \
  flow-run \
  --dsl templates/providers/github/oauth2.json \
  --input-file "$FLOW_INPUT" \
  --output "$FLOW_OUTPUT"

echo "[3/6] Flow output saved to $FLOW_OUTPUT"
cat "$FLOW_OUTPUT" | jq

AUTH_REF=$(jq -r '.auth_ref // .trn // empty' "$FLOW_OUTPUT")
if [[ -z "$AUTH_REF" ]]; then
  echo "Error: Flow output missing auth_ref/trn." >&2
  exit 1
fi

echo "[4/6] Extracted auth_ref: $AUTH_REF"

echo "[5/6] Inspecting auth_connections table:"
sqlite3 "$DB_PATH" <<SQL
.headers on
.mode column
SELECT trn, tenant, provider, user_id, token_type, expires_at
FROM auth_connections
WHERE trn = '$AUTH_REF';
SQL

cat <<YAML > "$CONN_CONFIG"
connections:
  github_api:
    kind: http
    auth_ref: "$AUTH_REF"
    base_url: "https://api.github.com"
    headers:
      User-Agent: "openact/0.1"

actions:
  github.user:
    connection: github_api
    config:
      method: "GET"
      path: "/user"
      headers:
        Accept: "application/vnd.github+json"
YAML

echo "[6/6] HTTP connection config written to $CONN_CONFIG"

echo
cat <<'NOTE'
Next steps (manual):
  # Import or update connectors (replace with your CLI command if different)
  cargo run -p openact-cli -- connectors import --file /tmp/github-connection.yaml --db ./openact.db

  # Execute the sample action to verify token reuse
  cargo run -p openact-cli -- connectors exec --action github.user --input '{}' --db ./openact.db

If the request succeeds and returns GitHub /user data, the auth_ref binding works.
NOTE
