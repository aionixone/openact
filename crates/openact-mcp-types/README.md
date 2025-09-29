This crate contains Rust types generated from the Model Context Protocol (MCP) JSON Schema.

How to regenerate:

1) Ensure `schema/<version>/schema.json` and `generate_mcp_types.py` exist in this crate.
2) Run:

   ./generate_mcp_types.py

3) To verify against the checked-in version without writing files:

   ./generate_mcp_types.py --check

By default, the script uses SCHEMA_VERSION set inside the script (currently 2025-06-18).

