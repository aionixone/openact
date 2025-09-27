-- Add MCP support fields to actions table
-- This migration adds support for MCP (Multi-Cloud Protocol) functionality

-- Add MCP-related columns to actions table
ALTER TABLE actions ADD COLUMN mcp_enabled BOOLEAN DEFAULT 0 NOT NULL;
ALTER TABLE actions ADD COLUMN mcp_overrides_json TEXT;

-- Create index for MCP-enabled actions for efficient querying
CREATE INDEX idx_actions_mcp_enabled ON actions(mcp_enabled) WHERE mcp_enabled = 1;

-- Update existing actions to have MCP disabled by default (already handled by DEFAULT 0)
-- No data migration needed as we're adding new optional columns
