# docs/bkg-db.md

Version: 1.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

---

## Zweck
Dieser Leitfaden bündelt alle Architektur-, Workflow- und Governance-Vorgaben für **bkg-db** – das Supabase-inspirierte Daten-, Auth-, Storage- und Realtime-Backend der BKG-Plattform. Er ersetzt ältere Prompt-Varianten (`docs/PROMPT_BKG_DB.md`) und dient als maßgebliche Referenz für Coding-Agenten, Reviewer und Betreiber.

---

## Zielbild (Stack-Übersicht)

| Ebene | Funktion |
| --- | --- |
| **Kernel** | MVCC-Storage, Write-Ahead-Log (WAL), Checkpoints, Snapshot Isolation, Recovery |
| **SQL-Pipeline** | Parser → Planner → Executor (aktueller Stand: `INSERT`, `SELECT *` mit `WHERE`-Filtern, `COUNT(*)`, `UPDATE`, `DELETE`; geplante Erweiterungen: Joins, Aggregationen) |
| **Auth/RLS** | JWT-basierte Authentifizierung (HS256), JSON-basierte RLS-Policies, persistente API-Keys, Scopes (Admin/NAMESPACE) |
| **API-Layer** | PostgreSQL Wire-Protokoll (pgwire), HTTP/REST (`/query`, `/auth`, `/policy`, `/schema`), gRPC |
| **Realtime/CDC** | WAL-basierte Pub/Sub-Events via WebSocket oder SSE (`/realtime`) |
| **Objekt-Storage** | Buckets, presigned URLs, S3-kompatible Backends, RLS auf Objektebene |
| **Admin-UI** | Next.js Dashboard (`web/admin`) für Policies, Users, Telemetrie, Audit |
| **Telemetry/Audit** | OpenTelemetry Export, signierte JSONL-Logs (`cosign`), konfigurierbare Sampling-Rate |
| **CI/Supply Chain** | Make Targets (`lint`, `test`, `sbom`, `slsa`, `sign`), signierte Artefakte, OpenAPI-Validierung |

Status und Priorisierung der Deliverables werden zentral in `docs/Progress.md` und `docs/roadmap.md` gepflegt.

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

Der Code der Datenbank liegt in `./crates/bkg-db/`, Migrations in `crates/bkg-db/migrations/`, das Admin-Panel entsteht in `web/admin/`. Diese Struktur muss beibehalten werden, damit Builds, Tests und andere Agenten korrekt funktionieren. Details zur Gesamtstruktur stehen ergänzend in `file.md`.

---

## Feature- & Deliverables-Checkliste

| Kategorie | Kernfeatures |
| --- | --- |
| **DB-Kern** | MVCC, WAL, Snapshots, Recovery |
| **SQL-Layer** | Parser (sqlparser), Planner, Executor (Iter/Batch), geplante Optimizer |
| **Auth/RLS** | JWT-Issuer, Scope-Prüfung, JSON-Policies, persistente API-Keys |
| **API** | pgwire (Tokio), HTTP/REST (axum), gRPC (tonic) |
| **Realtime** | WAL-Tailer, Pub/Sub via WS/SSE |
| **Storage** | Pluggable Storage Layer (S3, lokale Files), RLS für Buckets |
| **Admin-UI** | Next.js Dashboard (Overview, Policies, Users, Telemetry, Audit) |
| **Telemetry/Audit** | tracing + OTEL, JSONL Audit-Logs mit cosign-Signatur |
| **CI/Supply Chain** | `make sbom`, `make slsa`, `cosign sign-blob`, OpenAPI-Validierung |

Diese Tabelle dient als CI-orientierte Prüfliste und spiegelt die Phase-0 Deliverables für bkg-db wider.

---

## Implementierungsrichtlinien (Clean-Room & Modularität)

- **Clean-Room** – Inspiration ist erlaubt (z. B. Supabase), aber Quellcode wird vollständig neu erstellt (Rust/TypeScript). Kein Copy&Paste externer Snippets.  
- **Modularität** – Die Module in `crates/bkg-db/src/` folgen klaren Verantwortlichkeiten (Kernel, SQL, Planner, Executor, RLS, Auth, API, Realtime, Storage, Telemetry, Audit). Traits/Interfaces sauber versionieren.  
- **Test-Driven** – Für Parser, Planner, Executor, RLS, API Schichten sowohl Unit- als auch Integrationstests anlegen (`cargo test`, optional Feature `postgres`). End-to-End Tests validieren komplette Request-Flows sowie Policy-Enforcement.  
- **OpenAPI & API-Stabilität** – Definiere REST-Endpunkte in `docs/api.md`; prüfe Schema in CI (`openapi-cli validate`). Breaking Changes müssen dokumentiert werden (Changelog-Sektion in diesem Dokument oder Release Notes).  
- **Telemetry & Audit** – Nutze `tracing` und `opentelemetry-otlp`. Audit-Logs als JSONL mit `cosign` signieren, Sampling via `CAVE_OTEL_SAMPLING_RATE` steuern.  
- **CI & Supply Chain** – Stelle Make Targets für `lint`, `test`, `sbom`, `slsa`, `sign`, `api-validate` bereit und verankere sie in der CI-Pipeline.

