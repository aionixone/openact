# OpenAct Connector Development Guide

This document defines the conventions and expectations for building and maintaining OpenAct connectors.

## 1. Core Architecture

- Connectors integrate via the factory traits in `crates/openact-registry/src/factory.rs`.
- Each connector crate must provide:
  - `ConnectionFactory` + `ActionFactory` implementations producing `Arc<dyn Connection>` and `Box<dyn Action>`.
  - Runtime wrappers that implement `Connection` / `Action` and `AsAny` for downcasting.
  - A `registrar()` function that registers both factories with `ConnectorRegistry` (see HTTP/Postgres connectors).
- `ConnectorRegistry` ( `crates/openact-registry/src/registry.rs`) caches connections, derives MCP schemas/annotations, and executes actions. Connectors should avoid internal caching that competes with registry behaviour.

### Required Traits

| Trait | Responsibilities |
|-------|-------------------|
| `ConnectionFactory` | Parse `ConnectionRecord.config_json`, return `Arc<dyn Connection>`, supply metadata. |
| `ActionFactory` | Parse `ActionRecord.config_json`, downcast to the connector-specific connection type, return `Box<dyn Action>`. |
| `Connection` | Expose `trn`, `connector_kind`, `health_check`, and metadata for observability. |
| `Action` | Implement `execute`, optional `validate_input`, `mcp_input_schema`, `mcp_output_schema`, `mcp_wrap_output`, `mcp_annotations`. |

## 2. Repository Layout

Recommended directory structure within a connector crate:

```
crate/
  src/
    connection.rs      # serde config structs, defaults, validation helpers
    action.rs          # action config + serde definitions
    executor.rs        # runtime execution logic
    factory.rs         # ConnectionFactory & ActionFactory implementations
    mod.rs             # exports registrar()
```

Keep config structs `Serialize` + `Deserialize` and versioned (e.g. `config_version`) when schema may evolve.

## 3. Configuration & Spec Alignment

- Authoring format follows `docs/config/spec.yaml`.
- Connections accept flattened key/values or `config` wrapper; loaders normalize to `ConnectionRecord.config_json`.
- Actions must store execution settings inside `config` and declare `parameters` for validation/MCP schema derivation.
- New connector kinds must document expected `config` fields and example YAML in the spec.
- Respect canonical `kind` IDs (e.g. `postgres`, `http`), matching `ConnectorKind::canonical()`.

### Error Handling

- Config parsing failures → `RegistryError::ConnectionCreationFailed` / `ActionCreationFailed` with actionable messages.
- Execution failures should wrap lower-level errors (`anyhow::Context` recommended) and surface sanitized outputs.

### MCP Exposure

- Set `ActionRecord.mcp_enabled` and `McpOverrides` via config loader.
- `Action::mcp_input_schema` / `mcp_output_schema` should represent actual runtime inputs/outputs. Only fall back to default permissive schemas when unsuitable.
- Use `tool_name` patterns `<connector>.<action>` unless a domain-specific alias is required.

## 4. Execution & Validation

- `Connection::health_check` must exercise real connectivity or sanity checks (ping endpoint, open DB connection, etc.).
- `Action::validate_input` validates call-time arguments against declared `parameters` (JSON Schema or custom rules).
- `execute` should consume the merged configuration according to connector rules (HTTP merge precedence, SQL parameter binding, etc.).
- Prefer reusable executors (e.g. HTTP executor) to avoid duplicated logic across actions of the same connector.

## 5. Testing Strategy

Minimum coverage per connector:

1. `create_connection` happy path + invalid config rejection.
2. `create_action` correct downcasting and config parsing.
3. `execute` exercising core behaviour (use mocks/fakes where necessary).
4. `mcp_input_schema` / `mcp_output_schema` returning expected schemas.
5. Registry integration test verifying MCP schema caching (example: `derive_mcp_schemas_uses_cache`).

Whenever possible, add integration tests using mock servers (HTTP) or in-memory stores (SQLlite/Postgres test harness) to validate real execution flows.

## 6. Naming, Governance & Discovery

- Connector names (kinds) should be short, lowercase, and stable. Document aliases if loader accepts them but normalize before storage.
- Action keys are globally unique; choose semantic names as they become default MCP tool IDs.
- Governance filters operate on tool names (`governance.allow_patterns` / `deny_patterns`); design connector override names to cooperate with policy patterns.
- `mcp.requires_auth`, `tags`, and `metadata` should reflect connector capabilities for clients and governance engines.

## 7. Development Workflow Checklist

1. **Plan config schema**: define connection/action fields, defaults, validation, and spec documentation.
2. **Implement config structs** with serde + version fields.
3. **Write factories** that parse config and construct runtime wrappers using `Arc<dyn Connection>`.
4. **Implement runtime wrappers** with `health_check`, `execute`, validation, and MCP schema hooks.
5. **Register factories** via `registrar()` and ensure plugin registration.
6. **Add documentation**: update `docs/config/spec.yaml` with examples and notes for the new connector.
7. **Write tests** covering parsing, execution, MCP hooks, registry caching.
8. **Validate with cargo**: `cargo check`, `cargo test -p <connector>`, and relevant integration tests.

## 8. Extending the Config Spec

When introducing a new connector kind:

- Add a subsection in `docs/config/spec.yaml` describing the connection/action fields, merge semantics, parameter handling, and governance considerations.
- Include YAML examples demonstrating Style A (flattened) and Style B (`config:`) authoring.
- Document any connector-specific limits (e.g. `limits.max_rows`) and how they map to runtime enforcement.
- Align spec comments with connector implementation, ensuring loader/normalizer output matches documented expectations.

## 9. Reference Implementations

- **HTTP connector** showcases generic execution, layered config merge, multiple auth strategies, and structured output.
- **Postgres connector** illustrates domain-specific parameter binding and pool management.

Use these as canonical references when designing new connectors; deviate only with documented justification.

---

For questions or proposals, open an RFC in `/docs/rfcs/` and link back to this guide.
