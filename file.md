# Repository Listing (concise)

```text
.
├── AGENTS.md
├── Cargo.lock
├── Cargo.toml
├── PROMPT.md
├── README.md
├── config/
│   └── sandbox_config.toml
├── crates/
│   ├── bkg-db/
│   │   ├── Cargo.toml
│   │   ├── migrations/0001_init.sql
│   │   └── src/lib.rs
│   ├── cave-daemon/
│   │   ├── Cargo.toml
│   │   └── src/{auth.rs,lib.rs,main.rs,server.rs,bin/export-openapi.rs}
│   └── cave-kernel/
│       ├── Cargo.toml
│       └── src/{isolation.rs,lib.rs}
├── docs/
│   ├── Progress.md
│   ├── architecture.md
│   ├── api.md
│   ├── cli.md
│   ├── compatibility.md
│   ├── deployment.md
│   ├── env.md
│   ├── FEATURE_ORIGINS.md
│   ├── governance.md
│   ├── operations.md
│   ├── roadmap.md
│   ├── security.md
│   └── testing.md
├── schema/
│   └── cave.schema.json
├── web/
│   ├── package.json (workspaces: admin, app)
│   ├── lib/api.ts
│   ├── playwright.config.ts
│   ├── admin/
│   │   ├── package.json
│   │   ├── next.config.mjs
│   │   ├── tsconfig.json
│   │   ├── src/app/(dashboard)/{sandboxes,keys,telemetry}/page.tsx
│   │   ├── src/components/{token-context.tsx,token-form.tsx}
│   │   └── src/app/{layout.tsx,page.tsx,globals.css}
│   ├── app/
│   │   ├── package.json
│   │   ├── next.config.mjs
│   │   ├── tsconfig.json
│   │   ├── src/app/(dashboard)/history/page.tsx
│   │   ├── src/components/{token-context.tsx,token-form.tsx}
│   │   └── src/app/{layout.tsx,page.tsx,globals.css}
│   └── tests/e2e/
│       ├── admin-sandboxes.spec.ts
│       └── namespace-dashboard.spec.ts
├── .github/workflows/
│   └── ci.yml
└── file.md
```

> Vollständige Listings können bei Bedarf mit `find . -print | sort` erzeugt werden (Achtung: erzeugt sehr lange Ausgaben).
