-- 初始化 OpenAct 数据库表结构

-- Actions 表：存储 Action 定义
CREATE TABLE IF NOT EXISTS actions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trn TEXT UNIQUE NOT NULL,
    tenant TEXT NOT NULL,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    openapi_spec TEXT NOT NULL,
    extensions TEXT,
    auth_flow TEXT,
    metadata TEXT,
    is_active BOOLEAN NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Action Executions 表：存储执行记录
CREATE TABLE IF NOT EXISTS action_executions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    execution_trn TEXT UNIQUE NOT NULL,
    action_trn TEXT NOT NULL,
    tenant TEXT NOT NULL,
    input_data TEXT,
    output_data TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    status_code INTEGER,
    error_message TEXT,
    duration_ms INTEGER,
    retry_count INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at DATETIME
);

-- Action Tests 表：存储测试用例
CREATE TABLE IF NOT EXISTS action_tests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action_trn TEXT NOT NULL,
    test_name TEXT NOT NULL,
    input_data TEXT NOT NULL,
    expected_output TEXT,
    test_type TEXT DEFAULT 'contract',
    is_active BOOLEAN NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Action Test Results 表：存储测试结果
CREATE TABLE IF NOT EXISTS action_test_results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    test_id INTEGER NOT NULL,
    execution_id INTEGER,
    status TEXT NOT NULL,
    actual_output TEXT,
    diff_data TEXT,
    duration_ms INTEGER,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Action Metrics 表：存储性能指标
CREATE TABLE IF NOT EXISTS action_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action_trn TEXT NOT NULL,
    tenant TEXT NOT NULL,
    metric_type TEXT NOT NULL,
    metric_value REAL NOT NULL,
    metric_unit TEXT,
    timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Action Configurations 表：存储配置设置
CREATE TABLE IF NOT EXISTS action_configurations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    action_trn TEXT NOT NULL,
    config_key TEXT NOT NULL,
    config_value TEXT NOT NULL,
    config_type TEXT DEFAULT 'string',
    is_active BOOLEAN NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
