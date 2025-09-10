-- OpenAct Database Schema
-- 支持 Action 管理、执行记录、测试和指标

-- Actions 表：存储 Action 定义
CREATE TABLE IF NOT EXISTS actions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trn TEXT UNIQUE NOT NULL,                    -- Action TRN: trn:openact:tenant:action/name:provider/provider
    tenant TEXT NOT NULL,                        -- 租户标识
    name TEXT NOT NULL,                          -- Action 名称
    provider TEXT NOT NULL,                      -- 提供商
    openapi_spec TEXT NOT NULL,                  -- 完整的 OpenAPI 规范 (JSON)
    extensions TEXT,                             -- x-* 扩展字段 (JSON)
    auth_flow TEXT,                              -- 认证流程配置 (JSON)
    metadata TEXT,                               -- 额外元数据 (JSON)
    is_active BOOLEAN DEFAULT 1,                 -- 是否激活
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    -- 索引
    UNIQUE(tenant, name, provider)
);

-- Action Executions 表：存储执行记录
CREATE TABLE IF NOT EXISTS action_executions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    execution_trn TEXT UNIQUE NOT NULL,          -- 执行 TRN: trn:stepflow:tenant:execution:action-execution:execution-id
    action_trn TEXT NOT NULL,                    -- 关联的 Action TRN
    tenant TEXT NOT NULL,                        -- 租户标识
    input_data TEXT,                             -- 输入数据 (JSON)
    output_data TEXT,                            -- 输出数据 (JSON)
    status TEXT NOT NULL DEFAULT 'pending',      -- 状态: pending, running, completed, failed
    status_code INTEGER,                         -- HTTP 状态码
    error_message TEXT,                          -- 错误信息
    duration_ms INTEGER,                         -- 执行时长 (毫秒)
    retry_count INTEGER DEFAULT 0,              -- 重试次数
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    completed_at DATETIME,
    
    -- 外键约束
    FOREIGN KEY (action_trn) REFERENCES actions(trn) ON DELETE CASCADE
);

-- Action Tests 表：存储测试用例
CREATE TABLE IF NOT EXISTS action_tests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action_trn TEXT NOT NULL,                    -- 关联的 Action TRN
    test_name TEXT NOT NULL,                     -- 测试名称
    input_data TEXT NOT NULL,                    -- 测试输入 (JSON)
    expected_output TEXT,                        -- 期望输出 (JSON)
    test_type TEXT DEFAULT 'contract',           -- 测试类型: contract, integration, unit
    is_active BOOLEAN DEFAULT 1,                 -- 是否激活
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    -- 外键约束
    FOREIGN KEY (action_trn) REFERENCES actions(trn) ON DELETE CASCADE,
    UNIQUE(action_trn, test_name)
);

-- Action Test Results 表：存储测试结果
CREATE TABLE IF NOT EXISTS action_test_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_id INTEGER NOT NULL,                    -- 关联的测试 ID
    execution_id INTEGER,                        -- 关联的执行 ID
    status TEXT NOT NULL,                        -- 测试状态: passed, failed, skipped
    actual_output TEXT,                          -- 实际输出 (JSON)
    diff_data TEXT,                              -- 差异数据 (JSON)
    duration_ms INTEGER,                         -- 测试时长 (毫秒)
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    -- 外键约束
    FOREIGN KEY (test_id) REFERENCES action_tests(id) ON DELETE CASCADE,
    FOREIGN KEY (execution_id) REFERENCES action_executions(id) ON DELETE SET NULL
);

-- Action Metrics 表：存储性能指标
CREATE TABLE IF NOT EXISTS action_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action_trn TEXT NOT NULL,                    -- 关联的 Action TRN
    tenant TEXT NOT NULL,                        -- 租户标识
    metric_type TEXT NOT NULL,                   -- 指标类型: latency, throughput, error_rate, success_rate
    metric_value REAL NOT NULL,                  -- 指标值
    metric_unit TEXT,                            -- 指标单位: ms, req/s, %, etc.
    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    -- 外键约束
    FOREIGN KEY (action_trn) REFERENCES actions(trn) ON DELETE CASCADE
);

-- Action Configurations 表：存储配置设置
CREATE TABLE IF NOT EXISTS action_configurations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action_trn TEXT NOT NULL,                    -- 关联的 Action TRN
    config_key TEXT NOT NULL,                    -- 配置键
    config_value TEXT NOT NULL,                  -- 配置值
    config_type TEXT DEFAULT 'string',           -- 配置类型: string, number, boolean, json
    is_active BOOLEAN DEFAULT 1,                 -- 是否激活
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    
    -- 外键约束
    FOREIGN KEY (action_trn) REFERENCES actions(trn) ON DELETE CASCADE,
    UNIQUE(action_trn, config_key)
);
