# Neon MCP Server Integration Guide

## Overview

The **Neon MCP Server** enables natural language interaction with Neon Postgres databases through the Model Context Protocol (MCP). This integration allows you to manage the `bkg-db` database using conversational commands.

## What is Neon MCP Server?

Neon MCP Server acts as a bridge between natural language requests and the Neon API, enabling you to:

- **Create and manage databases** using natural language
- **Run SQL queries** without writing code
- **Perform database migrations** conversationally
- **Manage branches and projects** intuitively
- **Execute schema changes** through dialogue

## Prerequisites

- Node.js >= v18.0.0
- Neon account (https://console.neon.tech/signup)
- Neon API key (https://neon.tech/docs/manage/api-keys)
- MCP-compatible client (Claude Desktop, Cursor, etc.)

## Setup Options

### Option 1: Remote Hosted MCP Server (Recommended)

Connect to Neon's managed MCP server using OAuth:

1. No local installation required
2. Automatic updates
3. OAuth authentication
4. Zero configuration

**Configuration for Claude Desktop:**
```json
{
  "mcpServers": {
    "neon": {
      "url": "https://mcp.neon.tech/mcp"
    }
  }
}
```

### Option 2: Local MCP Server

Run the server locally with your Neon API key:

```bash
# Install globally
npm install -g @neondatabase/mcp-server-neon

# Or use npx
npx @neondatabase/mcp-server-neon
```

**Configuration for Claude Desktop:**
```json
{
  "mcpServers": {
    "neon": {
      "command": "npx",
      "args": ["-y", "@neondatabase/mcp-server-neon"],
      "env": {
        "NEON_API_KEY": "your-neon-api-key-here"
      }
    }
  }
}
```

## Integration with BKG Platform

### Current Setup

The BKG platform uses:
- **bkg-db**: Rust crate with SQLx for Postgres
- **PostgreSQL**: Primary database
- **Migrations**: Via SQLx migrations

### Neon MCP Benefits for BKG

1. **Natural Language Queries**: "Show me all API keys for namespace 'default'"
2. **Schema Changes**: "Add a 'created_by' column to the sandboxes table"
3. **Data Analysis**: "Give me a summary of sandbox execution statistics"
4. **Branch Management**: "Create a development branch for testing new features"
5. **Migration Testing**: "Run migration v3 on a test branch"

## Example Use Cases

### Database Management

```
User: Create a new table called 'blockchain_transactions' with columns:
      - id (uuid, primary key)
      - from_address (text)
      - to_address (text)
      - amount (decimal)
      - timestamp (timestamp)
      - status (text)
```

### Query Execution

```
User: Show me the top 10 most active users by sandbox creation count
```

### Migrations

```
User: I want to add an index on the 'api_keys' table for the 'namespace' column
      to improve query performance
```

### Branch Testing

```
User: Create a branch called 'feature-blockchain' from the main database
      so I can test the new blockchain tables
```

## Configuration for BKG

### Environment Variables

Add to `.env`:
```bash
# Neon Database Configuration
NEON_API_KEY=neon_api_xxxxxxxxxxxxx
NEON_PROJECT_ID=your-project-id
NEON_DATABASE_URL=postgresql://user:pass@host/dbname
```

### Cursor/Claude Integration

Add to MCP settings:
```json
{
  "mcpServers": {
    "neon-bkg": {
      "command": "npx",
      "args": ["-y", "@neondatabase/mcp-server-neon"],
      "env": {
        "NEON_API_KEY": "${NEON_API_KEY}",
        "NEON_PROJECT_ID": "bkg-production"
      }
    }
  }
}
```

## Security Considerations

⚠️ **IMPORTANT**: The Neon MCP Server has powerful database capabilities.

### Best Practices:

1. **Review all actions** before execution
2. **Use read-only keys** for non-admin users
3. **Enable audit logging** for all database operations
4. **Restrict access** to authorized users only
5. **Use branches** for testing schema changes
6. **Never use in production** without proper safeguards

### Recommended Setup:

```bash
# Development environment only
NEON_API_KEY=<dev-api-key>

# Use branches for testing
NEON_BRANCH=development

# Enable audit logs
NEON_AUDIT_LOG=enabled
```

## Features Available

### Database Operations

- ✅ Create/delete projects
- ✅ Create/delete databases
- ✅ Create/manage branches
- ✅ Execute SQL queries
- ✅ Run migrations
- ✅ View table schemas
- ✅ Analyze data

### Branch Management

- ✅ Create feature branches
- ✅ Test migrations safely
- ✅ Point-in-time recovery
- ✅ Branch comparison
- ✅ Merge changes

### Query Capabilities

- ✅ SELECT queries
- ✅ Aggregations
- ✅ Joins across tables
- ✅ Data exports
- ✅ Statistics generation

## Integration Steps

### 1. Install Neon MCP Server

```bash
npm install -g @neondatabase/mcp-server-neon
```

### 2. Generate Neon API Key

1. Go to https://console.neon.tech
2. Navigate to Account Settings → API Keys
3. Generate new API key
4. Save securely

### 3. Configure MCP Client

Add to your MCP client configuration (Claude Desktop, Cursor, etc.)

### 4. Test Connection

```
User: List all my Neon projects
```

### 5. Integrate with BKG

Update `bkg-db` configuration to use Neon connection string:

```rust
// In bkg-db/src/lib.rs
pub async fn connect_neon() -> Result<Pool<Postgres>> {
    let neon_url = std::env::var("NEON_DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&neon_url)
        .await?;
    Ok(pool)
}
```

## Natural Language Examples

### Schema Design

```
User: Design a schema for storing blockchain wallet transactions with 
      address validation and transaction history
```

### Data Analysis

```
User: Analyze the sandbox usage patterns over the last 30 days and 
      identify the most frequently used runtimes
```

### Performance Optimization

```
User: Identify slow queries in the database and suggest indexes to 
      improve performance
```

### Migration Planning

```
User: Plan a migration to add JWT token support to the api_keys table 
      without downtime
```

## Resources

- **Neon MCP GitHub**: https://github.com/neondatabase/mcp-server-neon
- **Neon Documentation**: https://neon.tech/docs
- **MCP Protocol**: https://modelcontextprotocol.io
- **Neon API Reference**: https://api-docs.neon.tech

## Troubleshooting

### Connection Issues

```bash
# Test Neon connection
psql $NEON_DATABASE_URL -c "SELECT version();"
```

### API Key Problems

```bash
# Verify API key
curl https://api.neon.tech/api/v2/projects \
  -H "Authorization: Bearer $NEON_API_KEY"
```

### MCP Server Not Starting

```bash
# Run with debug logs
DEBUG=* npx @neondatabase/mcp-server-neon
```

## Next Steps

1. ✅ Install Neon MCP Server
2. ✅ Generate Neon API key
3. ✅ Configure MCP client
4. ✅ Migrate bkg-db to Neon
5. ✅ Test natural language queries
6. ✅ Integrate with cave-daemon
7. ✅ Set up branch-based development
8. ✅ Enable audit logging

## Conclusion

Integrating Neon MCP Server with the BKG platform enables:

- **Faster development** through natural language database operations
- **Safer migrations** using branch-based testing
- **Better collaboration** with non-technical team members
- **Improved debugging** through conversational queries
- **Enhanced productivity** by eliminating SQL writing for common tasks

Start using Neon MCP today to supercharge your database workflows! 🚀
