# Repository Guidelines

## Project Structure & Module Organization
- `crates/bkg-db`: persistence layer for sandbox metadata, migrations under `migrations/`.
- `crates/cave-kernel`: sandbox orchestration logic, isolation primitives in `src/isolation.rs`.
- `crates/cave-daemon`: HTTP/MCP entrypoint exposing `/api/v1` and `/healthz`.
- `docs/`: architecture, env references, roadmap; keep them in sync with `Progress.md`.
- `schema/cave.schema.json`: validation target for `cave.yaml`.

## Build, Test & Development Commands
- `cargo build` — compile the entire workspace (default members include `cave-daemon`).
- `cargo test` — run unit/integration tests for all crates.
- `cargo run -p cave-daemon` — launch the REST/MCP service (honors `BKG_DB_DSN` / `BKG_DB_PATH`).
- `cargo fmt --all && cargo clippy --all-targets --all-features` — enforce Rust formatting/lints before opening a PR.

## Coding Style & Naming Conventions
- Rust code follows `rustfmt` defaults (4 spaces, trailing commas, module files named `mod.rs` or `lib.rs`).
- Use `snake_case` for modules/functions, `CamelCase` for types, and `SCREAMING_SNAKE_CASE` for consts/env keys.
- Keep docs ASCII-only unless a document already uses UTF-8 (e.g., German README).
- Prefer targeted edits via `apply_patch`; avoid destructive Git commands.

## Testing Guidelines
- Place unit tests alongside modules using `#[cfg(test)]`; integration tests belong in `tests/`.
- Name test files after the feature under test (e.g., `sandbox_lifecycle.rs`).
- Future security checks must run via `pytest security/`; document any missing coverage in `Progress.md`.
- Validate schemas with `ajv validate -s schema/cave.schema.json -d cave.yaml` when config changes.

## Commit & Pull Request Guidelines
- Use imperative commit subjects (`Add sandbox audit log writer`); group logical changes per commit.
- Reference corresponding entries in `docs/FEATURE_ORIGINS.md` when adapting external ideas.
- Pull requests should include: scope description, testing evidence, updated docs/Progress links, and screenshots for UI work.
- Ensure CI passes (`cargo fmt`, `cargo clippy`, `cargo test`) before requesting review.

## Security & Configuration Tips
- Never commit secrets; rely on `BKG_API_KEY`/`BKG_DB_DSN` via environment or secret stores.
- Respect sandbox limits from `config/sandbox_config.toml`; request overrides through the Admin-Orchestrator.
- Keep telemetry aligned with `CAVE_OTEL_SAMPLING_RATE` to avoid noisy traces.
