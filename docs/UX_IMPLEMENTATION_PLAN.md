## OpenAct - UX 驱动的开发路径（基于共同基座）

本计划围绕“统一基座 OpenActService，CLI 与 HTTP API 仅做适配层”的用户体验目标，分阶段落地，附可验证的交付物与验收标准。

### 0. 现状与目标
- 现状：
  - 已有：StorageService + SQLx 迁移、Connections/Tasks CRUD、Execute、OAuth2（AC/CC）、Authflow 并入 /api/v1/authflow、核心 HTTP API 基线、Retry 脚手架、客户端池。
  - 待补：CLI --server 模式；TRN 校验统一；HttpPolicy（禁用头/多值）；策略配置化（Timeout/Retry/RateLimit/CircuitBreaker）；TLS/代理/mTLS；ResponsePolicy；Secret Store；观测指标。
- 目标：
  - 用户通过 Connection 管认证、Task 管业务；TRN 定位资源；CLI 与 HTTP 行为一致；最少命令即可执行外部 API。

---

### Phase P0（统一共同基座 + 入口一致性）
1) CLI --server 模式（代理 HTTP API，保留本地直连）
   - 交付：所有 CLI 子命令支持 `--server http://...`；默认本地模式。
   - 验收：同一 TRN/输入，CLI 本地 vs server 输出一致（E2E 覆盖）。
2) TRN 解析/校验统一模块
   - 交付：`trn::parse/validate` 提供资源/租户/类型校验，被 CLI/HTTP/Service 共用。
   - 验收：20+ 合法/非法用例；HTTP 统一 `ApiError` 结构。
3) 配置统一与文档
   - 交付：`.env.example`、README 最小指南（DB、日志、加密）；`OpenActService::from_env()` 收敛装配。
   - 验收：复制粘贴可跑通 CRUD + Execute。

---

### Phase P1（稳定性策略基础）
1) HttpPolicy（头/查询 多值与保护）
   - 交付：
     - 头大小写归一化；
     - denylist/reserved（如 authorization）；
     - `multi_value_append_headers` 追加规则；
     - Query 多值合并策略。
   - 验收：单测覆盖合并/覆盖/禁用场景；E2E 断言头行为正确。
2) Timeout/Retry 配置化（Connection/Task 均可设置）
   - 交付：尊重 Retry-After；取最严格策略；默认保持当前 0 重试。
   - 验收：httpmock 触发 429/5xx，按配置退避重试并遵循 Retry-After。

---

### Phase P2（流量治理：限流与熔断）
1) RateLimit（桶/令牌）
   - 交付：每 `(connection_trn, host)` 维度的速率限制，可在 Connection 或 Task 覆盖。
   - 验收：压测脚本触发限流，系统返回 429 并在窗口后恢复。
2) CircuitBreaker（基于 task_trn）
   - 交付：错误阈值打开；恢复超时后半开试探；可配置阈值与窗口。
   - 验收：httpmock 连续 5xx 触发熔断；恢复后自动闭环。

---

### Phase P3（安全与网络）
1) TLS/代理/mTLS 配置
   - 交付：`NetworkConfig { proxy_url, tls { ca_pem, client_cert_pem, client_key_pem, server_name, verify_peer } }`；客户端池按配置缓存。
   - 验收：自签 CA + 双向 TLS 场景通过；代理场景通过。
2) Secret Store + 脱敏
   - 交付：`Credential::Secret(SecretRef)` 接口与内存实现；日志脱敏；CLI `--reveal-secrets`（仅本地）。
   - 验收：密文不落日志；本地调试可显式解密（受控）。

---

### Phase P4（响应策略与二进制）
1) ResponsePolicy
   - 交付：默认 `allow_binary=false`；开启后小体积 bytes 返回，大体积落本地 OSS，返回 `oss_trn` 与摘要。
   - 验收：二进制接口 E2E；阈值分流正确；CLI/HTTP 打印摘要。

---

### 观测与测试台（交叉推进）
1) 指标与 API
   - 交付：/api/v1/system/stats 增加：客户端池（命中/构建/逐出/容量）、缓存（命中/命中率）、执行统计、迁移版本。
   - 验收：操作后指标可预期变化；脚本断言。
2) /test 与 dry-run（Phase 后续）
   - 交付：测试接口/CLI `openact task test <TRN> --trace --reveal-secrets`；不实际请求，仅展示合并/注入过程与将发起的请求。
   - 验收：适配 3 类任务样例，输出可读。

---

### 测试矩阵（最小必测集）
- 单测：
  - Handlers（connections/tasks/execute/system）正常+异常；ApiError 统一。
  - 执行器：ApiKey/Basic 注入；OAuth2 CC/AC 刷新；HttpPolicy；Timeout/Retry（含 Retry-After）。
  - TRN 校验器：合法/非法/跨租户/类型不符。
- 集成：
  - CRUD → Execute 全链路；overrides；stats 变化。
  - 限流/熔断；TLS/mTLS；代理。
- E2E：
  - CLI 本地 vs `--server` 行为一致（同一输入，输出一致）；
  - OAuth2 AC 实战（GitHub 已有脚本）。

---

### 具体落地顺序（两周节奏示意）
Week 1（P0 完成 + P1 启动）
1) CLI `--server` 模式（CRUD/execute/system 全覆）
2) TRN 校验器模块 + 统一 ApiError 接入
3) 文档与示例：.env.example、README 最小起步
4) HttpPolicy：头归一化/denylist/reserved + 多值合并

Week 2（P1 完成 + P2 启动）
5) Timeout/Retry 配置化 + Respect Retry-After（执行器接入）
6) RateLimit（桶）与 CircuitBreaker（task_trn）
7) 指标扩展与脚本断言（客户端池/缓存/执行）
8) TLS/代理/mTLS 配置与 E2E（可与 6 并行）

（后续）P3/P4 与测试台、ResponsePolicy、Secret Store 分批推进。

---

### 验收与交付物清单
- 文档：
  - 本计划 + README 起步指南 + CLI 与 HTTP 使用样例
  - 配置样例（连接/任务）与 E2E 脚本（actions_core / github_oauth）
- 代码：
  - 统一基座：OpenActService；TRN 校验模块；ApiError 统一
  - CLI：本地/`--server` 双路；输出一致化
  - HTTP API：Handlers 接入校验器 + 错误统一
  - 执行器：策略化（Timeout/Retry/HttpPolicy） + 后续 RateLimit/CircuitBreaker/TLS/ResponsePolicy
- 测试：
  - 单测/集成/E2E 脚本矩阵
  - CI（后续）：最小回归集（构建+单测+两个 E2E）


