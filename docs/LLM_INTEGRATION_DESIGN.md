# OpenAct LLM Integration — Development Plan and Detailed Design

## 1) Overview

OpenAct will add first‑class Large Language Model (LLM) support as a platform‑level subsystem with multiple UX surfaces (TUI, REST, MCP), not only as a connector. We still expose LLM execution through the connector/registry path for composition with other actions, but the subsystem provides conversation/session management, streaming UI, tool orchestration, and observability tailored to interactive use.

## 2) Goals

- Provide a consistent way to call LLMs (chat, embeddings, rerank later) via OpenAct actions and interactive frontends (TUI first).
- Support multiple providers behind a common abstraction (OpenAI‑compatible first, Anthropic next, local OpenAI‑compatible runtimes).
- Keep the shared execution path (registry) and TRN versioning intact.
- Offer streaming and non‑streaming responses, normalized outputs (text, tool_calls, usage).
- Enforce governance (tenancy, allow/deny, concurrency/timeout, token/cost caps) and safe defaults.
- Expose LLM actions to MCP as tools with clear JSON Schemas and hints; add a TUI that supports multi‑turn sessions, streaming, tool call visualization, and transcript persistence.

## 2b) Why subsystem (not only connector)

- Interactive chat needs stateful sessions, transcript persistence, and UI widgets (input area, stream window, tool call panes) beyond stateless action execution.
- Cross‑surface parity: TUI, REST, and MCP should share the same LLM orchestration core (messages, sampling, tool policy, streaming) to avoid duplicated logic.
- Extensibility: Future features like memory, system prompt switching, quick tools, snippets, and session search fit naturally into a subsystem boundary.

## 3) Non‑Goals (Initial)

- Full RAG platform (we will only provide basic resource hooks initially).
- Fine‑grained prompt memory/personalization policies (can be layered later).
- On‑prem provider deployment tooling (assume a configured API base).

## 4) Milestones (Phased Plan)

1. MVP (OpenAI‑compatible + TUI v1):
   - LLM core + OpenAI‑compatible adapter; chat (non‑stream + stream), embeddings.
   - TUI v1: basic interactive chat, streaming render, copy code blocks, save transcripts.
   - REST/MCP wiring; governance caps; normalized result (text, usage).
2. Provider Expansion:
   - Anthropic adapter; support OpenRouter/local OpenAI‑compatible endpoints.
3. Tool Calling & Safety:
   - Tool call bridging to OpenAct actions by TRN with strict JSON Schema validation (visible in TUI as collapsible steps).
   - Optional moderation hooks; TUI shows blocked/filtered events with reason.
4. Cost & Caching:
   - Usage accounting + cost estimates; idempotency; semantic cache (opt‑in).
5. RAG & Advanced:
   - Resource fetch tools; rerank; JSON schema strict mode and auto‑repair loop.
   - TUI features: session search, pin messages, quick actions (slash commands), multi‑pane layout.

## 5) Architecture Additions

### 5.1 Subsystem layout & crates

Proposed crates/modules:
- `openact-llm-core`: session model, message types, orchestration (prompt assembly, streaming assembly, tool policy), provider trait.
- `openact-llm-providers-*`: concrete adapters (openai, anthropic, openrouter, local/openai‑compatible).
- `openact-llm-tui`: interactive terminal UI (crossterm/ratatui), streaming renderer, keybindings, session persistence.
- `openact-llm-rest`: REST handler glue (or integrate into server handlers), SSE endpoints.
- `openact-llm-mcp`: MCP tools glue (or integrate into current MCP server).

LLM also appears as a connector kind (`llm`) for composition with registry‑based actions.

### 5.2 Connector: `llm`

- New connector kind in code: `ConnectorKind::LLM` with canonical name `llm`.
- Connection = provider configuration; Action = model/task configuration.

### 5.3 TRN Scheme (Version Required)

- Connection TRN: `trn:openact:{tenant}:connection/llm/{provider}@v{N}`
- Action TRN: `trn:openact:{tenant}:action/llm/{name}@v{N}`
- Name is decoupled from model to allow retargeting by version.

### 5.4 Provider Abstraction

- Trait (pseudo):
  ```rust
  pub trait LlmProvider: Send + Sync {
      async fn chat(&self, req: ChatRequest) -> Result<ChatResponse>;
      async fn embed(&self, req: EmbedRequest) -> Result<EmbedResponse>;
      // later: rerank, image, batch
  }
  ```
- Adapters:
  - OpenAI‑compatible HTTP first (covers OpenAI and many local servers).
  - Anthropic adapter next (different message format and headers).
  - Optional: custom HTTP adapter for advanced users.

### 5.5 Configuration Schema (Conceptual)

