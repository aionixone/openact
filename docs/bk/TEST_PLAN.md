## OpenAct System Test Plan

### Scope
- Validate end-to-end “Connect” flows (Client Credentials / Authorization Code / Device Code)
- Ensure errors are unified as {error_code, message, hints[]}
- Verify CLI parity with Server (--server mode)

### Environments
- DB: sqlite (temp file per test)
- HTTP: httpmock for external endpoints (token/device authorization/httpbin-like)
- Features: server; server+callback

### Test Matrix (High-level)
- Connect (CC): success / validation failure / storage failure
- Connect (AC): start (authorize_url/run_id) / status pending→done / resume path
- Connect (Device Code): success (auth_trn persisted, optional bind+test) / polling errors
- Connections: status (ready/expiring_soon/expired/unbound/not_authorized/not_issued) / test endpoint
- Callback (feature=callback): redirect link, auto-bind via DSL (bind_connection_trn), state mismatch
- CLI: connect --server (CC/AC) with polling flags; local mode fallbacks

---

### Detailed Test Cases

#### CC-1: Connect CC Success
- Endpoint: POST /api/v1/connect (mode=cc)
- Pre-req: mock token endpoint returns access_token/expires_in
- Steps:
  1) POST body {provider,template,tenant,name,mode:"cc"}
  2) Assert 200; body contains connection/status/test/next_hints
  3) status.status == "ready"; test.status < 400
- Assertions:
  - Fields present; next_hints contains actionable hint
  - No sensitive values in logs (manual check or regex)

#### CC-2: Connect CC Validation Error
- Steps: POST missing required fields (e.g., invalid template)
- Assert 400; {error_code startsWith "validation.", hints[] present}

#### AC-1: Connect AC Start
- Endpoint: POST /api/v1/connect (mode=ac)
- Pre-req: provide minimal valid DSL YAML
- Steps:
  1) POST with dsl_yaml
  2) Assert 200; contains authorize_url/run_id/next_hints

#### AC-2: AC Status Pending→Done (resume path)
- Steps:
  1) Start (AC-1) to get run_id
  2) GET /api/v1/connect/ac/status?run_id=... → done=false & next_hints
  3) POST /api/v1/connect/ac/resume with {run_id,code,state,connection_trn}
  4) GET status again → done=true; contains auth_trn and/or bound_connection; next_hints present

#### AC-3: AC Start Validation Error
- Steps: POST /connect (mode=ac) without dsl_yaml
- Assert 400; error_code=validation.dsl_required; hints present

#### DC-1: Device Code Success
- Endpoint: POST /api/v1/connect/device-code
- Pre-req: mock device_code_url and token_url (authorization_pending→success)
- Steps:
  1) POST with client_id/scope/tenant/provider/user_id
  2) Assert 200; contains auth_trn; optional bind/test results; next_hints present

#### DC-2: Device Code Polling Error
- Pre-req: token endpoint returns non-JSON / missing access_token
- Assert 500 with execution error_code and hints

#### CN-1: Connection Status Variants
- Seed DB with:
  - API Key (ok/misconfigured)
  - OAuth2 CC (token present/absent)
  - OAuth2 AC (auth_ref bound/unbound/expired)
- GET /api/v1/connections/{trn}/status
- Assert status values and no crash

#### CN-2: Connection Test Endpoint
- POST /api/v1/connections/{trn}/test with default endpoint
- Assert 200; body contains status/headers/body

#### CB-1: Callback Redirect Link (feature=callback)
- Start callback server; simulate waiter
- Call /oauth/callback?state=...&redirect=https://app/ok&connection_trn=...
- Assert HTML contains Return to application link with run_id & connection_trn

#### CB-2: Callback Invalid State (feature=callback)
- Call /oauth/callback without matching waiter
- Assert 400 HTML error page

#### CLI-1: Server CC Flow
- Command: openact-cli --server <base> connect --provider ... --template oauth2_cc --tenant ... --name ...
- Assert 输出 JSON 含 connection/status/test/next_hints

#### CLI-2: Server AC Flow with Polling
- Command: connect --server ... --dsl-file flow.yml --poll-interval-secs 1 --poll-timeout-secs 5
- Assert 打印 authorize_url/run_id；轮询状态；输出 ac_status；pending 时打印 hints

#### CLI-3: Local Mode Back-compat
- Command: connect (no --server) using templates service
- Assert success; 输出创建的 TRN

---

### Non-functional
- Logs redaction: tokens/client_secret never appear
- Performance sanity: AC status polling with 50 concurrent run_ids returns <100ms median locally

---

### Execution Order (Phased)
1) CC-1, CC-2
2) AC-1, AC-2, AC-3
3) DC-1, DC-2
4) CN-1, CN-2
5) CB-1, CB-2 (callback feature)
6) CLI-1, CLI-2, CLI-3

---

### Implementation Notes
- Use sqlite temp DB per test (tempfile)
- httpmock to stub token/device endpoints and httpbin
- For AC DSL，提供最小 YAML fixture；resume 时直传 code/state 模拟
- Prefer server::router::core_api_router() + axum::serve with a random port (or use Router::oneshot for handler-level tests)
