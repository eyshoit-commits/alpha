# Repository Listing (concise)

```text
.
├── AGENTS.md
├── Cargo.lock
├── Cargo.toml
├── PROMPT.md
├── Progress.md
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
│   │   └── src/{auth.rs,main.rs}
│   └── cave-kernel/
│       ├── Cargo.toml
│       └── src/{isolation.rs,lib.rs}
├── docs/
│   ├── architecture.md
│   ├── env.md
│   ├── FEATURE_ORIGINS.md
│   ├── Progress.md
│   ├── roadmap.md
│   ├── api.md
│   ├── cli.md
│   ├── deployment.md
│   ├── governance.md
│   ├── operations.md
│   ├── security.md
│   ├── testing.md
│   └── compatibility.md
├── schema/
│   └── cave.schema.json
├── web/
│   └── admin/
│       ├── README.md
│       └── package.json
├── .codex/
│   └── codex_config.toml
└── .github/workflows/
    └── ci.yml
```

> Vollständige Listings können bei Bedarf mit `find . -print | sort` erzeugt werden (Achtung: erzeugt sehr lange Ausgaben).
