# OpenAct 路线图（重构版）

本文件定义近期目标、分层边界、阶段交付与任务清单，统一指导 CLI 与 HTTP API 的实现与测试。

## 1. 目标与范围
- 形成“最小可用闭环”：Connections/Tasks CRUD + Task 执行（Executor）对外暴露 HTTP API，CLI 与 HTTP API 行为一致。
- 保持 Authflow 专注“复杂认证编排”（AC/CC、PKCE、回调/暂停/恢复、刷新），不承载通用业务 API。

## 2. 分层边界（关键共识）
- **接口层（Interface）**：CLI、HTTP API（共享 DTO/错误模型）。
- **应用层（Application）**：OpenActService（门面），封装 `StorageService + Executor`，统一 TRN 校验、覆盖参数归一化、错误与脱敏。
- **领域/基础层（Domain/Infra）**：`StorageService`（CRUD、统计、清理）、`Executor`（参数合并、认证注入、HTTP 调用）。
- **编排层（Authflow）**：仅处理复杂认证；与接口层并列，路由空间隔离。

## 3. 里程碑（Milestones）
### Phase 1：核心 HTTP API（最小闭环）
- 交付：
  - Connections/Tasks CRUD（/api/v1 前缀）
  - Task 执行端点（POST /api/v1/tasks/{trn}/execute）
  - 统一 ApiError 与 JSON 响应
  - 处理器单测（connections、tasks、execute）
- 退出标准：
  - CRUD + 执行可用；错误与日志脱敏；与 CLI 本地模式结果一致

### Phase 2：CLI 远程模式与一致性
- 交付：
  - CLI `--server` 模式；本地/远程共用 DTO/错误模型
  - CLI CRUD/执行通过 HTTP API 路由（保留直连模式）
  - E2E：CLI ⇄ API ⇄ Storage/Executor 一致性
- 退出标准：
  - 相同输入下，本地/远程输出一致；核心路径覆盖测试通过

### Phase 3：系统与可观测性
- 交付：
  - /api/v1/system/stats、/api/v1/system/cleanup
  - Client Pool 指标导出（API/CLI）
  - 文档与样例完善

### Phase 4：企业级能力（可选）
- 交付：
  - 重试（尊重 Retry-After）、限流、熔断、TLS 文件路径支持
  - 策略默认关闭，可按需启用

## 4. 任务清单（按阶段分解）
### Phase 1（高优先级）
- [ ] 建立应用层 `OpenActService` 门面（封装 StorageService/Executor）
- [ ] 统一 DTO/错误：`src/interface/{dto.rs,error.rs}`
- [ ] HTTP API：Connections CRUD（GET/POST/GET{id}/PUT{id}/DELETE{id}）
- [ ] HTTP API：Tasks CRUD（GET/POST/GET{id}/PUT{id}/DELETE{id}）
- [ ] HTTP API：Task 执行（POST /api/v1/tasks/{trn}/execute）
- [ ] 路由接入：`core_api_router` 与 `authflow_router` 合并（前缀隔离）
- [ ] 处理器单元测试（connections、tasks、execute）

### Phase 2（中优先级）
- [ ] CLI 增加 `--server` 模式（同一 DTO/错误模型）
- [ ] CLI CRUD/执行走 HTTP API（保留直连模式）
- [ ] E2E：本地 vs 远程模式一致性测试

### Phase 3（中优先级）
- [ ] /api/v1/system/stats（storage + caches + client_pool）
- [ ] /api/v1/system/cleanup（清理过期 auth 记录）
- [ ] 文档：端点说明、错误码、示例脚本

### Phase 4（低优先级）
- [ ] Executor 重试策略（指数回退、尊重 Retry-After、幂等安全）
- [ ] 每连接/主机限流（Rate Limiting）
- [ ] 熔断器（Circuit Breaker）
- [ ] Client Pool 指标导出到 API/CLI
- [ ] Connection 支持 TLS 证书/私钥文件路径加载

## 5. 测试策略
- 单测：HTTP 处理器（参数校验/错误/成功）、OpenActService 门面
- 集成：CLI ⇄ API ⇄ Storage/Executor（本地/远程一致性）
- 回归：Authflow AC/CC/刷新不中断；回调/暂停/恢复链路不受影响

