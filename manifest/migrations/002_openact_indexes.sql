-- OpenAct Database Indexes
-- 优化查询性能

-- Actions 表索引
CREATE INDEX IF NOT EXISTS idx_actions_tenant ON actions(tenant);
CREATE INDEX IF NOT EXISTS idx_actions_provider ON actions(provider);
CREATE INDEX IF NOT EXISTS idx_actions_tenant_provider ON actions(tenant, provider);
CREATE INDEX IF NOT EXISTS idx_actions_name ON actions(name);
CREATE INDEX IF NOT EXISTS idx_actions_active ON actions(is_active);
CREATE INDEX IF NOT EXISTS idx_actions_created_at ON actions(created_at);

-- Action Executions 表索引
CREATE INDEX IF NOT EXISTS idx_executions_action_trn ON action_executions(action_trn);
CREATE INDEX IF NOT EXISTS idx_executions_tenant ON action_executions(tenant);
CREATE INDEX IF NOT EXISTS idx_executions_status ON action_executions(status);
CREATE INDEX IF NOT EXISTS idx_executions_created_at ON action_executions(created_at);
CREATE INDEX IF NOT EXISTS idx_executions_completed_at ON action_executions(completed_at);
CREATE INDEX IF NOT EXISTS idx_executions_tenant_status ON action_executions(tenant, status);

-- Action Tests 表索引
CREATE INDEX IF NOT EXISTS idx_tests_action_trn ON action_tests(action_trn);
CREATE INDEX IF NOT EXISTS idx_tests_test_name ON action_tests(test_name);
CREATE INDEX IF NOT EXISTS idx_tests_test_type ON action_tests(test_type);
CREATE INDEX IF NOT EXISTS idx_tests_active ON action_tests(is_active);

-- Action Test Results 表索引
CREATE INDEX IF NOT EXISTS idx_test_results_test_id ON action_test_results(test_id);
CREATE INDEX IF NOT EXISTS idx_test_results_execution_id ON action_test_results(execution_id);
CREATE INDEX IF NOT EXISTS idx_test_results_status ON action_test_results(status);
CREATE INDEX IF NOT EXISTS idx_test_results_created_at ON action_test_results(created_at);

-- Action Metrics 表索引
CREATE INDEX IF NOT EXISTS idx_metrics_action_trn ON action_metrics(action_trn);
CREATE INDEX IF NOT EXISTS idx_metrics_tenant ON action_metrics(tenant);
CREATE INDEX IF NOT EXISTS idx_metrics_type ON action_metrics(metric_type);
CREATE INDEX IF NOT EXISTS idx_metrics_timestamp ON action_metrics(timestamp);
CREATE INDEX IF NOT EXISTS idx_metrics_tenant_type ON action_metrics(tenant, metric_type);

-- Action Configurations 表索引
CREATE INDEX IF NOT EXISTS idx_configs_action_trn ON action_configurations(action_trn);
CREATE INDEX IF NOT EXISTS idx_configs_key ON action_configurations(config_key);
CREATE INDEX IF NOT EXISTS idx_configs_active ON action_configurations(is_active);
