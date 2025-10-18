# Repository Guidelines

## Document Usage
- Load this file as part of every session's context alongside README.md and PROMPT.md.
- Treat all instructions herein as mandatory unless superseded by README updates.
- Update the guide when repository processes evolve; keep change history in git logs.

## Table of Contents
- [Purpose & Scope](#purpose-&-scope)
- [Repository Topology](#repository-topology)
- [Core Principles](#core-principles)
- [Standard Workflow](#standard-workflow)
- [Directory Guide: crates/bkg-db](#directory-guide:-cratesbkg-db)
- [Directory Guide: crates/cave-kernel](#directory-guide:-cratescave-kernel)
- [Directory Guide: crates/cave-daemon](#directory-guide:-cratescave-daemon)
- [Pre-Commit Megachecklist](#pre-commit-megachecklist)
- [Environment Policy: Development](#environment-policy:-development)
- [Environment Policy: Staging](#environment-policy:-staging)
- [Environment Policy: Production](#environment-policy:-production)
- [Release Protocol](#release-protocol)
- [Frequently Asked Questions](#frequently-asked-questions)
- [File Governance](#file-governance)
- [Role Responsibility Matrix](#role-responsibility-matrix)
- [Scenario Playbooks](#scenario-playbooks)
- [Glossary](#glossary)
- [Outstanding Decisions](#outstanding-decisions)
- [Reinforcement Reminders](#reinforcement-reminders)

## Purpose & Scope
This guide codifies every operational rule for contributors and automated agents within the BKG Phase-0 repository.
Use it as the authoritative reference when preparing tasks, reviews, deployments, or sandbox operations.
It complements the binding README v1.8.2 and supersedes prior drafts scattered across docs/Agents.md or legacy prompts.
Always load this document alongside README, PROMPT.md, and docs/roadmap.md before starting substantive work.

## Repository Topology
Workspace root contains Cargo.toml describing members bkg-db, cave-kernel, cave-daemon.
Rust crates live under crates/, each with dedicated Cargo manifests and src directories.
Operational documentation resides under docs/, including architecture, env references, roadmap, and progress tracker.
Schema definitions live in schema/ with cave.schema.json acting as the ajv validation target for cave.yaml.
Configuration defaults (sandbox limits, security toggles) live in config/sandbox_config.toml.
Automation settings for the LLM tooling sit in .codex/codex_config.toml and must be updated carefully.
Build artifacts (target/) are ignored; never commit generated binaries or incremental caches.
Root-level AGENTS.md (this file) consolidates all contributor rules and should be opened first by automated agents.

## Core Principles
Phase-0 gating is inviolable: do not enable P2P, marketplace, or distributed inference features until kernel, DB, and Web-UIs ship with tests.
Clean-Room engineering applies to every feature: you may consult external repos for ideas but must write code from scratch.
Security-by-default is mandatory: enforce namespaces, seccomp, cgroups, FS overlays, TLS, audit logging, and key rotation policies.
Least-privilege access: prefer namespace-scoped keys over admin keys, justify escalations, document in Progress.md.
Telemetry discipline: tune CAVE_OTEL_SAMPLING_RATE per environment to balance observability and cost.
Auditability: every significant action (key issuance, sandbox exec, deploy) must emit signed JSONL audit events.
Documentation parity: whenever you adjust code behaviour, update relevant docs (architecture, roadmap, env, feature origins).

## Standard Workflow
Review README.md (system prompt, release requirements).
Open PROMPT.md for the active coding instructions.
Scan AGENTS.md (this guide) and docs/Progress.md for outstanding tasks.
Inspect docs/roadmap.md to understand current milestones and blockers.
Assess existing changes via `git status -sb`; do not overwrite uncommitted work.
Set up environment variables (BKG_API_KEY, BKG_DB_DSN/BKG_DB_PATH) as needed.
Create or update work plan using the plan tool (minimum two steps, no single-step plans).
Search repository with `rg` or `rg --files`; avoid slower recursive grep by default.
Perform edits using `apply_patch`; avoid destructive `rm`, `mv`, `git reset --hard` unless explicitly approved.
Run `cargo fmt --all` and `cargo clippy --all-targets --all-features -- -D warnings` when touching Rust files.
Run `cargo test` or targeted `cargo test -p crate_name` where unit/integration tests exist.
Validate configs: `ajv validate -s schema/cave.schema.json -d cave.yaml` when editing configuration schemas.
Trigger SBOM/SLSA pipelines (`make sbom`, `make slsa`, `cosign sign-blob`) before release-related merges.
Update docs/Progress.md with concise status referencing file:line.
Document test results, follow-up tasks, and open questions in final response.
Stop or clean sandboxes via `sandbox.stop()`; never leave persistent sessions running unattended.

## Directory Guide: crates/bkg-db
Purpose: Persistence layer handling sandbox metadata, key records, audit entries.
Key Files:
  * Cargo.toml
  * src/lib.rs
  * migrations/0001_init.sql
  * tests/ (add when implementing integration tests)
Primary Responsibilities:
  * Provide async Database API with connection pooling and migration support via sqlx.
  * Implement row-level security policy stubs; upgrade to Postgres with RLS soon.
  * Expose resource limit persistence and audit event storage for the kernel.
  * Ensure unique constraints for sandbox namespace/name combinations, returning domain errors on conflicts.
  * Add property-based tests for serialization/deserialization of records using serde.
Testing Notes:
  * `cargo test -p bkg-db` to run all DB-related tests.
  * Use `SQLX_OFFLINE=true sqlx migrate run` to verify migrations locally before PRs.

## Directory Guide: crates/cave-kernel
Purpose: Sandbox orchestration library powering lifecycle management and isolation controls.
Key Files:
  * Cargo.toml
  * src/lib.rs
  * src/isolation.rs
  * tests/ (add for integration)
Primary Responsibilities:
  * Manage sandbox lifecycle states (provisioned, running, stopped) and persist transitions.
  * Apply resource limits using cgroups v2, namespaces, seccomp filters, and FS overlays.
  * Integrate with audit logging by recording execution outcomes and durations.
  * Expose traits and structs for runtime implementations (ProcessSandboxRuntime and future MicroVM runtimes).
  * Emit tracing spans for start/exec/stop flows to support telemetry sampling.
Testing Notes:
  * `cargo test -p cave-kernel` for unit tests.
  * Plan to add integration tests exercising actual cgroup and seccomp interactions (Linux only).

## Directory Guide: crates/cave-daemon
Purpose: Axum-based service exposing REST, WebSocket, and MCP endpoints to manage sandboxes and keys.
Key Files:
  * Cargo.toml
  * src/main.rs
  * src/auth.rs
Primary Responsibilities:
  * Serve /api/v1/sandboxes endpoints for create/start/exec/stop/status/delete actions.
  * Expose /healthz and /metrics endpoints for liveness/readiness and Prometheus scraping.
  * Implement authentication service issuing namespace/admin keys with TTLs and revocation logic.
  * Stream logs via WebSocket or SSE and ensure audit records are appended per operation.
  * Coordinate telemetry (tracing_subscriber) including support for CAVE_OTEL_SAMPLING_RATE overrides.
Testing Notes:
  * `cargo test -p cave-daemon` (add integration tests using `tower::Service` or `axum::Router`).
  * Use curl or HTTP client scripts to verify /healthz, /metrics, /api/v1/auth/keys flows locally.

## Pre-Commit Megachecklist
001. Run `cargo fmt --all` to format Rust code.
002. Run `cargo clippy --all-targets --all-features -- -D warnings` to enforce lint cleanliness.
003. Run `cargo test` (workspace) or targeted crate tests.
004. Ensure docs/Progress.md is updated with status and file references.
005. Update docs/roadmap.md if milestone progress changed.
006. Validate schema changes with `ajv validate`.
007. Attach SBOM/SLSA results if part of release pipeline.
008. Reference docs/FEATURE_ORIGINS.md entry for new features or inspirations.
009. Verify AGENTS.md remains consistent with PROMPT.md and README.
010. Confirm `file.md` summary listing stays aligned with actual structure.
011. Run `cargo fmt --all` to format Rust code.
012. Run `cargo clippy --all-targets --all-features -- -D warnings` to enforce lint cleanliness.
013. Run `cargo test` (workspace) or targeted crate tests.
014. Ensure docs/Progress.md is updated with status and file references.
015. Update docs/roadmap.md if milestone progress changed.
016. Validate schema changes with `ajv validate`.
017. Attach SBOM/SLSA results if part of release pipeline.
018. Reference docs/FEATURE_ORIGINS.md entry for new features or inspirations.
019. Verify AGENTS.md remains consistent with PROMPT.md and README.
020. Confirm `file.md` summary listing stays aligned with actual structure.
021. Run `cargo fmt --all` to format Rust code.
022. Run `cargo clippy --all-targets --all-features -- -D warnings` to enforce lint cleanliness.
023. Run `cargo test` (workspace) or targeted crate tests.
024. Ensure docs/Progress.md is updated with status and file references.
025. Update docs/roadmap.md if milestone progress changed.
026. Validate schema changes with `ajv validate`.
027. Attach SBOM/SLSA results if part of release pipeline.
028. Reference docs/FEATURE_ORIGINS.md entry for new features or inspirations.
029. Verify AGENTS.md remains consistent with PROMPT.md and README.
030. Confirm `file.md` summary listing stays aligned with actual structure.
031. Run `cargo fmt --all` to format Rust code.
032. Run `cargo clippy --all-targets --all-features -- -D warnings` to enforce lint cleanliness.
033. Run `cargo test` (workspace) or targeted crate tests.
034. Ensure docs/Progress.md is updated with status and file references.
035. Update docs/roadmap.md if milestone progress changed.
036. Validate schema changes with `ajv validate`.
037. Attach SBOM/SLSA results if part of release pipeline.
038. Reference docs/FEATURE_ORIGINS.md entry for new features or inspirations.
039. Verify AGENTS.md remains consistent with PROMPT.md and README.
040. Confirm `file.md` summary listing stays aligned with actual structure.
041. Run `cargo fmt --all` to format Rust code.
042. Run `cargo clippy --all-targets --all-features -- -D warnings` to enforce lint cleanliness.
043. Run `cargo test` (workspace) or targeted crate tests.
044. Ensure docs/Progress.md is updated with status and file references.
045. Update docs/roadmap.md if milestone progress changed.
046. Validate schema changes with `ajv validate`.
047. Attach SBOM/SLSA results if part of release pipeline.
048. Reference docs/FEATURE_ORIGINS.md entry for new features or inspirations.
049. Verify AGENTS.md remains consistent with PROMPT.md and README.
050. Confirm `file.md` summary listing stays aligned with actual structure.
051. Run `cargo fmt --all` to format Rust code.
052. Run `cargo clippy --all-targets --all-features -- -D warnings` to enforce lint cleanliness.
053. Run `cargo test` (workspace) or targeted crate tests.
054. Ensure docs/Progress.md is updated with status and file references.
055. Update docs/roadmap.md if milestone progress changed.
056. Validate schema changes with `ajv validate`.
057. Attach SBOM/SLSA results if part of release pipeline.
058. Reference docs/FEATURE_ORIGINS.md entry for new features or inspirations.
059. Verify AGENTS.md remains consistent with PROMPT.md and README.
060. Confirm `file.md` summary listing stays aligned with actual structure.
061. Run `cargo fmt --all` to format Rust code.
062. Run `cargo clippy --all-targets --all-features -- -D warnings` to enforce lint cleanliness.
063. Run `cargo test` (workspace) or targeted crate tests.
064. Ensure docs/Progress.md is updated with status and file references.
065. Update docs/roadmap.md if milestone progress changed.
066. Validate schema changes with `ajv validate`.
067. Attach SBOM/SLSA results if part of release pipeline.
068. Reference docs/FEATURE_ORIGINS.md entry for new features or inspirations.
069. Verify AGENTS.md remains consistent with PROMPT.md and README.
070. Confirm `file.md` summary listing stays aligned with actual structure.
071. Run `cargo fmt --all` to format Rust code.
072. Run `cargo clippy --all-targets --all-features -- -D warnings` to enforce lint cleanliness.
073. Run `cargo test` (workspace) or targeted crate tests.
074. Ensure docs/Progress.md is updated with status and file references.
075. Update docs/roadmap.md if milestone progress changed.
076. Validate schema changes with `ajv validate`.
077. Attach SBOM/SLSA results if part of release pipeline.
078. Reference docs/FEATURE_ORIGINS.md entry for new features or inspirations.
079. Verify AGENTS.md remains consistent with PROMPT.md and README.
080. Confirm `file.md` summary listing stays aligned with actual structure.

## Environment Policy: Development
High observability, default sampling rate 1.0, sqlite fallback allowed.
Mandatory Actions:
  * Apply sandbox limits defined in config/sandbox_config.toml for Development.
  * Ensure BKG_API_KEY scope matches environment (namespace vs admin).
  * Rotate keys on schedule (admin 90d, namespace 30d, model access 1h).
  * Validate telemetry endpoints (/metrics, OTLP collector) respond correctly.
  * Run threat-matrix tests and audit log verifications after deployments.

## Environment Policy: Staging
Balanced telemetry, sampling 0.5, replicate production key rotation and RLS policies.
Mandatory Actions:
  * Apply sandbox limits defined in config/sandbox_config.toml for Staging.
  * Ensure BKG_API_KEY scope matches environment (namespace vs admin).
  * Rotate keys on schedule (admin 90d, namespace 30d, model access 1h).
  * Validate telemetry endpoints (/metrics, OTLP collector) respond correctly.
  * Run threat-matrix tests and audit log verifications after deployments.

## Environment Policy: Production
Strict security posture, sampling 0.05-0.2, Postgres + RLS mandatory, cosign signing enforced.
Mandatory Actions:
  * Apply sandbox limits defined in config/sandbox_config.toml for Production.
  * Ensure BKG_API_KEY scope matches environment (namespace vs admin).
  * Rotate keys on schedule (admin 90d, namespace 30d, model access 1h).
  * Validate telemetry endpoints (/metrics, OTLP collector) respond correctly.
  * Run threat-matrix tests and audit log verifications after deployments.

## Release Protocol
1. Draft release scope and verify Phase-0 dependencies satisfied (kernel isolation, RLS, Web-UI).
2. Generate SBOM via `make sbom`; review output for completeness.
3. Generate SLSA provenance via `make slsa`; store artifacts in secure registry.
4. Sign SBOM using `cosign sign-blob <sbom> --key cosign.key`; archive signature references.
5. Run full test matrix (cargo test, pytest security/, UI end-to-end tests where applicable).
6. Verify docs (README, roadmap, Progress) reflect release content.
7. Coordinate with Admin-Orchestrator to distribute release notes and update cosign key records.
8. Tag release in git with signed tag; include SBOM/SLSA artifact references.

## Frequently Asked Questions
**Q:** How do I request additional sandbox resources?
**A:** Submit a ticket to the Admin-Orchestrator with justification; update docs/Progress.md once approved.
**Q:** Where do I document adaption of external ideas?
**A:** Use docs/FEATURE_ORIGINS.md and include repository URL, rationale, implementation summary, tests, and no-copy statement.
**Q:** What tests must pass before PR merge?
**A:** At minimum cargo fmt, cargo clippy, cargo test, schema validations, and threat-matrix tests for relevant changes.
**Q:** How are API keys rotated?
**A:** Run rotation jobs, hit POST /api/v1/auth/keys/rotate, notify subscribers via POST /api/v1/auth/keys/rotated webhook.
**Q:** What telemetry endpoints are required?
**A:** All services must expose /healthz (200/503) and /metrics (Prometheus).
**Q:** When can multi-agent orchestration be enabled?
**A:** Only after Phase-0 deliverables (kernel, DB, Web-UI) are production-ready with tests.

## File Governance
### PROMPT.md
- Condense operational steps for coding sessions; keep synced with AGENTS.md.
- Mention AGENTS.md and docs/Agents.md where relevant.
- Highlight plan tool usage, sandbox etiquette, testing obligations.
### docs/Progress.md
- Record current status with references to file:line.
- Mark tasks complete with rationale, cite missing tests, and note owners.
- Maintain sections for Phase-0 commitments, docs, CI, governance, open questions.
### docs/roadmap.md
- List Phase-0 tasks with deliverables, dependencies, and actions.
- Update when milestones finish; align with Progress.md checkboxes.
- Include supportive workstreams (CI, governance, documentation).
### config/sandbox_config.toml
- Ensure limits align with README Section 7 defaults.
- Disallow overrides unless allow_override=true with recorded approval.
- Keep security toggles (namespaces, seccomp, cgroups, overlay) enabled by default.
### .codex/codex_config.toml
- Guarantee include_agents_md=true so AGENTS.md is loaded into prompts.
- Set approval policies to require confirmation for dangerous actions (filesystem delete, force push, network).
- Disable local shell and network access to enforce sandbox usage.

## Role Responsibility Matrix
### Sandbox Coding Agent
- Follow AGENTS.md, PROMPT.md, README.
- Produce code changes via apply_patch.
- Run tests and record results.
- Update Progress.md and propose next steps.
### Admin-Orchestrator
- Manage API keys, cosign keys, release readiness.
- Approve sandbox overrides and monitor telemetry.
- Ensure docs/roadmap.md and Progress.md stay accurate.
- Coordinate multi-agent deployments post Phase-0.
### Security Agent
- Maintain docs/security.md threat matrix.
- Run pytest security/ pipelines.
- Audit webhook signatures, rotation logs, and telemetry configuration.
### Docs Agent
- Maintain documentation set (PROMPT, AGENTS, Progress, Roadmap, Feature Origins).
- Ensure cross-references are current and consistent.
- Facilitate onboarding by keeping AGENTS.md accessible.

## Scenario Playbooks
Scenario 001: Kernel: Add seccomp profile for syscall filtering.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 002: Kernel: Extend audit logging for exec outcomes.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 003: Kernel: Implement persistent workspace snapshotting.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 004: DB: Migrate to Postgres with RLS policies.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 005: DB: Add key rotation audit trail.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 006: DB: Implement tests for namespace scoping.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 007: Daemon: Add SSE streaming for logs.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 008: Daemon: Enforce rate limits via middleware.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 009: Daemon: Expose metrics for sandbox lifecycle durations.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 010: Docs: Update roadmap milestone statuses.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 011: Kernel: Add seccomp profile for syscall filtering.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 012: Kernel: Extend audit logging for exec outcomes.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 013: Kernel: Implement persistent workspace snapshotting.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 014: DB: Migrate to Postgres with RLS policies.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 015: DB: Add key rotation audit trail.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 016: DB: Implement tests for namespace scoping.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 017: Daemon: Add SSE streaming for logs.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 018: Daemon: Enforce rate limits via middleware.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 019: Daemon: Expose metrics for sandbox lifecycle durations.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 020: Docs: Update roadmap milestone statuses.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 021: Kernel: Add seccomp profile for syscall filtering.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 022: Kernel: Extend audit logging for exec outcomes.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 023: Kernel: Implement persistent workspace snapshotting.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 024: DB: Migrate to Postgres with RLS policies.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 025: DB: Add key rotation audit trail.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 026: DB: Implement tests for namespace scoping.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 027: Daemon: Add SSE streaming for logs.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 028: Daemon: Enforce rate limits via middleware.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 029: Daemon: Expose metrics for sandbox lifecycle durations.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 030: Docs: Update roadmap milestone statuses.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 031: Kernel: Add seccomp profile for syscall filtering.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 032: Kernel: Extend audit logging for exec outcomes.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 033: Kernel: Implement persistent workspace snapshotting.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 034: DB: Migrate to Postgres with RLS policies.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 035: DB: Add key rotation audit trail.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 036: DB: Implement tests for namespace scoping.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 037: Daemon: Add SSE streaming for logs.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 038: Daemon: Enforce rate limits via middleware.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 039: Daemon: Expose metrics for sandbox lifecycle durations.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 040: Docs: Update roadmap milestone statuses.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 041: Kernel: Add seccomp profile for syscall filtering.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 042: Kernel: Extend audit logging for exec outcomes.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 043: Kernel: Implement persistent workspace snapshotting.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 044: DB: Migrate to Postgres with RLS policies.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 045: DB: Add key rotation audit trail.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 046: DB: Implement tests for namespace scoping.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 047: Daemon: Add SSE streaming for logs.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 048: Daemon: Enforce rate limits via middleware.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 049: Daemon: Expose metrics for sandbox lifecycle durations.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 050: Docs: Update roadmap milestone statuses.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 051: Kernel: Add seccomp profile for syscall filtering.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 052: Kernel: Extend audit logging for exec outcomes.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 053: Kernel: Implement persistent workspace snapshotting.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 054: DB: Migrate to Postgres with RLS policies.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 055: DB: Add key rotation audit trail.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 056: DB: Implement tests for namespace scoping.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 057: Daemon: Add SSE streaming for logs.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 058: Daemon: Enforce rate limits via middleware.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 059: Daemon: Expose metrics for sandbox lifecycle durations.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 060: Docs: Update roadmap milestone statuses.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 061: Kernel: Add seccomp profile for syscall filtering.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 062: Kernel: Extend audit logging for exec outcomes.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 063: Kernel: Implement persistent workspace snapshotting.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 064: DB: Migrate to Postgres with RLS policies.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 065: DB: Add key rotation audit trail.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 066: DB: Implement tests for namespace scoping.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 067: Daemon: Add SSE streaming for logs.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 068: Daemon: Enforce rate limits via middleware.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 069: Daemon: Expose metrics for sandbox lifecycle durations.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 070: Docs: Update roadmap milestone statuses.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 071: Kernel: Add seccomp profile for syscall filtering.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 072: Kernel: Extend audit logging for exec outcomes.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 073: Kernel: Implement persistent workspace snapshotting.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 074: DB: Migrate to Postgres with RLS policies.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 075: DB: Add key rotation audit trail.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 076: DB: Implement tests for namespace scoping.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 077: Daemon: Add SSE streaming for logs.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 078: Daemon: Enforce rate limits via middleware.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 079: Daemon: Expose metrics for sandbox lifecycle durations.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 080: Docs: Update roadmap milestone statuses.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 081: Kernel: Add seccomp profile for syscall filtering.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 082: Kernel: Extend audit logging for exec outcomes.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 083: Kernel: Implement persistent workspace snapshotting.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 084: DB: Migrate to Postgres with RLS policies.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 085: DB: Add key rotation audit trail.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 086: DB: Implement tests for namespace scoping.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 087: Daemon: Add SSE streaming for logs.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 088: Daemon: Enforce rate limits via middleware.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 089: Daemon: Expose metrics for sandbox lifecycle durations.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 090: Docs: Update roadmap milestone statuses.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 091: Kernel: Add seccomp profile for syscall filtering.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 092: Kernel: Extend audit logging for exec outcomes.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 093: Kernel: Implement persistent workspace snapshotting.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 094: DB: Migrate to Postgres with RLS policies.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 095: DB: Add key rotation audit trail.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 096: DB: Implement tests for namespace scoping.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 097: Daemon: Add SSE streaming for logs.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 098: Daemon: Enforce rate limits via middleware.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 099: Daemon: Expose metrics for sandbox lifecycle durations.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.
Scenario 100: Docs: Update roadmap milestone statuses.
    - Read relevant section in README and AGENTS.md before coding.
    - Draft plan with at least two steps; note dependencies.
    - Edit affected files via apply_patch; include comments where necessary.
    - Run targeted tests; record results in final summary.
    - Update Progress.md citing scenario number and file references.

## Glossary
- **CAVE** — Isolated sandbox instance with enforced resource and security policies.
- **Admin-CAVE** — Authoritative node responsible for model hosting, key issuance, and P2P replication.
- **Namespace** — Logical tenant segmentation used for sandbox scoping and key permissions.
- **MCP** — Management Control Protocol used for remote sandbox control via JSON-RPC.
- **SBOM** — Software Bill of Materials generated via make sbom and signed with cosign.
- **SLSA** — Supply-chain Level for Software Artifacts; produced via make slsa.

## Outstanding Decisions
- Define task routing logic for multi-agent orchestration once Phase-0 completes.
- Select secrets management solution (Vault, KMS, etc.) for cosign keys and API credentials.
- Establish escalation policy for sandbox failures and authorization issues.
- Document Postgres deployment strategy (Helm chart, operator) for production RLS roll-out.
- Define UI test automation tooling (Playwright, Cypress) and integrate into CI.

## Reinforcement Reminders
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 1)
- Always mention test status (run/not run, commands) in final hand-off. (pass 1)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 1)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 1)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 1)
- Avoid storing large listings in prompts; keep file.md concise. (pass 1)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 1)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 2)
- Always mention test status (run/not run, commands) in final hand-off. (pass 2)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 2)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 2)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 2)
- Avoid storing large listings in prompts; keep file.md concise. (pass 2)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 2)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 3)
- Always mention test status (run/not run, commands) in final hand-off. (pass 3)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 3)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 3)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 3)
- Avoid storing large listings in prompts; keep file.md concise. (pass 3)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 3)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 4)
- Always mention test status (run/not run, commands) in final hand-off. (pass 4)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 4)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 4)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 4)
- Avoid storing large listings in prompts; keep file.md concise. (pass 4)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 4)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 5)
- Always mention test status (run/not run, commands) in final hand-off. (pass 5)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 5)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 5)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 5)
- Avoid storing large listings in prompts; keep file.md concise. (pass 5)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 5)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 6)
- Always mention test status (run/not run, commands) in final hand-off. (pass 6)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 6)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 6)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 6)
- Avoid storing large listings in prompts; keep file.md concise. (pass 6)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 6)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 7)
- Always mention test status (run/not run, commands) in final hand-off. (pass 7)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 7)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 7)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 7)
- Avoid storing large listings in prompts; keep file.md concise. (pass 7)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 7)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 8)
- Always mention test status (run/not run, commands) in final hand-off. (pass 8)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 8)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 8)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 8)
- Avoid storing large listings in prompts; keep file.md concise. (pass 8)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 8)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 9)
- Always mention test status (run/not run, commands) in final hand-off. (pass 9)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 9)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 9)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 9)
- Avoid storing large listings in prompts; keep file.md concise. (pass 9)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 9)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 10)
- Always mention test status (run/not run, commands) in final hand-off. (pass 10)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 10)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 10)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 10)
- Avoid storing large listings in prompts; keep file.md concise. (pass 10)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 10)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 11)
- Always mention test status (run/not run, commands) in final hand-off. (pass 11)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 11)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 11)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 11)
- Avoid storing large listings in prompts; keep file.md concise. (pass 11)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 11)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 12)
- Always mention test status (run/not run, commands) in final hand-off. (pass 12)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 12)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 12)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 12)
- Avoid storing large listings in prompts; keep file.md concise. (pass 12)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 12)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 13)
- Always mention test status (run/not run, commands) in final hand-off. (pass 13)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 13)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 13)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 13)
- Avoid storing large listings in prompts; keep file.md concise. (pass 13)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 13)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 14)
- Always mention test status (run/not run, commands) in final hand-off. (pass 14)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 14)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 14)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 14)
- Avoid storing large listings in prompts; keep file.md concise. (pass 14)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 14)
- Keep README.md untouched unless performing coordinated version update with tests and signatures. (pass 15)
- Always mention test status (run/not run, commands) in final hand-off. (pass 15)
- Log new TODOs or blockers in docs/Progress.md Open Questions section. (pass 15)
- Use `git status -sb` to confirm only intended files changed before commit. (pass 15)
- Respect sandbox cleanup procedures; call sandbox.stop(). (pass 15)
- Avoid storing large listings in prompts; keep file.md concise. (pass 15)
- Sync AGENTS.md and PROMPT.md references whenever rules change. (pass 15)
