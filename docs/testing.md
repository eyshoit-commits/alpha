# docs/testing.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

---

## Testpyramide
- **Unit Tests**: Rust `cargo test` (je Crate), Python `pytest` (für Security).  
- **Integration Tests**:  
  - Kernel Lifecycle (`crates/cave-kernel/tests`).  
  - Daemon API (`crates/cave-daemon/tests`).  
  - bkg-db SQL/RLS (`cargo test -p bkg-db`).  
- **End-to-End Tests**: Web-UIs (Playwright/Cypress), CLI Szenarien.  
- **Security Tests**: `pytest tests/security/` (Threat Matrix).
- **Load Tests**: k6/Gatling (Backlog).

---

## Laufzeiten & Commands
- `cargo fmt --all && cargo clippy -- -D warnings` (Pre-commit).  
- `cargo test --workspace` (Unit/Integration).  
- `SQLX_OFFLINE=true sqlx migrate run` (Migration Tests).  
- `npm test` (UI, sobald vorhanden).  
- `pytest tests/security/` (Security Suite).
- `make sbom && make slsa && cosign sign-blob` (Supply Chain).

---

## CI Konfiguration
Siehe `.github/workflows/ci.yml`:
1. `lint-test` – fmt, clippy, cargo test.
2. `api-schema` – `make api-schema`, `openapi-cli validate`, optional `ajv` gegen `cave.yaml`.
3. `security-tests` – `pytest tests/security/` für Threat-Matrix Verifikation.
4. `web-ui` – Next.js Lint/Build + Playwright.
5. `supply-chain` – `make sbom`, `make slsa`, optional cosign Signatur.

---

## Testdaten & Fixtures
- SQLite Fixtures (`tests/fixtures/sqlite.db`) – TODO hinzufügen.  
- Postgres Test Database (Docker Compose) – Backlog.  
- Sandbox Templates (`tests/fixtures/sandbox.yaml`).  
- Mock Telemetrie Exporter (`tests/mocks/otel`).

---

## Backlog
- [x] Security Test Suite implementieren (`tests/security/`).
- [ ] Integration Tests für Websocket Streaming.  
- [ ] CLI Snapshot Tests (assert_cmd).  
- [ ] Load/Stress Tests automatisieren (Nightly Pipeline).

---

SPDX-License-Identifier: Apache-2.0
