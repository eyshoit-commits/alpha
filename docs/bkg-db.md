# docs/bkg-db.md

Version: 1.0  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

---

## Zweck
Dieser Leitfaden definiert die vollständige Entwicklungsroadmap für **bkg-db** – das Daten-, Auth-, Storage- und Realtime-Backend der BKG-Plattform. Die Zielarchitektur orientiert sich an Supabase, wird jedoch vollständig innerhalb der BKG Microsandbox implementiert und betrieben. Grundlage ist das Target-Verzeichnis `./crates/bkg-db` sowie die angeschlossenen Frontend-, API- und Operations-Komponenten.

---

## Zielbild (Stack-Übersicht)

| Ebene | Funktion |
| --- | --- |
| **Kernel** | MVCC-Storage, Write-Ahead-Log (WAL), Checkpoints, Snapshot Isolation |
| **SQL-Pipeline** | Parser → Planner → Executor (aktuell INSERT, SELECT * mit WHERE (AND/OR, >,<,=), SELECT COUNT(*), UPDATE & DELETE) |
| **Auth/RLS** | JWT-basierte Authentifizierung (HS256), Row-Level-Security Engine mit EQ/AND/OR Policies, persistente API-Keys |
| **API-Layer** | PostgreSQL Wire-Protokoll, HTTP (REST), gRPC |
| **Realtime/CDC** | WAL-basierte Subscriptions via WebSocket |
| **Objekt-Storage** | Buckets, presigned URLs, S3-kompatible Backends |
| **Admin-UI** | Web-Dashboard (`web/admin`) für Policies, Users, Telemetrie |
| **Telemetry/Audit** | OpenTelemetry Export, signierte JSONL-Logs (`cosign`) |
| **CI/Supply Chain** | SBOM, SLSA, `make lint/test/sbom/slsa/sign` |

---

## Repository-Struktur (Soll)

```text
./
├─ Cargo.toml
├─ crates/
│  ├─ cave-daemon/
│  ├─ cave-kernel/
│  └─ bkg-db/
│     ├─ Cargo.toml
│     ├─ migrations/
│     │  └─ 0001_init.sql
│     └─ src/
│        ├─ lib.rs
│        ├─ kernel.rs
│        ├─ sql.rs
│        ├─ planner.rs
│        ├─ executor.rs
│        ├─ rls.rs
│        ├─ auth.rs
│        ├─ api.rs
│        ├─ realtime.rs
│        ├─ storage.rs
│        ├─ telemetry.rs
│        └─ audit.rs
├─ web/
│  └─ admin/
│     ├─ package.json
│     ├─ pages/
│     └─ components/
└─ docs/
   ├─ bkg-db.md
   ├─ governance.md
   ├─ operations.md
   ├─ security.md
   ├─ api.md
   └─ testing.md
```

---

## Agent-Instruktionen & Clean-Room-Regeln

- **Build-Kompatibilität:** Jeder Commit muss mit `cargo build --workspace` erfolgreich durchlaufen.
- **Clean-Room:** Kein Copy/Paste aus externen Projekten; sämtliche Implementierungen (Kernel, SQL-Pipeline, API-Layer etc.) werden eigenständig erstellt.
- **RLS-Policies:** Vor jeder Query sind Row-Level-Security-Policies zu evaluieren.
- **Audit-Events:** DDL/DML-Operationen werden als JSON Lines protokolliert und mit `cosign` signiert.
- **API-Key Persistenz:** Ausgestellte Keys werden in `api_keys` (hashed Token, Prefix, Scope, TTL) gespeichert und aus `cave-daemon` via `bkg_db::Database` verwaltet.
- **Telemetry:** `CAVE_OTEL_SAMPLING_RATE` respektieren (Dev 1.0 / Prod 0.05).

---

## API-Integration

- **HTTP (axum):** Endpunkte `/query`, `/auth`, `/policy`, `/schema`.
- **pgwire (Tokio):** PostgreSQL Wire-Protokoll mit Basic Auth, Simple Queries, optional Extended Flow.
- **Realtime:** WebSocket/SSE unter `/realtime`, getrieben durch WAL-Events.
- **Admin-UI:** React/Next.js Dashboard (`web/admin`) mit Tabs *Overview · Policies · Users · Telemetry · Audit*. Verbindung via REST + WebSocket.

---

## Telemetry & Audit

- `tracing` + `opentelemetry-otlp` für Metriken und Traces.
- Audit-Log als signierte JSON Lines (cosign) mit versionierter Schema-Datei.
- Rate-Limits & RBAC wie in `docs/governance.md` beschrieben durchsetzen.

---

## Beispiel-Workflow

```bash
# Build & Start
cargo build --workspace
cargo run -p bkg-db

# Apply Migration
bkg-cli migrate apply

# Test Query
curl -X POST http://localhost:8080/query \
     -H "Authorization: Bearer <jwt>" \
     -d '{"sql":"SELECT * FROM projects;"}'

# Start Admin-UI
cd web/admin && npm run dev
```

---

## CI-Targets (Makefile)

```makefile
lint:      cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings
test:      cargo test --workspace --all-features
sbom:      syft packages . -o json > sbom.json
slsa:      echo "slsa placeholder"
sign:      cosign sign-blob sbom.json --key cosign.key > sbom.sig
api-validate: openapi-cli validate openapi.yaml
```

---

## Governance & Sicherheit

- JWT-gestützte Authentifizierung, RLS-Pflichten vor Query-Execution.
- Auditpflicht: jede DDL/DML-Operation wird signiert und versioniert.
- Key-Scopes: ADMIN / NAMESPACE (Rate-Limits aus `docs/governance.md`).
- Secrets (`cosign.key`, API-Keys, JWT-Secrets) sind im Vault abzulegen.
- Phase-0 Gate: Multi-Agent-Features erst nach bkg-db Phase-0 Abnahme aktivieren.

---

## Offene Lieferobjekte

1. **Kernel & Storage** – MVCC, WAL, Checkpoints, Recovery.
2. **SQL-Schichten** – Parser, Planner, Optimizer, Executor.
3. **RLS & Auth** – Policy Engine, JWT-Issuer, Admin API.
4. **API & Protokolle** – HTTP (`/query`), pgwire, gRPC (Schema-first).
5. **Realtime/CDC** – WAL Listener, Broadcast Hub, Client SDK-Schnittstellen.
6. **Objekt-Storage** – Bucket Management, presigned URLs, Backend Abstraction.
7. **Admin-UI** – Next.js App mit Telemetrie-, Policy-, User- und Audit-Ansichten.
8. **Telemetry & Audit** – OTEL Export, cosign-signierte JSONL Logs inkl. Rotation.
9. **CI & Supply Chain** – Make Targets, SBOM/SLSA Artefakte, cosign Signing.

Status und Priorisierung werden in `docs/Progress.md` und `docs/roadmap.md` geführt.

SPDX-License-Identifier: Apache-2.0