- Connection (provider‑level):
  - `provider`: `openai | anthropic | openrouter | local`
  - `api_base`: string (e.g., `https://api.openai.com`)
  - `authorization`: `bearer_token | api_key`
  - `token` or secrets ref; `default_headers` (map), `timeout_ms`, `rate_limits`, `proxy`.
- Action (model‑level):
  - `task`: `chat | embed`
  - `model`: string (e.g., `gpt-4o-mini`)
  - `system`: string (chat system prompt)
  - `templates`: optional map for prompt templating
  - `sampling`: `{ temperature, top_p, max_tokens }`
  - `json_schema`: optional JSON Schema for strict JSON output
  - `tool_calls`: optional list of `{ tool_trn, name_override?, schema }`
  - `stream`: bool (default false)
  - `mcp_enabled`: bool (expose as MCP tool)

### 5.6 Execution Flow (Chat)

1. Resolve action TRN (respecting version rules) and load action + connection.
2. Normalize input into provider request:
   - Input JSON: `{ messages, tools?, sampling?, response_format?, metadata? }`
   - Merge defaults from action config (`system`, `sampling`, `tool_calls`).
3. Call adapter `provider.chat(req)` with timeout + concurrency limits.
4. Stream or aggregate response; assemble final normalized response:
   - `{ text, tool_calls: [{name,args,id}], usage: {prompt_tokens,completion_tokens,total_tokens}, model_info }`
5. If `json_schema` present: validate; on failure attempt one repair pass (small token budget) and re‑validate.
6. Return response to registry; MCP/REST wrap as usual.

### 5.7 Tool Calling Bridge

- If model emits tool calls:
  - Map to configured `tool_trn` (or block if not configured/allowed).
  - Validate arguments against that action's JSON Schema (when available).
  - Execute via registry with governance (timeout/concurrency/policy patterns).
  - Feed tool results back to the model (optional multi‑turn) or render in response.
  - Default policy: single synchronous turn; later: bounded multi‑turn.

### 5.8 Streaming

- REST: support SSE endpoint variant for chat; accumulate deltas; send final summary (usage, model_info).
- MCP: send incremental `TextContent` blocks; final `structured_content` carries normalized result.
- Respect client cancellation and backpressure; enforce server‑side timeouts.

### 5.9 Governance & Security

- Per‑tenant caps: `max_concurrency`, `timeout`, `max_tokens`, `models_allowed`, `cost_cap`.
- Allow/deny rules already exist; extend patterns to include `llm.*` tools and specific action TRNs.
- Default tool_calls disabled unless specified in action config.
- Redact secrets in logs; optional PII redaction on prompts.
- Optional moderation stage pre/post generation when provider supports it.

### 5.10 Usage & Cost

- Normalize usage into `{prompt_tokens, completion_tokens, total_tokens}`.
- Cost estimation by provider/model (configurable rate sheet) → return in response metadata; optionally store per‑exec.

### 5.11 Observability

- Structured logs: tenant, action_trn, provider, model, latency, usage, error class.
- Metrics: counters/histograms by tenant/provider/model; streaming durations; tool call counts.

### 5.12 Caching & Idempotency

- Optional semantic cache key: hash(provider, model, messages, sampling, json_schema).
- Honor `Idempotency-Key` (REST) to dedupe retries.
- Disable cache for sensitive inputs by flag.

### 5.13 RAG (Later)

### 5.14 TUI Design (v1 → v2)

UI goals:
- Real‑time streaming, stable layout on resize, colorized roles (system/user/assistant/tool).
- Code block detection with quick copy; line wrap toggle; scrollback.
- Slash commands: `/model`, `/system`, `/temperature`, `/save`, `/load`, `/tools on|off`, `/new`.
- Session list pane: recent sessions with timestamp/model, quick search.

State & services:
- `SessionService`: create/load/rename/delete sessions; append messages; persist transcripts.
- `ConversationStore`: pluggable (SQLite default via existing DB; file store optional).
- `EventBus`: stream tokens/events to UI; handle cancellation.
- `Settings`: last used provider/model, sampling defaults, UI preferences.

Keybindings (suggested):
- Enter: send; Shift+Enter: new line.
- Ctrl+S: save session; Ctrl+O: open session list; Ctrl+K: clear screen.
- Tab: cycle panes; F2: settings; F3: tools panel.

MVP scope:
- Single session, send/stream, save transcript to DB; basic settings dialog.
- Display tool call events inline (collapsed by default) once tool bridge lands.

- Provide `resources.get/search` actions to fetch context; inject via templates.
- Add a `rerank` task and adapter; unify schema with chat/embed.

## 6) REST/MCP/TUI Additions