---

## Agenten- & Governance-Regeln

- `cargo build --workspace` muss vor jedem Commit fehlerfrei laufen.  
- API-Keys werden persistiert (`api_keys` Tabelle: Hash, Prefix, Scope, TTL) und über `cave-daemon` verwaltet.  
- Vor jeder Query sind RLS-Policies zu evaluieren; Auth-Zustand via JWT (HS256) prüfen.  
- Jede DDL/DML-Operation erzeugt ein signiertes Audit-Event (JSONL + cosign).  
- Sandbox-Limits & Telemetrie-Einstellungen aus `config/sandbox_config.toml` bzw. `CAVE_OTEL_SAMPLING_RATE` respektieren.  
- Befolge die übergeordneten Agenten-Regeln aus `AGENTS.md`.

---

## Workflow (Entwicklung & Tests)

```bash
# Build & Start
cargo build --workspace
cargo run -p bkg-db

# Migration anwenden
SQLX_OFFLINE=true sqlx migrate run
# oder (zukünftig) via CLI / REST-Endpunkt z. B. POST /migrate/apply

# Test Query
curl -X POST http://localhost:8080/query \
     -H "Authorization: Bearer <jwt>" \
     -d '{"sql":"SELECT * FROM projects;"}'

# Admin-UI local starten
cd web/admin && npm install && npm run dev
```

Weitere Schritte:
1. Implementiere `/query`, `/auth`, `/policy`, `/schema`, `/realtime` im HTTP-Layer; erweitere pgwire-Unterstützung für Simple/Extended Queries.  
2. Entwickle das Admin-UI mit Next.js + Tailwind/shadcn; binde es an REST/Realtime APIs.  
3. Teste Telemetrie & Audit (Log-Signaturen prüfen, Sampling anpassen).  
4. Pflege Make Targets (Lint, Test, SBOM/SLSA, Signaturen) und integriere sie in CI.  
5. Dokumentiere Fortschritt und offene Aufgaben in `docs/Progress.md`.

---

## CI-Targets (Beispiel-Makefile)

```makefile
lint:      cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings
test:      cargo test --workspace --all-features
sbom:      syft packages . -o json > sbom.json
slsa:      echo "slsa placeholder"
sign:      cosign sign-blob sbom.json --key cosign.key > sbom.sig
api-validate: openapi-cli validate openapi.yaml
```

Diese Targets sind Mindestanforderung; ergänze Scripte/Workflows je nach CI-Anbieter.

---

## Governance & Sicherheit

- JWT-Authentifizierung (HS256) und RLS-Prüfung sind Pflicht vor Query-Ausführung.  
- Auditpflicht: jede DDL/DML-Operation signiert; Audit-Schema versionieren.  
- Key-Scopes: ADMIN vs. NAMESPACE – halte Rate-Limits aus `docs/governance.md` ein.  
- Secrets (`cosign.key`, API-Keys, JWT-Secrets) nur in gesicherten Stores (Vault, Secret Manager).  
- Phase-0 Gate: Multi-Agent-Features erst nach erfolgreicher bkg-db Abnahme aktivieren.  
- Telemetrie: wähle Sampling-Raten pro Umgebung (Dev 1.0, Staging ≈0.5, Prod 0.05–0.2).

---

## Offene Lieferobjekte (Roadmap-Integration)

1. **Kernel & Storage** – MVCC, WAL, Checkpoints, Crash-Recovery.  
2. **SQL-Schichten** – Parser, Planner, Optimizer, Executor (mit Joins/Aggregationen).  
3. **Auth & RLS** – Erweiterte Policy Engine, persistente Storage, Admin APIs für Policy Management.  
4. **API & Protokolle** – Vollständige HTTP-/pgwire-/gRPC-Unterstützung inklusive Schema-Validierung.  
5. **Realtime/CDC** – WAL Listener, Broadcast Hub, Client SDKs.  
6. **Objekt-Storage** – Bucket Verwaltung, presigned URLs, Backend-Abstraktion.  
7. **Admin-UI** – Next.js App (Overview, Policies, Users, Telemetry, Audit).  
8. **Telemetry & Audit** – OTEL Export, cosign-signierte Logs mit Rotation.  
9. **CI & Supply Chain** – Implementierte Make Targets, SBOM/SLSA, cosign Signierung, OpenAPI-Checks.

Der Fortschritt wird kontinuierlich in `docs/Progress.md` (Status) und `docs/roadmap.md` (Planung) gespiegelt.

---

## Referenzen
- `docs/Progress.md` – aktueller Status, offene Fragen, Verantwortliche.  
- `docs/roadmap.md` – Reihenfolge und Abhängigkeiten der Deliverables.  
- `docs/api.md`, `docs/security.md`, `docs/governance.md`, `docs/operations.md`, `docs/testing.md` – weitere Detailrichtlinien.  
- `AGENTS.md` & `PROMPT.md` – allgemeine Arbeitsweise, Tests, Cleanup.

---

SPDX-License-Identifier: Apache-2.0
