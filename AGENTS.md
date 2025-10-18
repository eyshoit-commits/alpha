# Repository Guidelines

Version: 2.0  
Last Updated: 2025-10-18  
Maintainer: @bkgoder

---

## 1. How To Use This Guide
- Treat this document, the binding `README.md` (v1.8.2), and `PROMPT.md` as a single rule set.  
- Load all three files plus `docs/Progress.md` and `docs/roadmap.md` at the start of every session.  
- Update this guide whenever processes change; note the rationale in commit messages.  
- File names in **bold** are mandatory references; inline code denotes commands or paths.

---

## 2. Repository Topology (Quick Map)
```
.
├── Cargo.toml           # Workspace manifest (bkg-db, cave-kernel, cave-daemon)
├── config/              # Sandbox configuration (`sandbox_config.toml`)
├── crates/              # Rust crates (db, kernel, daemon)
├── docs/                # Architecture, env, roadmap, feature origins, legacy agents guide
├── schema/              # JSON schema (`cave.schema.json`)
├── .codex/              # Codex/LLM agent configuration
├── PROMPT.md            # Day-to-day operating prompt
├── file.md              # Concise tree listing for quick orientation
└── README.md            # Binding system prompt & implementation spec
```

Key pointers:
- `crates/bkg-db` – persistence API & migrations (`migrations/0001_init.sql`).  
- `crates/cave-kernel` – sandbox lifecycle orchestration (`src/lib.rs`, `src/isolation.rs`).  
- `crates/cave-daemon` – Axum-based service exposing REST/SSE/MCP (`src/main.rs`, `src/auth.rs`).  
- `docs/Progress.md` – live status tracker; keep references to `file:line`.  
- `docs/roadmap.md` – Phase-0 milestones with dependencies and deliverables.  
- `.codex/codex_config.toml` – ensures `PROMPT.md`, `AGENTS.md`, `Progress.md` are injected into the agent context.

---

## 3. Phase-0 Priorities (Mandatory Order)
1. **Kernel Hardening**  
   - Implement namespaces, seccomp, cgroups v2, FS overlay.  
   - Add integration tests for lifecycle (`create → start → exec → stop`).  
   - Capture signed JSONL audit events.
2. **Persistent Database**  
   - Migrate from SQLite prototype to Postgres with Row-Level Security.  
   - Maintain migrations and rotation-friendly schema.  
   - Provide integration tests covering namespace isolation.
3. **Web-UIs**  
   - Admin UI: model manager, key wizard, peer dashboard.  
   - User UI: sandbox lifecycle control, telemetry view, chat studio.  
   - Provide CI-backed end-to-end tests (Playwright/Cypress acceptable).
4. Only after these three are production-ready (with CI artefacts: SBOM, SLSA, tests) may P2P, marketplace, or multi-agent orchestration be enabled.

Document Phase-0 progress and evidence in **docs/Progress.md** and release notes.

---

## 4. Roles & Responsibilities
### Sandbox Coding Agent
- Follow `PROMPT.md`, this guide, and `README.md` for every session.  
- Use `apply_patch` or equivalent targeted edits; never destructive git commands without approval.  
- Run relevant tests (`cargo fmt`, `cargo clippy`, `cargo test`, schema checks) and record results.  
- Update **docs/Progress.md** with concise status (include filenames/lines) and note open questions.  
- Provide clear hand-off summaries with next steps.

### Admin-Orchestrator
- Issue, rotate, and revoke API keys (`/api/v1/auth/keys/*`); manage rotation webhook signatures.  
- Maintain SBOM/SLSA pipelines (`make sbom`, `make slsa`, `cosign sign-blob`).  
- Approve sandbox limit overrides and coordinate telemetry settings (`CAVE_OTEL_SAMPLING_RATE`).  
- Ensure documentation and roadmap stay aligned with implementation.

### Security Agent
- Maintain **docs/security.md** (Threat Matrix) and enforce CI check via `pytest security/`.  
- Verify webhook HMAC signatures, audit log integrity, and secrets management plan.  
- Oversee Postgres RLS deployment and rotation caches.

### Docs Agent
- Keep textual assets up to date (`PROMPT.md`, `AGENTS.md`, `docs/Progress.md`, `docs/roadmap.md`, `docs/FEATURE_ORIGINS.md`).  
- Ensure new features include provenance entries (inspiration, rationale, no-copy statement, commits, reviewers).  
- Support onboarding by highlighting key workflows and diagrams.

---

## 5. Standard Workflow (Per Task)
1. **Context** – Read `README.md`, `PROMPT.md`, `AGENTS.md`, `docs/Progress.md`, `docs/roadmap.md`.  
2. **Plan** – Draft a multi-step plan (no single-step plans) using the planning tool; keep it updated.  
3. **Investigate** – Inspect code with non-destructive commands (`rg`, `ls`, `sed`); check `git status -sb`.  
4. **Implement** – Apply changes with `apply_patch` or crate-specific tools; respect sandbox limits.  
5. **Validate** – Run required checks:  
   - `cargo fmt --all`  
   - `cargo clippy --all-targets --all-features -- -D warnings`  
   - `cargo test [--package <name>]`  
   - `ajv validate -s schema/cave.schema.json -d cave.yaml` (when config changes)  
