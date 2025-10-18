# Repository Guidelines

Version 2.2 · Last updated 2025-10-18 · Maintainer: @bkgoder

---

## 1. How to Use This Guide
- Load `README.md` (v1.8.2), `PROMPT.md`, this file, `docs/Progress.md`, and `docs/roadmap.md` at the start of every session.  
- Treat these sources as a single rule set; update them together if processes change.  
- Record deviations, decisions, and owners in `docs/Progress.md`.

---

## 2. Repository Snapshot
```
.
├── Cargo.toml        # workspace (bkg-db, cave-kernel, cave-daemon)
├── crates/           # Rust code
├── docs/             # architecture, env, roadmap, progress, feature origins
├── config/           # sandbox_config.toml (limits, security)
├── schema/           # cave.schema.json (ajv target)
├── PROMPT.md         # day-to-day briefing
├── AGENTS.md         # this guide
└── README.md         # binding system prompt
```
Key directories:
- `crates/bkg-db` – persistence + migrations (`migrations/0001_init.sql`).  
- `crates/cave-kernel` – sandbox lifecycle & isolation (`src/lib.rs`, `src/isolation.rs`).  
- `crates/cave-daemon` – REST/MCP service (`src/server.rs`, `src/auth.rs`, `/api/v1/*`, `/healthz`, `/metrics`).
- `.codex/codex_config.toml` – loads `PROMPT.md`, `AGENTS.md`, `docs/Progress.md` for the agent context.

---

## 3. Phase-0 Priority Ladder
1. **Kernel hardening** – Namespaces, seccomp, cgroups v2, FS overlay, lifecycle integration tests, signed audit logs.  
2. **Persistent DB** – Postgres + RLS, migrations, namespace isolation tests.  
3. **Web-UIs** – Admin (models/keys/peers) & user UI (sandbox lifecycle, telemetry, chat) with CI-backed E2E tests.  
→ P2P, distributed inference, marketplace, multi-agent orchestration unlock **only** after 1–3 ship with SBOM/SLSA artefacts.  
Progress evidence belongs in `docs/Progress.md` and release notes.

---

