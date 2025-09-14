## OpenAct HTTP API v1 (Draft)

All endpoints are planned and currently Not Implemented.

- Base URL: `/api/v1`
- Content-Type: `application/json` (actions supports `text/yaml` upload)
- Error format: `{ "error": { "code": string, "message": string, "details"?: object } }`

### 1) Health & Status
- [GET] `/health` — Liveness probe
  - Status: Not Implemented
- [GET] `/status` — Basic stats and encryption/env info
  - Status: Not Implemented

### 2) Doctor (Environment & Config Preflight)
- [POST] `/doctor`
  - Body: `{ dsl?: string, portStart?: number, portEnd?: number }`
  - Status: Not Implemented

### 3) OAuth Authentication (Two-step)
- [POST] `/auth/oauth/begin`
  - Body: `{ tenant, provider, dsl, flow?, redirectUri?, scope? }`
  - Resp: `{ authorize_url, session_id, redirect_uri }`
  - Status: Not Implemented
- [POST] `/auth/oauth/complete`
  - Body: `{ tenant, session_id, callback_url? | code? & state? }`
  - Resp: `{ auth_trn }`
  - Status: Not Implemented
- [GET] `/oauth/callback` (browser redirect target)
  - Query: `code`, `state`
  - Status: Not Implemented

### 4) PAT Authentication
- [POST] `/auth/pat`
  - Body: `{ tenant, provider, user_id, token }`
  - Resp: `{ auth_trn }`
  - Status: Not Implemented

### 5) Auth Resources
- [GET] `/auth` — List connections (filters: `tenant`, `provider`)
  - Status: Not Implemented
- [GET] `/auth/{trn}` — Inspect connection (masked)
  - Status: Not Implemented
- [POST] `/auth/{trn}/refresh` — Refresh if supported
  - Status: Not Implemented
- [DELETE] `/auth/{trn}` — Delete connection
  - Status: Not Implemented

### 6) Action Management
- [POST] `/actions` — Register
  - JSON: `{ tenant, provider, name, trn, yaml }` or `text/yaml` with query params
  - Resp: `{ trn }`
  - Status: Not Implemented
- [GET] `/actions` — List (filter: `tenant`)
  - Status: Not Implemented
- [GET] `/actions/{trn}` — Inspect
  - Status: Not Implemented
- [PUT] `/actions/{trn}` — Update YAML
  - Body: `{ yaml }` or `text/yaml`
  - Status: Not Implemented
- [GET] `/actions/{trn}/export` — Export YAML
  - Content-Type: `text/yaml`
  - Status: Not Implemented
- [DELETE] `/actions/{trn}` — Delete
  - Status: Not Implemented

### 7) Bindings (Auth ↔ Action)
- [POST] `/bindings` — Bind `{ tenant, auth_trn, action_trn }`
  - Status: Not Implemented
- [DELETE] `/bindings` — Unbind `{ tenant, auth_trn, action_trn }`
  - Status: Not Implemented
- [GET] `/bindings` — List (filters: `tenant`, `auth_trn`, `action_trn`, `verbose`)
  - Status: Not Implemented

### 8) Action Execution
- [POST] `/run`
  - Body: `{ tenant, action_trn, exec_trn?, headers?, output?=text|json, dry_run?=false, trace?=false }`
  - Resp (dry_run=false): `{ status, response?, error?, status_code?, duration_ms?, exec_trn, action_trn }`
  - Resp (dry_run=true): `{ preview: { method, path, base_url, headers, ... } }`
  - Status: Not Implemented
- [GET] `/executions/{exec_trn}` — Inspect an execution
  - Status: Not Implemented

### 9) Session Utilities (Optional, for debugging)
- [GET] `/sessions` — List saved sessions
  - Status: Not Implemented
- [GET] `/sessions/{id}` — Inspect session
  - Status: Not Implemented
- [DELETE] `/sessions` (`?all=true`) or `/sessions/{id}` — Cleanup
  - Status: Not Implemented

### Status Codes (Guideline)
- 200 OK, 202 Accepted (future async), 400 Bad Request, 401 Unauthorized (if applicable), 404 Not Found,
  409 Conflict, 422 Unprocessable Entity, 500 Internal Server Error, 501 Not Implemented


