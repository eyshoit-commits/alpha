-- Seed default namespaces for development and tests (Postgres).
INSERT INTO namespaces (id, code, display_name, created_at, updated_at)
VALUES
    (
        '11111111-1111-1111-1111-111111111111',
        'namespace:alpha',
        'Alpha Namespace',
        '2025-01-01T00:00:00Z',
        '2025-01-01T00:00:00Z'
    ),
    (
        '22222222-2222-2222-2222-222222222222',
        'namespace:beta',
        'Beta Namespace',
        '2025-01-01T00:00:00Z',
        '2025-01-01T00:00:00Z'
    )
ON CONFLICT (id) DO NOTHING;
