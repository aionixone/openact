# OpenAct OpenAPI 开发计划

## 📋 项目概述

### 目标
为 OpenAct 现有 API 生成完整的 OpenAPI 3.0 文档，并通过 Swagger UI 提供交互式文档预览。

### 核心原则
- **零破坏性**: 不改动现有 handler/DTO/路由逻辑
- **可选功能**: 默认不启用 `openapi` feature，现有行为完全一致
- **渐进实施**: 分阶段交付，每步可独立验证和回滚
- **类型安全**: 直接使用现有类型，避免重复定义

### 技术边界
- ✅ 允许: 添加注解、新增文件、可选依赖
- ❌ 禁止: 修改 handler 逻辑、改变 API 响应格式、影响 CLI 行为

---

## 🏗️ 技术架构

### 依赖管理
```toml
# Cargo.toml
[features]
default = []
server = ["axum", "tokio", "tower", "tower-http"]
openapi = ["utoipa", "utoipa-swagger-ui"]  # 新增

[dependencies]
# 现有依赖保持不变...
utoipa = { version = "4.2", optional = true, features = ["axum_extras"] }
utoipa-swagger-ui = { version = "5.9", optional = true, features = ["axum"] }
```

### 文件结构
```
src/
├── api/
│   └── openapi.rs           # 🆕 OpenAPI 文档定义
├── server/
│   ├── handlers/            # ✅ 现有，仅添加注解
│   ├── authflow/handlers/   # ✅ 现有，仅添加注解
│   └── router.rs            # ✅ 现有，仅添加 Swagger UI 路由
├── interface/dto.rs         # ✅ 现有，添加 ToSchema 派生
├── models/                  # ✅ 现有，添加 ToSchema 派生
└── ...                      # ✅ 其他目录完全不变
```

### 实现策略
1. **类型注解**: 在现有 DTO/模型上使用 `#[cfg_attr(feature = "openapi", derive(ToSchema))]`
2. **Handler 注解**: 使用 `#[cfg_attr(feature = "openapi", utoipa::path(...))]`
3. **文档集成**: 创建统一的 `ApiDoc` 收集所有注解信息
4. **路由集成**: 在启用 feature 时合并 Swagger UI 路由

---

## 📅 开发里程碑

### M0: 基础骨架 (0.5 天)

**目标**: 建立 OpenAPI 基础设施，确保 feature 开关正常工作

**任务清单**:
- [ ] 更新 `Cargo.toml` 添加 `openapi` feature 和依赖
- [ ] 创建 `src/api/mod.rs` 和 `src/api/openapi.rs`
- [ ] 实现基础 `ApiDoc` 结构 (空的 paths/components)
- [ ] 在 `src/server/router.rs` 中集成 Swagger UI 路由

**验收标准**:
- [ ] `cargo build` (不带 openapi) 行为无变化
- [ ] `cargo build --features openapi` 编译成功
- [ ] `cargo run --features "server,openapi"` 可访问 `/swagger-ui` (空文档)

**关键代码**:
```rust
// src/api/openapi.rs
#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "OpenAct API",
        version = "0.1.0",
        description = "OpenAct - Universal API Integration Platform"
    ),
    servers(
        (url = "http://localhost:8080", description = "Development server")
    ),
    tags(
        (name = "connections", description = "Connection management"),
        (name = "tasks", description = "Task management"),
        // ... 其他 tags
    )
)]
pub struct ApiDoc;
```

### M1: 类型 Schema 注解 (1 天)

**目标**: 为现有 DTO 和模型添加 OpenAPI Schema 支持

**任务清单**:
- [ ] `src/interface/dto.rs`: 添加 ToSchema 派生
  - [ ] `ConnectionUpsertRequest`
  - [ ] `TaskUpsertRequest` 
  - [ ] `ExecuteRequestDto`
  - [ ] `ExecuteResponseDto`
  - [ ] `AdhocExecuteRequestDto`
- [ ] `src/models/connection.rs`: 添加 ToSchema 派生
  - [ ] `AuthorizationType`
  - [ ] `AuthParameters`
  - [ ] `ApiKeyAuthParameters`
  - [ ] `BasicAuthParameters`
  - [ ] `OAuth2Parameters`
- [ ] `src/models/task.rs`: 添加 ToSchema 派生
- [ ] `src/models/common.rs`: 添加 ToSchema 派生
  - [ ] `RetryPolicy`
  - [ ] `TimeoutConfig`
  - [ ] `NetworkConfig`
