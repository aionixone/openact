## OpenAct 开发执行计划（对齐当前代码）

### 概要
- 目标：稳定“最小可用闭环”并固化接口契约，随后完善可靠性与可观测性。
- 范围：Connections/Tasks CRUD、Execute、CLI 本地/远程一致性、统一错误模型、测试与文档。

---

### 当前状态（基于仓库现状）
- 接口层：`src/server/handlers/{connections.rs,tasks.rs,execute.rs,system.rs}` 与 `src/server/router.rs` 已提供 CRUD/执行/系统端点。
- 应用层：`src/app/service.rs` 封装 `StorageService + Executor`，支持 overrides 合并与统计/清理。
- 执行层：`src/executor/{http_executor.rs,parameter_merger.rs,client_pool.rs}` 已实现 ConnectionWins 合并、HTTP 策略、重试骨架、连接池；`src/oauth/runtime.rs` 已打通 OAuth2 CC/AC（DB/内存缓存、singleflight、refresh）。
- 存储层：`src/store/{database.rs,connection_repository.rs,task_repository.rs,service.rs}`（SQLite+迁移、加密字段、TTL 缓存、统计）。
- CLI：`src/cli/mod.rs` 支持本地与 `--server` 模式，CRUD/execute 走统一 DTO。
- 可观测性：日志完整；指标默认 Noop，可按 feature 切 Prometheus（端点已占位）。

---

### 近期计划（1–2 周）

#### 📋 任务依赖关系
```
错误模型统一 → HTTP处理器单测 → 示例脚本校对
     ↓
Upsert DTO 收敛 → 日志脱敏（可并行）
```

#### 🎯 关键里程碑
- **Day 2 中点**：错误模型 + Upsert DTO 完成（基础契约稳定）
- **Day 3 末**：单测通过，核心功能验证完毕
- **Day 5 末**：可靠性改进完成，可对外发布

#### 1) 统一错误模型与响应（高优先级，基础设施）
- 目标：所有 HTTP 端点返回统一 `ApiError { code, message, details? }`；状态码映射一致（400/404/409/500）。
- 变更点：
  - 使用 `src/interface/error.rs::ApiError` 替换 handlers 中内联 JSON 错误。
  - 在 `src/server/handlers/*` 抽出错误助手（本地函数）统一构造 `(StatusCode, Json<ApiError>)`。
- 验收：同类错误返回统一 code；CLI `--server` 模式对错误能稳定解析并打印。
- **风险**：确保 CLI 错误解析兼容性。

#### 2) Upsert 输入 DTO 收敛（高优先级）
- 目标：创建/更新时客户端无需提供 `created_at/updated_at/version`；由服务端生成。
- 变更点：
  - 在 `src/interface/dto.rs` 新增 `ConnectionUpsertRequest`、`TaskUpsertRequest`（仅业务字段）。
  - `src/server/handlers/{connections.rs,tasks.rs}` 接收上述 DTO，handler 内使用 `Utc::now()` 与 `version=1` 补齐元数据后调用 `StorageService`。
- 验收：README/示例 JSON 去除时间戳字段仍可成功创建；返回对象带服务端时间。

#### 3) HTTP 处理器单测（高优先级）
- 目标：覆盖 CRUD/执行 happy-path 与常见错误路径（TRN 校验、关联不存在、输入不合法）。
- 变更点：
  - 在 `src/server/handlers/tests.rs` 基于 `axum::Router` + `sqlite::memory:` 编写单测；可通过 `OpenActService::from_env()` 或注入 `StorageService::new(DatabaseManager::new("sqlite::memory:"))`。
- 验收：`cargo test` 全绿，断言错误 code/状态码一致。

#### 4) Retry-After 支持完善（中优先级）
- 目标：支持 `Retry-After` 秒与 HTTP-date 两种格式，且与策略上限取最小值。
- 变更点：
  - `src/executor/http_executor.rs::parse_retry_after` 增强解析；`calculate_delay` 保持不超过 `max_delay_ms`。
- 验收：对 429/503 mock 响应分别含秒值/HTTP-date 时，实际延迟符合预期并受上限约束。

#### 5) Prometheus 指标启用（按需，低风险）
- 目标：启用 `metrics` feature 时注册 Prometheus recorder；默认继续 Noop。
- 变更点：
  - `src/observability/metrics.rs` 在 `cfg(feature = "metrics")` 下初始化 `metrics_exporter_prometheus` 并保存 handle。
  - `src/observability/endpoints.rs::metrics_endpoint` 返回 exporter 文本（已存在占位分支）。
- 验收：`cargo run --features "server,metrics"` 后 `GET /metrics` 有 Prometheus 文本输出。