### 6.1 REST

- Execute by name (must specify version or use TRN):
  - `POST /api/v1/actions/llm.chat/execute?version=latest`
  - Body example (chat): `{ "input": { "messages": [{"role":"user","content":"hi"}] } }`
- Execute by TRN:
  - `POST /api/v1/execute` with `{ "action_trn": "trn:openact:default:action/llm/my-chat@v1", "input": {...} }`
- Streaming: SSE variant `.../execute/stream` (to be added) or query flag.

### 6.2 MCP

### 6.3 TUI

- New CLI command `openact chat` (or `openact tui`):
  - Options: `--session <name>`, `--provider <id>`, `--model <id>`, `--stream/--no-stream`.
  - Reads/writes sessions in the existing DB (new table `chat_sessions`, `chat_messages`).
  - Uses governance for timeouts/concurrency and respects allowed models.

- Tools:
  - `llm.chat`: parameters `{ messages, sampling?, json_schema?, stream? }`
  - `llm.embed`: parameters `{ inputs: [string], model? }`
- Generic executor also works with LLM TRNs via `openact.execute`.
- Strictly require `version` or TRN with `@vN` when calling by name.

## 7) Data Model & Migrations

- Actions and connections tables unchanged (JSON config already flexible).
- New tables (TUI + usage):
  - `chat_sessions(id, tenant, name, provider, model, created_at, updated_at, metadata_json)`
  - `chat_messages(id, session_id, role, content_json, tool_call_json, created_at, seq)`
- Optional `usage_records` (later):
  - `id, tenant, action_trn, provider, model, prompt_tokens, completion_tokens, total_tokens, cost, created_at`.

## 8) Config Manager & Import/Export

- Extend loader to recognize `llm` connections/actions in flat config.
- Importer already generates `@vN` TRNs; reuse `VersioningStrategy` (AlwaysBump, ReuseIfUnchanged, ForceRollbackToLatest).
- Exporter redacts secrets and outputs actionable manifests.

## 9) Error Handling

- Invalid input/schema → `INVALID_INPUT`.
- Provider errors → `UPSTREAM_ERROR` with sanitized message.
- Governance violations (model blocked, token/cost cap) → `FORBIDDEN` or `INVALID_INPUT` with reason.
- Timeouts → `TIMEOUT`.

## 10) Testing Strategy

- Unit tests: provider adapters with mocked HTTP; SSE assembly; JSON schema validation and repair loop.
- Integration: REST/MCP name vs TRN, version=latest, governance caps, tool call bridge to an HTTP demo action.
- Load tests: concurrency, token caps, cancellation; streaming correctness.

## 11) Rollout Plan

- Branch `feat/llm-integration` (created).
- Phase 1 PRs:
  1) Core interfaces + config schemas + OpenAI adapter stub (chat non‑stream) + TUI skeleton + golden tests.
  2) Streaming + embeddings + REST/MCP plumbing + governance caps + TUI streaming render + transcripts.
  3) Tool call bridge + schema validation + TUI tool events + examples.
  4) Anthropic adapter + docs + examples + TUI session management polish.

## 12) Risks & Mitigations

- Provider divergence: hide behind adapter mapping and normalized schema.
- Cost overruns: enforce governance caps and default conservative sampling.
- Tool misuse: opt‑in tool_calls, TRN allowlist, JSON Schema validation.
- Streaming complexity: provide robust assembly with backpressure and cancellation.

## 13) Open Questions

- Do we persist usage/cost per execution now or later?
- Which providers are mandatory in MVP beyond OpenAI‑compatible?
- Default JSON strict mode policy and repair retries?

## 14) Appendix A — Example Flat Config (YAML)

```yaml
version: "1.0"

connections:
  my-llm:
    kind: llm
    provider: openai
    api_base: https://api.openai.com
    authorization: bearer_token
    token: ${OPENAI_API_KEY}
    timeout_ms: 30000

actions:
  chat-support:
    connection: my-llm
    kind: llm
    task: chat
    model: gpt-4o-mini
    system: |
      You are a helpful assistant.
    sampling:
      temperature: 0.2
      max_tokens: 512
    mcp_enabled: true

  embed-search:
    connection: my-llm
    kind: llm
    task: embed
    model: text-embedding-3-small
    mcp_enabled: false
```

## 15) Appendix B — Provider Request/Response (Pseudo)

```rust
pub struct ChatRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub tools: Option<Vec<ToolSpec>>, // optional for function calling
    pub sampling: Option<Sampling>,
    pub response_format: Option<ResponseFormat>, // json/none
    pub stream: bool,
}

pub struct ChatResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
    pub model_info: ModelInfo,
}
```