6. **Document** – Update `docs/Progress.md` (status, tests run, follow-ups), adjust roadmap/docs if scope changed.  
7. **Report** – Summarize changes, test results, and next steps in the final message; highlight remaining risks.  
8. **Cleanup** – Stop sandboxes (`sandbox.stop()`), ensure no persistent sessions remain, clear temporary files if needed.

Failure to run available tests must be explicitly noted with mitigation or follow-up actions.

---

## 6. Key Directories & Expectations
### `crates/bkg-db`
- Provide async database APIs with sqlx migrations.  
- Enforce uniqueness for `(namespace, name)` pairs and translate DB errors to domain errors.  
- Implement future Postgres RLS policies; keep DTOs serialized via serde.  
- Add integration tests (`cargo test -p bkg-db`); use `SQLX_OFFLINE=true sqlx migrate run` before committing migrations.

### `crates/cave-kernel`
- Drive sandbox lifecycle, resource enforcement, and audit logging.  
- Keep `ProcessSandboxRuntime` modular to support additional isolation implementations.  
- Instrument lifecycle transitions with tracing.  
- Add Linux-only integration tests when isolation primitives are in place.

### `crates/cave-daemon`
- Serve `/api/v1/sandboxes`, `/api/v1/auth/keys`, `/healthz`, `/metrics`, and `/mcp`.  
- Ensure AuthService handles scopes, TTLs, revocation cache, and eventual rotation webhook.  
- Stream stdout/stderr to clients with backpressure (WS/SSE).  
- Expand `tests/` to exercise REST flows or use request-based integration tests.

### `docs/`
- `architecture.md` – keep diagrams and component descriptions current.  
- `env.md` – enumerate mandatory environment variables (highlight sensitive flags).  
- `roadmap.md` – align milestones with Phase-0 checklist; update as tasks close.  
- `Progress.md` – single source of truth for current status, pending questions, and owners.  
- `FEATURE_ORIGINS.md` – document inspirations, tests, links, and no-copy attestations.

### `config/` & `schema/`
- `sandbox_config.toml` – default limits (CPU 1 vCPU, RAM 512 MiB, Timeout 60 s, Disk 500 MiB) must match README.  
- `cave.schema.json` – update schema whenever configuration surface changes; validate using `ajv`.

---

## 7. Build, Test & Development Commands
- `cargo build` – compile entire workspace (default target: `cave-daemon`).  
- `cargo run -p cave-daemon` – start the daemon (requires `BKG_DB_DSN` or `BKG_DB_PATH`).  
- `cargo fmt --all` – enforce Rust formatting.  
- `cargo clippy --all-targets --all-features -- -D warnings` – lint with warnings-as-errors.  
- `cargo test` – workspace test suite; use `-p <crate>` for targeted runs.  
- `SQLX_OFFLINE=true sqlx migrate run` – verify migrations offline.  
- `make sbom`, `make slsa`, `cosign sign-blob <file>` – release artefact pipeline (ensure cosign key available).  
- `ajv validate -s schema/cave.schema.json -d cave.yaml` – configuration validation.

Record command outcomes (success/failure) in `docs/Progress.md` or final summaries.

---

## 8. Pre-Commit Checklist
1. `cargo fmt --all`  
2. `cargo clippy --all-targets --all-features -- -D warnings`  
3. `cargo test` (or targeted crate tests)  
4. Schema validation when configs change  
5. Update `docs/Progress.md` and, if needed, `docs/roadmap.md`  
6. Ensure `FEATURE_ORIGINS.md` entry exists for newly adapted ideas  
7. Confirm documentation (PROMPT, AGENTS, architecture, env) matches code behaviour  
8. Verify only intended files changed (`git status -sb`)  
9. Summarize tests and open questions in final output  
10. Clean up sandboxes and temporary artifacts

---

## 9. Environment Policies
| Environment | Telemetry | Database | Notes |
|-------------|-----------|----------|-------|
| Development | `CAVE_OTEL_SAMPLING_RATE = 1.0` | SQLite fallback permitted | Fast iteration; treat secrets as real. |
| Staging     | `≈ 0.5`   | Postgres + RLS | Mirror production key rotation and audit policies. |
| Production  | `0.05 – 0.2` | Postgres + RLS | Strict security controls; cosign signing mandatory. |

Always verify `/healthz` and `/metrics` before promoting builds. Record deviations in `docs/Progress.md`.

---