#### 6) 安全与日志脱敏（中优先级，可与 Upsert DTO 并行）
- 目标：避免日志泄露 `client_secret/refresh_token/access_token`。
- 变更点：
  - 巡检 `src/executor/http_executor.rs`、`src/oauth/runtime.rs`、`src/observability/logging.rs` 的 tracing 输出，改为布尔/长度提示，不打印明文。
- 验收：grep 不出现敏感值；OAuth 流程日志仅显示有无/是否更新等元信息。

#### 7) 示例与文档对齐（并行）
- 目标：示例脚本与当前服务端契约保持一致，补充接口文档草案。
- 变更点：
  - 校正 `examples/*.sh` 请求体（移除时间戳字段等）。
  - 在 `docs/` 增加 `openapi.yaml`（覆盖 /api/v1/{connections,tasks,execute,system}）。
- 验收：README/脚本一键跑通；接口与 OpenAPI 一致。

---

### 中期规划（3–6 周）

#### A) 企业级能力
- 智能重试（尊重 Retry-After、指数回退上限）：扩展 `RetryPolicy` 与 `HttpExecutor` 路径。
- 限流（Token Bucket，作用域 `(connection_trn, host)`）：独立中间层或在 `HttpExecutor` 前应用。
- 熔断（以 `task_trn` 为 key，半开自恢复）：进程内状态，后续可选持久化。
- 幂等/去重（可选，按 Header/Idempotency-Key）：扩展策略与注入逻辑。

#### B) 扩展能力
- 认证：AWS SigV4、JWT 签名；在 `executor/auth_injector.rs` 增加注入器。
- 响应：分片/流式与二进制落盘（配合 `ResponsePolicy.binary_sink_trn`）。
- 分页：`PaginationConfig` 与迭代器；由执行层提供简单上限与翻页钩子。

#### C) 性能优化
- 连接池统计导出到 API（`client_pool::get_stats` 已有，可拓展指标与端点）。
- 内存与并发参数调优（TTL、池容量、队列深度、Tokio worker 观测）。

---

### 风险与缓解
- OAuth 注入路径一致性：保持 OAuth2 仅在 `HttpExecutor::inject_authentication` 内处理，`auth_injector` 的 OAuth 分支继续占位，避免路径分裂。
- 时间戳来源：先由 handler 注入 `created_at/updated_at`，不改动迁移与仓储结构，降低风险。
- 指标后端：默认 Noop，仅 `--features metrics` 生效，避免对现用户引入依赖。

---

### 验收清单（DoD）
- CLI 本地 vs `--server` 模式对同一 Task 输出一致。
- 所有 CRUD/执行端点错误返回统一 `ApiError` 且状态码映射一致。
- Upsert 不带元数据字段也能创建/更新；返回含服务端时间戳。
- 429/503 含 `Retry-After` 时，重试延迟符合预期且受上限约束。
- `--features metrics` 启用时 `/metrics` 暴露 Prometheus 文本。
- `cargo test` 全绿；`examples/*.sh` 与 README 流程跑通。

---

### 变更参考（文件级指引）
- 接口 & 错误：`src/interface/{dto.rs,error.rs}`
- 处理器：`src/server/handlers/{connections.rs,tasks.rs,execute.rs,system.rs}`、`src/server/router.rs`
- 执行器：`src/executor/{http_executor.rs,parameter_merger.rs}`
- OAuth 运行时：`src/oauth/runtime.rs`
- 可观测性：`src/observability/{metrics.rs,endpoints.rs,logging.rs}`
- 存储与缓存：`src/store/{service.rs,database.rs,*_repository.rs}`
- CLI：`src/cli/mod.rs`

---

### 优化后执行顺序
- **Day 1**：错误模型统一（基础设施，阻塞后续）
- **Day 2**：Upsert DTO 收敛 + 日志脱敏（可并行，风险低）
- **Day 3**：HTTP 处理器单测（验证前面改动）
- **Day 4**：Retry-After 增强（独立功能）
- **Day 5**：Prometheus + 示例校对（收尾工作）

### 🚨 开始前准备
- [ ] 当前分支打 tag（便于回滚）
- [ ] 确保开发环境独立
- [ ] 检查依赖版本兼容性（Prometheus 等）

### ⚠️ 回滚预案
- 如果错误模型影响现有客户端 → 回退到内联 JSON，保留 ApiError 结构
- 如果指标系统有问题 → 通过环境变量快速禁用 metrics feature

> 本计划严格基于当前代码实现制定，落点明确、风险可控，可直接作为 Sprint 执行清单。