- [ ] `src/interface/error.rs`: 添加 ToSchema 派生
  - [ ] `ApiError`

**验收标准**:
- [ ] `cargo check` (不带 openapi) 通过
- [ ] `cargo check --features openapi` 通过
- [ ] `ApiDoc::openapi().components` 包含所有定义的 schemas

**关键代码**:
```rust
// src/interface/dto.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(
    example = json!({
        "trn": "trn:openact:tenant:connection/my-conn@v1",
        "name": "My API Connection"
    })
))]
pub struct ConnectionUpsertRequest {
    #[cfg_attr(feature = "openapi", schema(
        example = "trn:openact:tenant:connection/my-conn@v1",
        description = "Unique TRN identifier for the connection"
    ))]
    pub trn: String,
    // ...
}
```

### M2: 核心 API 路径注解 (1 天)

**目标**: 为 Connections 和 Tasks API 添加完整的路径文档

**任务清单**:
- [ ] **Connections API** (`src/server/handlers/connections.rs`):
  - [ ] `list` - GET `/api/v1/connections`
  - [ ] `create` - POST `/api/v1/connections`
  - [ ] `get` - GET `/api/v1/connections/{trn}`
  - [ ] `update` - PUT `/api/v1/connections/{trn}`
  - [ ] `del` - DELETE `/api/v1/connections/{trn}`
  - [ ] `status` - GET `/api/v1/connections/{trn}/status`
  - [ ] `test` - POST `/api/v1/connections/{trn}/test`

- [ ] **Tasks API** (`src/server/handlers/tasks.rs`):
  - [ ] `list` - GET `/api/v1/tasks`
  - [ ] `create` - POST `/api/v1/tasks`
  - [ ] `get` - GET `/api/v1/tasks/{trn}`
  - [ ] `update` - PUT `/api/v1/tasks/{trn}`
  - [ ] `del` - DELETE `/api/v1/tasks/{trn}`

- [ ] 更新 `ApiDoc` 的 `paths` 和 `components`

**验收标准**:
- [ ] `/api-docs/openapi.json` 包含所有路径定义
- [ ] Swagger UI 可正确显示和测试这些端点
- [ ] 用 curl 验证启用/未启用 openapi 的响应格式完全一致

**关键代码**:
```rust
// src/server/handlers/connections.rs
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/connections",
    request_body = crate::interface::dto::ConnectionUpsertRequest,
    responses(
        (status = 201, description = "Connection created successfully", 
         body = crate::models::ConnectionConfig),
        (status = 400, description = "Invalid input", 
         body = crate::interface::error::ApiError),
        (status = 409, description = "Connection already exists", 
         body = crate::interface::error::ApiError)
    ),
    tag = "connections",
    summary = "Create a new connection",
    description = "Creates a new connection with the specified configuration"
))]
pub async fn create(Json(req): Json<ConnectionUpsertRequest>) -> impl IntoResponse {
    // 现有代码完全不变
}
```

### M3: 执行与系统 API 注解 (0.5-1 天)

**目标**: 为执行、连接向导和系统管理 API 添加文档

**任务清单**:
- [ ] **执行 API** (`src/server/handlers/execute.rs`):
  - [ ] `execute` - POST `/api/v1/tasks/{trn}/execute`
  - [ ] `execute_adhoc` - POST `/api/v1/execute/adhoc`

- [ ] **连接向导 API** (`src/server/handlers/connect.rs`):
  - [ ] `connect` - POST `/api/v1/connect`
  - [ ] `connect_ac_resume` - POST `/api/v1/connect/ac/resume`
  - [ ] `connect_ac_status` - GET `/api/v1/connect/ac/status`
  - [ ] `connect_device_code` - POST `/api/v1/connect/device-code`

- [ ] **系统管理 API** (`src/server/handlers/system.rs`):
  - [ ] `health` - GET `/api/v1/system/health`
  - [ ] `stats` - GET `/api/v1/system/stats`
  - [ ] `cleanup` - POST `/api/v1/system/cleanup`

**验收标准**:
- [ ] 所有端点在 Swagger UI 中正确分类显示
- [ ] 连接向导流程的参数和响应格式准确
- [ ] 系统管理端点的权限要求明确标注

### M4: AuthFlow API 注解 (1-1.5 天)

**目标**: 为 AuthFlow 工作流引擎 API 添加完整文档

