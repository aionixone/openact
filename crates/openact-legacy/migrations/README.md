# Database Migrations

This directory contains SQLx database migrations for the OpenAct project.

## Migration Files

- `001_initial_schema.sql` - Initial database schema with all core tables

## Creating New Migrations

To create a new migration:

1. Create a new file with format: `{number}_{description}.sql`
2. Add SQL DDL statements (CREATE TABLE, ALTER TABLE, etc.)
3. Run the application to apply migrations automatically

## Current Schema

The database includes these tables:

- `auth_connections` - OAuth tokens storage
- `auth_connection_history` - OAuth tokens audit trail  
- `connections` - Connection configurations
- `tasks` - Task configurations
- `_sqlx_migrations` - Migration tracking (auto-created by SQLx)

## Migration Best Practices

- Always use `IF NOT EXISTS` for CREATE statements
- Include appropriate indexes
- Add foreign key constraints where needed
- Use meaningful migration descriptions
- Test migrations with both fresh and existing databases