## 10. Release Protocol
1. Confirm Phase-0 gating items complete (kernel hardening, Postgres RLS, Web-UIs).  
2. Generate SBOM (`make sbom`) and SLSA provenance (`make slsa`).  
3. Sign SBOM(s) with `cosign sign-blob <sbom> --key cosign.key`; store signatures securely.  
4. Run full test matrix:  
   - `cargo fmt`, `cargo clippy`, `cargo test`  
   - `pytest security/` (Threat Matrix)  
   - UI end-to-end suites (Playwright/Cypress)  
5. Update documentation (README version, roadmap, Progress tracker, release notes).  
6. Coordinate with Admin-Orchestrator for key rotations and telemetry adjustments.  
7. Create signed git tag referencing SBOM/SLSA artefacts.

---

## 11. Security & Compliance
- Maintain Clean-Room discipline; credit inspirations in `FEATURE_ORIGINS.md`.  
- Rotate Admin keys every 90 days, Namespace keys every 30 days (auto-rotate after 7 days of age on use), Model access keys hourly, Session keys hourly.  
- Implement rotation webhook (`POST /api/v1/auth/keys/rotated`) with HMAC signatures; audit all events.  
- Keep audit logs as append-only JSONL with cryptographic signatures; store both in DB and filesystem.  
- Enforce gateway rate limits (Admin 1000/min, Namespace 100/min, Session 50/min, Model access 200/min).  
- Restrict secrets (`BKG_API_KEY`, `cosign.key`, TLS certs) to secure stores; never log plaintext.  
- Verify telemetry exporters respect sampling policies; route OTLP traffic to approved collectors.

---

## 12. File Governance Highlights
### `PROMPT.md`
- Condensed working instructions; muss auf `AGENTS.md` verweisen.  
- Enthält Erinnerungen an Planung, Tests, Dokumentation und Cleanup.

### `docs/Progress.md`
- Record current status with bullet updates referencing `file:line`.  
- Note tests executed (or not) and assign owners to pending work.  
- Maintain sections for Phase-0 commitments, documentation, CI, governance, open questions.

### `docs/roadmap.md`
- Reflect current milestone progress; align with Phase-0 sequence.  
- Update deliverables, dependencies, and actions when tasks close.

### `docs/FEATURE_ORIGINS.md`
- Add entries per feature: source URL, rationale, fresh implementation summary, API impacts, testing, no-copy statement, commit/PR, reviewer sign-off.

### `.codex/codex_config.toml`
- Ensure `include_agents_md = true`, `include_prompt_file = true`, `include_progress_file = true`.  
- Keep approval policies strict (`filesystem:delete`, `git:force-push`, `network:outgoing`).  
- Use large-context model (`gpt-4-1106-preview`) with matching `max_context_tokens`.

---

## 13. Frequently Asked Questions
- **How do I request higher sandbox limits?**  
  Submit a request to the Admin-Orchestrator; document approval in `docs/Progress.md` and enforce via config overrides.
- **Where do I record inspiration from external repositories?**  
  In `docs/FEATURE_ORIGINS.md`. Include URL, rationale, design summary, tests, no-copy statement, commit/PR, reviewer.
- **What tests are mandatory before opening a PR?**  
  `cargo fmt`, `cargo clippy`, `cargo test`, schema validation (when applicable), `pytest security/` if security artefacts touched. Mention results in the PR description and final summary.
- **When is multi-agent orchestration allowed?**  
  Only after Phase-0 deliverables are production-ready and documented.
- **What telemetry endpoints must exist?**  
  `/healthz` (200 or 503) and `/metrics` (Prometheus format) for every service; ensure CI/liveness probes rely on them.
- **How do I handle audit logs?**  
  Write signed JSONL entries for lifecycle events, key operations, rotations, and deletion actions; verify integrity during reviews.

---

## 14. Outstanding Decisions (Track in Progress.md)
- Define task routing for multi-agent orchestration post Phase-0.  
- Select secrets management solution for API keys, cosign keys, and orchestrator credentials.  
- Establish escalation procedures for sandbox failures or authorization denials.  
- Document production Postgres deployment approach (Helm chart/operator).  
- Choose UI testing framework (Playwright vs. Cypress) and integrate into CI.

---

## 15. Reinforcement Reminders
- Keep `README.md` unchanged unless a coordinated PR updates specs, acceptance criteria, tests, SBOM, and SLSA.  
- Always mention which tests ran (or why they were skipped) in handover messages.  
- Log new TODOs or blockers under **docs/Progress.md → Offene Fragen / Klärungsbedarf**.  
- Use `rg` and `rg --files` for fast searching; avoid broader `grep -R` unless required.  
- Respect sandbox cleanup procedures (`sandbox.stop()`); never leave persistent sessions running.  
- Sync `AGENTS.md`, `PROMPT.md` und relevante Dokumente, wenn Regeln angepasst werden.  
- Validate the concise `file.md` tree if repository layout changes.  
- Coordinate with the Admin-Orchestrator before large-scale refactors or infra changes.

---

End of document. Follow these rules to keep the repository compliant, secure, and aligned with the Phase-0 roadmap.