**任务清单**:
- [ ] **工作流管理** (`src/server/authflow/handlers/workflows.rs`):
  - [ ] `list_workflows` - GET `/api/v1/authflow/workflows`
  - [ ] `create_workflow` - POST `/api/v1/authflow/workflows`
  - [ ] `get_workflow` - GET `/api/v1/authflow/workflows/{id}`
  - [ ] `get_workflow_graph` - GET `/api/v1/authflow/workflows/{id}/graph`
  - [ ] `validate_workflow` - POST `/api/v1/authflow/workflows/{id}/validate`

- [ ] **执行管理** (`src/server/authflow/handlers/executions.rs`):
  - [ ] `list_executions` - GET `/api/v1/authflow/executions`
  - [ ] `start_execution` - POST `/api/v1/authflow/executions`
  - [ ] `get_execution` - GET `/api/v1/authflow/executions/{id}`
  - [ ] `resume_execution` - POST `/api/v1/authflow/executions/{id}/resume`
  - [ ] `cancel_execution` - POST `/api/v1/authflow/executions/{id}/cancel`
  - [ ] `get_execution_trace` - GET `/api/v1/authflow/executions/{id}/trace`

- [ ] **其他 AuthFlow API**:
  - [ ] `health_check` - GET `/api/v1/authflow/health`
  - [ ] `oauth_callback` - GET `/api/v1/authflow/callback`
  - [ ] `websocket_handler` - GET `/api/v1/authflow/ws/executions` (WebSocket)

**验收标准**:
- [ ] AuthFlow API 与 Core API 在文档中清晰区分
- [ ] WebSocket 端点正确标注为协议升级
- [ ] OAuth 回调参数和重定向行为准确描述

**特殊注意**:
```rust
// WebSocket 端点示例
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/authflow/ws/executions",
    responses(
        (status = 101, description = "WebSocket connection established"),
        (status = 400, description = "Invalid WebSocket upgrade request")
    ),
    tag = "authflow-executions",
    summary = "WebSocket for real-time execution updates",
    description = "Establishes a WebSocket connection to receive real-time updates about execution status and progress."
))]
```

### M5: 安全与认证文档 (0.5 天)

**目标**: 完善 API 安全模型和认证文档

**任务清单**:
- [ ] 在 `ApiDoc` 中定义 `security_schemes`:
  - [ ] `api_key`: Header `X-API-Key`
  - [ ] `basic_auth`: HTTP Basic Authentication
  - [ ] `oauth2_cc`: OAuth2 Client Credentials
  - [ ] `oauth2_ac`: OAuth2 Authorization Code

- [ ] 为需要认证的端点添加 `security` 标注

- [ ] 完善文档元信息:
  - [ ] 详细的 API 描述
  - [ ] 联系信息和许可证
  - [ ] 外部文档链接

**验收标准**:
- [ ] Swagger UI 正确显示认证方式
- [ ] 认证要求清晰标注在相关端点
- [ ] API 描述信息完整准确

**关键代码**:
```rust
// src/api/openapi.rs
#[openapi(
    // ... 其他配置
    components(
        // ... schemas
        security_schemes(
            ("api_key", ApiKey(ApiKeyValue(Header("X-API-Key")))),
            ("basic_auth", Basic),
            ("oauth2_cc", OAuth2(
                flows = [ClientCredentials(token_url = "/oauth/token")]
            )),
            ("oauth2_ac", OAuth2(
                flows = [AuthorizationCode(
                    authorization_url = "/oauth/authorize", 
                    token_url = "/oauth/token"
                )]
            ))
        )
    )
)]
```

### M6: 工程化与交付 (0.5 天)

**目标**: 完善工程化支持和文档交付

**任务清单**:
- [ ] **文档生成脚本**:
  - [ ] 创建 CLI 命令生成静态文档文件
  - [ ] 导出 `openapi.json` 和 `openapi.yaml` 到 `docs/` 目录

- [ ] **CI/CD 集成**:
  - [ ] 添加文档构建作业 (可选)
  - [ ] 文档变更检测和通知

- [ ] **使用文档**:
  - [ ] 更新 README 添加 OpenAPI 使用说明
  - [ ] 提供开发者快速上手指南

**验收标准**:
- [ ] 可通过命令行生成和更新文档
- [ ] 文档部署流程清晰可重复
- [ ] 开发者可轻松上手和贡献

**交付物**:
- [ ] `docs/openapi.json` - OpenAPI 规范文件
- [ ] `docs/openapi.yaml` - YAML 格式规范文件  
- [ ] `docs/API_GUIDE.md` - API 使用指南
- [ ] 更新的 `README.md` - 包含 OpenAPI 使用说明

---

## 🧪 验证与质量保证