## 6. 风险与缓解
- 兼容性：新 API 不影响 Authflow；路由与代码目录隔离
- 一致性：CLI/HTTP 共用 DTO/错误与 TRN 校验
- 脱敏：错误/日志对 token/密钥统一脱敏
- 迁移：SQLx 迁移幂等，老库平滑升级
- 配置：统一使用 `openact_STORE`、`OPENACT_DATABASE_URL`
- 策略：保留/禁用头策略在 API/Executor 一致

## 7. 非目标（本阶段不做）
- 复杂编排在 Authflow 中执行通用业务 Task（保持分层）
- 全量安全鉴权与配额系统（留待后续）

## 8. 参考（已具备能力）
- 存储与迁移：SQLite + SQLx Migrate（connections/auth_connections/auth_connection_history/tasks）
- OAuth2：AC/CC、刷新、审计；回调暂停/恢复；CLI/脚本验证
- 执行器：参数合并（ConnectionWins）、认证注入、Client Pool、响应策略
- CLI：执行与 CRUD 框架，待接入 HTTP API/远程模式

## 9. 接口契约与规范（必须项）

### 9.1 DTO Schema（请求/响应）
- Connections
  - Create/Update Request = `ConnectionConfig`（敏感字段按现有加密策略存储；响应脱敏）
  - Get/List Response = `ConnectionConfig[]`（响应脱敏）
- Tasks
  - Create/Update Request = `TaskConfig`
  - Get/List Response = `TaskConfig[]`
- Execute
  - Request:
    ```json
    {
      "overrides": {
        "method": "GET|POST|...",
        "endpoint": "https://...",
        "headers": {"Key": ["Value"]},
        "query": {"k": ["v"]},
        "body": {}
      },
      "output": "status-only|headers-only|body-only|full" // 可选，默认 full
    }
    ```
  - Response:
    ```json
    {
      "status": 200,
      "headers": {"content-type": "application/json"},
      "body": {"...": "..."}
    }
    ```

### 9.2 统一错误模型（ApiError）
- 结构：`{ code: string, message: string, details?: object }`
- 分类与建议 code：
  - 验证错误：`validation.invalid_input`, `validation.trn_mismatch`
  - 资源不存在：`not_found.connection`, `not_found.task`
  - 冲突/约束：`conflict.duplicate`
  - 内部错误：`internal.storage_error`, `internal.execution_failed`
- HTTP 映射：400/404/409/500

### 9.3 应用层门面（OpenActService）方法
- Connections：`upsert/get/list/delete/count`
- Tasks：`upsert/get/list/delete/count`
- Execute：`execute_task(task_trn, overrides) -> ExecutionResult`
- System：`stats()/cleanup()`
- Config：`import(conns, tasks)/export() -> (conns, tasks)`
- 说明：内部统一使用 `StorageService + Executor`；对外统一错误与脱敏。

### 9.4 Overrides 合并规则
- Overrides 作用在 Task 层（只修改 Task 临时视图），保持“ConnectionWins”总策略：
  - 合并顺序：`Task (含 overrides) → Connection`，冲突时以 Connection 为准
  - 目的：允许临时改动，但不破坏连接侧的强制策略（安全/合规头）

### 9.5 Phase 1 实施顺序（依赖）
1) 定义 DTO 与 ApiError（本节契约）
2) 实现 `OpenActService`（封装存储与执行）
3) 实现 HTTP Handlers（仅做反序列化/调用/序列化）
4) 合并路由（核心 API 与 Authflow 路由隔离）
5) 单元测试（handlers + service）

### 9.6 测试用例清单
- Handlers：
  - 成功（200/201/204）、校验失败（400）、不存在（404）、冲突（409）、内部错误（500）
  - 列表分页：`limit/offset` 边界与默认值
- Execute：
  - overrides 生效（method/endpoint/headers/query/body）
  - ResponsePolicy 二进制/超限路径
- 一致性：
  - 固定用例对比 CLI 本地 vs HTTP API 输出一致
- 回归：
  - Authflow AC/CC/刷新 + 回调/暂停/恢复不受影响

### 9.7 非功能规范
- 日志脱敏：token、client_secret、refresh_token、extra_data 中疑似密钥字段
- 环境变量优先级：
  - `db_url`（CLI 明确传入） > `OPENACT_DATABASE_URL` > `openact_SQLITE_URL`
  - `openact_STORE` 缺省为 `memory`，`sqlite` 时必须提供数据库 URL
- 分页默认：`limit=100`，`offset=0`，上限建议 `limit<=1000`
- 响应尺寸控制：按 `ResponsePolicy.max_body_bytes`（默认 8MB）
