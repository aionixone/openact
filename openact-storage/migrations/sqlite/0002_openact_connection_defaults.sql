-- Add connection-level default parameters
ALTER TABLE openact_connections ADD COLUMN default_headers_json TEXT;
ALTER TABLE openact_connections ADD COLUMN default_query_params_json TEXT;
ALTER TABLE openact_connections ADD COLUMN default_body_json TEXT;

