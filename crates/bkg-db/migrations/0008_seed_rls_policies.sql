-- Seed default RLS policies for sandbox isolation.
INSERT OR IGNORE INTO rls_policies (
    id,
    table_name,
    policy_name,
    expression,
    created_at,
    updated_at
) VALUES (
    '66666666-6666-6666-6666-666666666666',
    'sandboxes',
    'namespace_scope',
    '{"eq": {"column": "namespace", "claim": "scope"}}',
    '2025-01-01T00:00:00Z',
    '2025-01-01T00:00:00Z'
);