## 4. Role Overview
**Sandbox Coding Agent**  
- Follow README ↔ PROMPT ↔ AGENTS. Use targeted edits (`apply_patch`), no destructive git commands.  
- Run tests (`cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, schema checks) and note results.  
- Update `docs/Progress.md` (status, file:line refs, tests, open questions).  
- Deliver clear hand-off summaries with next steps and risks.

**Admin-Orchestrator**  
- Manage API keys (`/api/v1/auth/keys/*`), rotation webhook, cosign keys, telemetry policy (`CAVE_OTEL_SAMPLING_RATE`).  
- Maintain SBOM/SLSA workflows; approve sandbox limit overrides; keep roadmap/docs aligned.

**Security Agent**  
- Maintain `docs/security.md` (Threat Matrix) and enforce `pytest security/` in CI.  
- Verify webhook signatures, audit integrity, secrets handling, Postgres RLS rollout.

**Docs Agent**  
- Keep `PROMPT.md`, `AGENTS.md`, `docs/Progress.md`, `docs/roadmap.md`, `docs/FEATURE_ORIGINS.md` in sync.  
- Ensure each adaptation has a completed Feature Origin entry (URL, rationale, design, tests, no-copy statement, commit/PR, reviewer).

---

## 5. Standard Workflow
1. **Context** – Read README, PROMPT, AGENTS, Progress, Roadmap.  
2. **Plan** – Create a multi-step plan via the planning tool; update as you work.  
3. **Investigate** – Inspect with non-destructive commands (`rg`, `ls`, `sed`); check `git status -sb`.  
4. **Implement** – Change files with `apply_patch`; respect sandbox limits from `config/sandbox_config.toml`.  
5. **Validate** – Run:  
   - `cargo fmt --all`  
   - `cargo clippy --all-targets --all-features -- -D warnings`  
   - `cargo test` / `cargo test -p <crate>`  
   - `ajv validate -s schema/cave.schema.json -d cave.yaml` when configs change  
6. **Document** – Update `docs/Progress.md` (status, tests, follow-ups) and adjust roadmap/docs if scope changed.  
7. **Report** – Final summary must cover changes, tests, outstanding work, recommended next steps.  
8. **Cleanup** – Stop sandboxes (`sandbox.stop()`), remove temp artefacts, capture blockers.

If a required test cannot run, explicitly state why and propose mitigation.

---

## 6. Responsibilities by Area
- **`crates/bkg-db`** – Provide sqlx-backed persistence, JWT auth, RLS policy stubs; add integration tests (`cargo test -p bkg-db`, `SQLX_OFFLINE=true sqlx migrate run`).  
- **`crates/cave-kernel`** – Enforce limits, emit audit logs, add tracing spans, prepare seccomp/cgroup integration tests.  
- **`crates/cave-daemon`** – Serve lifecycle/auth endpoints, stream logs, expose `/healthz` & `/metrics`, persist keys, implement rotation webhook.  
- **`docs/`** – Maintain architecture, env, roadmap, progress, feature origins. Track inspirations in `docs/FEATURE_ORIGINS.md`.  
- **`config/` & `schema/`** – Keep defaults (CPU 2 vCPU, RAM 1024 MiB, Timeout 120 s, Disk 1024 MiB) aligned with README; validate schema after changes.

---

## 7. Core Commands & Tools
- `cargo build`, `cargo run -p cave-daemon`  
- `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`  
- `cargo test` / `cargo test -p <crate>`  
- `SQLX_OFFLINE=true sqlx migrate run`  
- `make sbom`, `make slsa`, `cosign sign-blob <file>`  
- `ajv validate -s schema/cave.schema.json -d cave.yaml`

Log command outcomes in Progress or final reports.

---

## 8. Pre-Commit Checklist
1. Format (`cargo fmt --all`)  
2. Lint (`cargo clippy -- -D warnings`)  
3. Test (`cargo test` or targeted)  
4. Validate schema if configs changed  
5. Update Progress/Roadmap entries  
6. Ensure Feature Origin entry exists for new adaptations  
7. Confirm docs mirror behaviour  
8. Verify `git status -sb` (only intended files changed)  
9. Mention tests run / skipped in summary  
10. Cleanup sandboxes & temp files

---

## 9. Environment Policy Snapshot
| Environment | Telemetry | Database | Notes |
|-------------|-----------|----------|-------|
| Development | `CAVE_OTEL_SAMPLING_RATE=1.0` | SQLite fallback allowed | Fast iteration, treat secrets seriously. |
| Staging     | ≈0.5      | Postgres + RLS | Mirror production rotation & audit policies. |
| Production  | 0.05–0.2  | Postgres + RLS | Strict security, cosign signing mandatory. |

Verify `/healthz` & `/metrics` before promotions; log deviations in Progress.

---

## 10. Release Protocol (summary)
1. Ensure Phase-0 milestones are met (kernel, DB, Web-UIs).  
2. Generate SBOM/SLSA (`make sbom`, `make slsa`) and sign artefacts (`cosign sign-blob`).  
3. Run full test matrix (`cargo fmt`, `cargo clippy`, `cargo test`, `pytest security/`, UI E2E).  
4. Sync docs (README version, roadmap, Progress, release notes).  
5. Coordinate key rotations & telemetry updates with Admin-Orchestrator.  
6. Create signed git tag referencing artefacts.

---

## 11. Security & Compliance
- Clean-Room: document every inspiration in `docs/FEATURE_ORIGINS.md` (URL, rationale, design, tests, no-copy statement, commit/PR, reviewer).  
- Key rotation: Admin 90d, Namespace 30d (auto-rotate after 7d on use), Model Access & Session 1h.  
- Rotation webhook (`POST /api/v1/auth/keys/rotated`) must be HMAC-signed; audit all key ops.  
- Audit logs = append-only JSONL with cryptographic signatures (DB + filesystem).  
- Enforce gateway rate limits (Admin 1000/min, Namespace 100/min, Session 50/min, Model access 200/min).  
- Guard secrets (`BKG_API_KEY`, TLS certs, cosign key); never log plaintext.  
- Keep telemetry exporters aligned with `CAVE_OTEL_SAMPLING_RATE` policy.

---

## 12. Outstanding Decisions
Log details and owners in `docs/Progress.md`.
- Multi-agent task routing & escalation (post Phase-0).  
- Secrets management solution for API/cosign/orchestrator keys.  
- Production Postgres deployment (Helm/operator, backups, RLS seeding).  
- UI automation tooling (Playwright vs. Cypress) and CI integration.  
- Object storage & realtime rollout plan (bucket policies, WAL subscriptions).

---

## 13. Quick FAQ
- **Höhere Sandbox-Limits?** → Admin-Orchestrator genehmigt; dokumentiere in Progress & config override.  
- **Feature-Inspiration dokumentieren?** → `docs/FEATURE_ORIGINS.md` (Template aus README §20).  
- **Pflicht-Tests vor PR?** → `cargo fmt`, `cargo clippy`, `cargo test`, schema check, `pytest security/` wenn relevant.  
- **Telemetry Pflicht?** → Jeder Service braucht `/healthz` & `/metrics`; Sampling je Umgebung justieren.  
- **Audit-Logs?** → Signierte JSONL schreiben, Integrität regelmäßig prüfen, Rotation festhalten.

---

## 14. Reinforcement Reminders
- README nur via koordinierten PR anfassen (inkl. Tests, SBOM, SLSA).  
- Nenne Tests (run/not run, command) in jeder Übergabe.  
- Nutze `rg` statt `grep -R`; halte `file.md` bei Strukturänderungen aktuell.  
- Stoppe Sandboxes (`sandbox.stop()`) vor Sitzungsende.  
- Halte `PROMPT.md`, `AGENTS.md`, Roadmap und Progress synchron, sobald Regeln oder Prioritäten wechseln.

---

Ende. Befolge diese Leitlinien, um Phase-0 sicher, auditierbar und dokumentiert abzuschließen.
