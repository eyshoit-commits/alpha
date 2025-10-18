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
- **Security Tests**: `pytest security/` (Threat Matrix).  
- **Load Tests**: k6/Gatling (Backlog).

---

## Laufzeiten & Commands
- `cargo fmt --all && cargo clippy -- -D warnings` (Pre-commit).  
- `cargo test --workspace` (Unit/Integration).  
- `SQLX_OFFLINE=true sqlx migrate run` (Migration Tests).  
- `npm test` (UI, sobald vorhanden).  
- `pytest security/` (Security Suite).  
- `make sbom && make slsa && cosign sign-blob` (Supply Chain).

---

## CI Konfiguration
Siehe `.github/workflows/ci.yml`:
1. `lint-test` – fmt, clippy, cargo test.  
2. `schema-and-security` – `ajv validate`, Security Tests (TODO).  
3. `supply-chain` – SBOM, SLSA, cosign Signatur (Schlüssel über Secrets).

---

## Testdaten & Fixtures
- SQLite Fixtures (`tests/fixtures/sqlite.db`) – TODO hinzufügen.  
- Postgres Test Database (Docker Compose) – Backlog.  
- Sandbox Templates (`tests/fixtures/sandbox.yaml`).  
- Mock Telemetrie Exporter (`tests/mocks/otel`).

---

## Backlog
- [ ] Security Test Suite implementieren (`security/`).  
- [ ] Integration Tests für Websocket Streaming.  
- [ ] CLI Snapshot Tests (assert_cmd).  
- [ ] Load/Stress Tests automatisieren (Nightly Pipeline).

---

SPDX-License-Identifier: Apache-2.0