### 每阶段验证清单
- [ ] **编译验证**:
  - [ ] `cargo check` (不带 openapi) 通过
  - [ ] `cargo check --features openapi` 通过
  - [ ] `cargo test` (不带 openapi) 通过
  - [ ] `cargo test --features openapi` 通过

- [ ] **功能验证**:
  - [ ] CLI 功能完全正常 (不带 openapi)
  - [ ] HTTP API 响应格式无变化 (对比启用前后)
  - [ ] Swagger UI 可正确访问和测试

- [ ] **文档质量**:
  - [ ] 所有端点都有适当的描述和示例
  - [ ] 错误响应格式统一且准确
  - [ ] 认证要求清晰标注

### 回归测试样本
建议建立固定的测试集合，每个里程碑后执行：

```bash
# CLI 回归测试
openact-cli connection list
openact-cli task list  
openact-cli system stats

# HTTP API 回归测试
curl -X GET http://localhost:8080/api/v1/connections
curl -X GET http://localhost:8080/api/v1/tasks
curl -X GET http://localhost:8080/api/v1/system/health
curl -X GET http://localhost:8080/api/v1/authflow/health
```

### 回滚策略
- **Feature 级回滚**: 不启用 `openapi` feature 即可回到原始状态
- **代码级回滚**: 可安全删除 `src/api/` 目录和相关注解
- **依赖级回滚**: 移除 `utoipa` 相关依赖

---

## ⏱️ 时间预算与资源

### 总体时间
- **预计总工期**: 3.5 - 5.5 天
- **关键路径**: M1 (类型注解) → M2 (核心 API) → M4 (AuthFlow)
- **可并行**: M3 与 M5 可与其他任务部分并行

### 里程碑时间分配
| 里程碑 | 预计时间 | 累计时间 | 关键依赖 |
|--------|----------|----------|----------|
| M0: 基础骨架 | 0.5 天 | 0.5 天 | 无 |
| M1: 类型注解 | 1 天 | 1.5 天 | M0 |
| M2: 核心 API | 1 天 | 2.5 天 | M1 |
| M3: 执行系统 API | 0.5-1 天 | 3.5 天 | M1 |
| M4: AuthFlow API | 1-1.5 天 | 5 天 | M1 |
| M5: 安全文档 | 0.5 天 | 5.5 天 | M2-M4 |
| M6: 工程化 | 0.5 天 | 6 天 | M5 |

### 风险与缓解
- **风险**: utoipa 与现有类型不兼容
  - **缓解**: M1 阶段优先验证类型兼容性
- **风险**: AuthFlow API 复杂度超预期  
  - **缓解**: M4 可分多次迭代，先覆盖核心路径
- **风险**: 性能影响
  - **缓解**: 使用 feature gate 确保默认构建无影响

---

## 🎯 成功标准

### 功能性标准
- [ ] **零破坏性**: 默认构建下所有现有功能完全正常
- [ ] **完整性**: 所有 Core API 和 AuthFlow API 都有完整文档
- [ ] **可用性**: Swagger UI 可正常浏览和测试所有端点
- [ ] **准确性**: 文档与实际 API 行为完全一致

### 质量标准  
- [ ] **类型安全**: 编译时验证文档与代码的一致性
- [ ] **维护性**: 新增 API 时可在 5 分钟内完成文档更新
- [ ] **可读性**: 文档描述清晰，示例准确有用
- [ ] **安全性**: 敏感信息在文档中正确脱敏

### 交付标准
- [ ] **在线文档**: `/swagger-ui` 提供完整交互式文档
- [ ] **静态文档**: 可导出 JSON/YAML 格式的 OpenAPI 规范
- [ ] **开发指南**: 为团队提供清晰的使用和维护指南
- [ ] **CI 集成**: 文档构建集成到开发流程中

---

## 📚 参考资源

### 技术文档
- [OpenAPI 3.0 Specification](https://spec.openapis.org/oas/v3.0.3/)
- [utoipa Documentation](https://docs.rs/utoipa/)
- [utoipa-swagger-ui Documentation](https://docs.rs/utoipa-swagger-ui/)

### 最佳实践
- [OpenAPI Best Practices](https://oai.github.io/Documentation/best-practices.html)
- [API Design Guidelines](https://apiguide.readthedocs.io/)

### 项目资源
- [OpenAct GitHub Repository](https://github.com/aionixone/openact)
- [Current API Documentation](./API_REFERENCE.md) (如果存在)

---

**最后更新**: 2025-09-23
**文档版本**: v1.0
**负责人**: OpenAct Team
