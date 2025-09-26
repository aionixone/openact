-- Add retry_policy fields to connections and tasks tables
-- Adds support for configurable retry policies

-- Add retry_policy_json to connections table
ALTER TABLE connections ADD COLUMN retry_policy_json TEXT;

-- Add retry_policy_json to tasks table  
ALTER TABLE tasks ADD COLUMN retry_policy_json TEXT;
