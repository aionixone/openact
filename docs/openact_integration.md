# OpenAct Integration Guide (Command → Event)

This document describes how the OpenAct service integrates with Stepflow using the
shared `aionix-contracts` envelopes (which wrap the lower-level `aionix-protocol`
schemas). Stepflow already accepts these envelopes on both the command
and event paths, so OpenAct only needs to adopt the same payload shapes.

## 1. Overview

```
Stepflow Runtime ──(CommandEnvelope)────► OpenAct Action
      ▲                                      │
      │                                      ▼
      └──(EventEnvelope via /stepflow/events)◄── Waiter / Callback
```

* Commands are expressed as `CommandEnvelope` JSON; Stepflow runtime already
  constructs these when dispatching workflow tasks.
* OpenAct validates the command, executes the requested action, and returns:
  * **sync** results immediately when possible.
  * **async** acknowledgement + later emits an `EventEnvelope` to Stepflow’s
    `/stepflow/events` endpoint (waiter behaviour).
* All request/response payloads should be validated with the
  [`aionix-contracts`](https://github.com/aionixone/aionix-contracts) crate (Rust) or the
  generated SDKs for other languages.

## 2. Command Handling

OpenAct must expose a task execution endpoint (e.g. `POST /actions/{trn}/execute`) that takes a
`CommandEnvelope`. Required fields:

| Field | Purpose |
| --- | --- |
| `schemaVersion` | Presently `"0.1.0"`; reject unsupported versions. |
| `id` | Unique command ID (UUIDv7 recommended). |
| `timestamp` | RFC3339 issuance time. |
| `command` | Namespace verb such as `aionix.openact.action.execute`. |
| `source` | TRN of the caller (`trn:stepflow:{tenant}:engine`). |
| `target` | Action TRN to be executed. |
| `tenant` | Tenant/workspace identifier. |
| `traceId` | Propagated distributed trace. |
| `parameters` | Action-specific payload (usually `{ "input": ..., ... }`). |
| `correlationId` | Mirrors `command.id`; use it for downstream logging. |

### Command Validation Checklist

1. Parse with `aionix_contracts::parse_command_envelope` (Rust) or the generated validator.
2. Ensure the `target` TRN belongs to OpenAct and resolve it to an internal action.
3. Authorise using `tenant`, `actorTrn`, `authzScopes` if provided.
4. Derive execution context:
   ```rust
   let action_trn = envelope.target;
   let input = envelope.parameters.get("input").cloned().unwrap_or(Value::Null);
   let trace_id = envelope.trace_id;
   let idempotency_key = envelope.idempotency_key;
   ```

### Command Response Patterns

| Scenario | Response |
| --- | --- |
| Immediate completion | `200 OK` + payload `{ "status": "succeeded", "result": ... }`. |
| Job accepted, running asynchronously | `202 Accepted` + handle object (see below). |
| Fire-and-forget trigger | `202 Accepted` + `{ "status": "accepted" }`. |
| Validation error | `400/422` with details. |
| Idempotency conflict | `409 Conflict`. |

### Async handle schema

When returning `status: "running"` include a handle describing how OpenAct should wait,
refresh heartbeats, and cancel:

```json
{
  "status": "running",
  "phase": "async_waiting",
  "runId": "openact-run-1234",
  "heartbeatTimeout": 60,
  "statusTtl": 3600,
  "handle": {
    "backendId": "generic_async",
    "externalRunId": "mock-123",
    "config": {
      "tracker": {
        "kind": "http_poll",
        "url": "https://jobs.example.com/api/runs/{{externalRunId}}",
        "method": "GET",
        "interval_ms": 2000,
        "timeout_ms": 900000,
        "max_attempts": 100,
        "backoff_factor": 1.5,
        "success_status": [200, 202],
        "failure_status": [400, 404, 500],
        "success_conditions": [
          { "pointer": "/state", "equals": "SUCCEEDED" },
          { "pointer": "/details/message", "contains": "completed" }
        ],
        "failure_conditions": [
          { "pointer": "/state", "equals": "FAILED" },
          { "pointer": "/state", "regex": "^ERROR" }
        ],
        "result_pointer": "/payload/output"
      },
      "cancel": {
        "kind": "http",
        "url": "https://jobs.example.com/api/runs/{{externalRunId}}/cancel",
        "method": "POST",
        "headers": {
          "X-Cancel-Reason": "{{reason}}"
        },
        "body": {
          "reason": "{{reason}}"
        }
      }
    }
  }
}
```

* `tracker.kind = http_poll` instructs OpenAct to poll the remote status until a success
  or failure condition is satisfied. Success / failure can be determined by HTTP status,
  response body fields, or both. Supported body predicates:
  * `equals` / `not_equals` — compare JSON values.
  * `contains` — substring search on string values.
  * `regex` — regular expression match (Rust regex syntax).
  * `jsonpath` — evaluate an expression (e.g. `$.items[?(@.state=='DONE')]`) with optional
    `equals`/`exists` flags.
  * `greater_than`, `greater_or_equal`, `less_than`, `less_or_equal` — numeric comparisons.
  * absence of a predicate means “exists and is non-null”.
* `backoff_factor` defaults to `1.0`, enabling exponential backoff when > 1.0.
* `success_conditions` / `failure_conditions` use JSON pointer syntax. When no `equals`
  is supplied, the condition is satisfied if the pointer resolves to a non-null value.
* `cancel.kind = http` describes a best-effort cancellation endpoint. Templates can
  reference `{{externalRunId}}` and `{{reason}}`.

OpenAct persists the handle metadata so the waiter and cancel code can recover state
after restarts.

## 3. Waiter / Event Emission

When the action completes (success or failure), OpenAct must post an `EventEnvelope`
to Stepflow’s `/stepflow/events` endpoint. This is performed by the waiter subsystem:

1. **Tracker**: executes according to the handle (`http_poll`, `mock_complete`, …).
2. **Heartbeat refresh**: each poll updates `heartbeat_at`; the HeartbeatSupervisor will
   mark the run as timed out if heartbeats stop.
3. **Cancel plan**: when a cancel request is received, the manager executes the configured
   cancel plan before marking the run cancelled.

### HTTP Endpoint

```
POST https://{stepflow-host}/api/v1/stepflow/events
Content-Type: application/json
Body: EventEnvelope
```

Use `aionix_contracts::parse_event_envelope` (or `validate_event_envelope`) to validate before sending. On HTTP errors
replay with exponential backoff; Stepflow’s endpoint is idempotent (by `id`).

### EventEnvelope Mapping

| Field | Value |
| --- | --- |
| `specversion` | `"1.0"` |
| `id` | Unique event UUID |
| `source` | `trn:openact:{tenant}:executor` (or specific adapter TRN) |
| `type` | e.g. `aionix.openact.action.succeeded` / `.failed` / `.cancelled` |
| `time` | RFC3339 completion time |
| `data` | JSON payload: `{ "status": 200, ... }` |
| `aionixSchemaVersion` | `"1.1.0"` |
| `tenant` | Mirrors command tenant |
| `traceId` | Same as the command’s `traceId` |
| `resourceTrn` | The action’s TRN (`CommandEnvelope.target`) |
| `runId` | Identifier of the Stepflow execution (from the command extensions or metadata) |
| `correlationId` | Command `id` |
| `relatedTrns` | Optional list (e.g. external resources touched) |
| `actorTrn` | If known |

Example (success):

```json
{
  "specversion": "1.0",
  "id": "018fb2e1-...",
  "source": "trn:openact:acme:executor",
  "type": "aionix.openact.action.succeeded",
  "time": "2025-10-10T12:36:20Z",
  "datacontenttype": "application/json",
  "data": {
    "status": 200,
    "durationMs": 480,
    "output": { "body": "..." }
  },
  "aionixSchemaVersion": "1.1.0",
  "tenant": "acme",
  "traceId": "00-4bf92f35-...",
  "resourceTrn": "trn:openact:acme:action/http/send-mail@v3",
  "runId": "openact-run-1234",
  "correlationId": "018fb0c2-...",
  "relatedTrns": [
    "trn:oss:acme:object/reports/2025-10-10.json@v1"
  ]
}
```

For a failure, change `type` to `aionix.openact.action.failed` and include `data.error`.

### Recommended Workflow

1. Persist the command (idempotency check, state `RUNNING`).
2. Execute the action. If asynchronous, store the handle (runId / externalTrn).
3. Upon completion, build and post the `EventEnvelope`.
4. Update internal state + send any additional domain events if needed.

## 4. Optional: Server-Side Waiter Queue

To avoid losing events, encapsulate the waiter as a simple outbox:

```rust
async fn emit_event(event: &EventEnvelope) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    client
        .post("https://stepflow/api/v1/stepflow/events")
        .json(event)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
```

If the HTTP call fails, persist the envelope in a retry queue and retry with backoff.

## 5. OpenAct Orchestrator Runtime

OpenAct persists asynchronous executions before returning a `202 Accepted`. Two tables in the
SQLite store keep the state durable even if the service restarts:

| Table | Purpose | Key Columns |
| --- | --- | --- |
| `orchestrator_runs` | One row per command execution (`run_id`). Tracks `status`, `phase`, `heartbeat_at`, `deadline_at`, serialized `result` / `error`, async handle metadata, and the external reference id. | `run_id` primary key |
| `orchestrator_outbox` | Pending `EventEnvelope` payloads waiting to be delivered back to Stepflow (or another orchestrator). | `id` (auto increment), `run_id`, `next_attempt_at`, `attempts` |

### Callback Endpoint

For long-running jobs, external systems (or internal pollers) can report completion through
OpenAct’s callback API:

```
POST /api/v1/orchestrator/runs/{runId}/completion
Content-Type: application/json
Body: { "status": "succeeded" | "failed" | "cancelled", "result": {...}, "error": {...} }
```

* `status` – required; case-insensitive (`succeeded`, `failed`, `cancelled`).
* `result` – optional JSON payload used when the run succeeds.
* `error` – optional JSON payload describing the failure / cancellation reason.

The handler performs three steps:

1. Load the persisted run and update `status` / `phase` / `result` / `error`.
2. Create an appropriate `EventEnvelope` (`aionix.openact.action.succeeded|failed|cancelled`).
3. Enqueue the event into `orchestrator_outbox` for the dispatcher to deliver.

### Background Workers

Two background tasks run inside the server; they are spawned automatically when the REST
or unified server starts:

| Task | Purpose | Key Env Vars (defaults) |
| --- | --- | --- |
| `OutboxDispatcher` | Sends pending events to `OPENACT_STEPFLOW_EVENT_ENDPOINT` with retry/backoff. | `OPENACT_OUTBOX_BATCH_SIZE` (50), `OPENACT_OUTBOX_INTERVAL_MS` (1000), `OPENACT_OUTBOX_RETRY_INITIAL_MS` (30000), `OPENACT_OUTBOX_RETRY_MAX_MS` (300000), `OPENACT_OUTBOX_RETRY_FACTOR` (2.0), `OPENACT_OUTBOX_RETRY_MAX_ATTEMPTS` (5) |
| `HeartbeatSupervisor` | Scans `orchestrator_runs` for stale heartbeats, marks them `TIMED_OUT`, and enqueues timeout events. | `OPENACT_HEARTBEAT_BATCH_SIZE` (50), `OPENACT_HEARTBEAT_INTERVAL_MS` (1000), `OPENACT_HEARTBEAT_GRACE_MS` (5000) |

Both tasks emit structured logs via `tracing`; hook these into your logging/metrics pipeline to
monitor delivery success, retries, and timed-out runs.

### State Transitions

1. `RUNNING` – persisted when the command is accepted; `heartbeat_at` is set immediately.
2. `SUCCEEDED` / `FAILED` – set either by the Stepflow handler (sync results) or by the callback API.
3. `CANCELLED` – callback API reports a cancellation (optional for orchestrators that support it).
4. `TIMED_OUT` – heartbeat supervisor detects `heartbeat_at` older than the grace period.

Every terminal state transition enqueues a corresponding event into the outbox so that
the orchestrator is notified.

## 6. Testing Checklist

* Unit tests validating `CommandEnvelope` → action dispatcher (use the schema validator).
* Unit tests building `EventEnvelope` for each outcome.
* Integration test with Stepflow sandbox:
  1. POST command to OpenAct’s execute endpoint.
  2. Simulate async completion → POST to `/api/v1/orchestrator/runs/{runId}/completion`.
  3. Confirm the outbox dispatcher emits `EventEnvelope` to Stepflow and the workflow continues.

## 7. References

* Contracts crate: <https://github.com/aionixone/aionix-contracts>
* Protocol schemas: <https://github.com/aionixone/aionix-protocol>
* Stepflow event handler: `crates/stepflow-http-server/src/handlers/events.rs`
* Runtime ingestion: `crates/stepflow-runtime/src/services/events.rs`
* Runtime command dispatch (builder): `crates/stepflow-runtime/src/runtime/task_handler.rs`

With this guide, OpenAct can implement envelope-based command ingestion and waiter-driven
event emission that matches Stepflow’s expectations.
