# Progress Tracker

Zuletzt synchronisiert mit `README.md` v1.8.2 und `PROMPT.md` (Commit-Stand 2025-10-18).
Review-Check am 2025-10-19: README §"Tests, CI & Release Artefakte" sowie PROMPT §"Standard Workflow" wurden gegen diese Notizen abgeglichen.

## Aktueller Status (Stand 2025-10-19)
- Phase-0 Komponenten sind teilweise implementiert: Das Prozess-Shim spannt jetzt Overlay-Workspaces und eine konfigurierbare Seccomp-BPF-Filterung auf, inklusive Linux-Tests für geblockte Sockets sowie nicht-persistente Schreibversuche (`crates/cave-kernel/src/lib.rs:624`, `crates/cave-kernel/src/lib.rs:1320-1455`). Namespace-Hardening via Bubblewrap bleibt offen.
- Persistenz läuft aktuell über eine SQLite-Anbindung mit `bkg_db::Database`, die API-Schlüssel, Rotationen und Nutzungs-Timestamps dauerhaft speichert (`crates/cave-daemon/src/auth.rs:66-160`, `crates/bkg-db/src/lib.rs:45-228`). Postgres-Migrationen liefern nun Namespaces-, Schlüssel- und Policy-Seeds inklusive JSONB-Handling (`crates/bkg-db/migrations_postgres/0005_namespaces.sql`, `0007_seed_api_keys.sql`, `0008_seed_rls_policies.sql`); der Daemon erwartet dafür `BKG_DB_DSN`/`DATABASE_URL` statt SQLite-Pfaden (`crates/cave-daemon/src/server.rs:109-132`).
- Die erwarteten Web-UIs (`web/admin`, `web/app`) liegen nun als Next.js-Apps mit gemeinsamem API-Client und Navigation für Lifecycle/Telemetry vor (`web/admin/src/app`, `web/app/src/app`). SSR-Guards schützen die Dashboards jetzt via Cookie-basierten Token-Gates samt CSP-Hardening (`web/admin/middleware.ts`, `web/app/middleware.ts`), und das Admin-Portal bringt zusätzliche Modelle- & Audit-Ansichten inklusive Schlüsselrotation mit Webhook-Bestätigung (`web/admin/src/app/(dashboard)/models/page.tsx`, `web/admin/src/app/(dashboard)/audit/page.tsx`, `web/admin/src/app/(dashboard)/keys/page.tsx`).
- Dokumentation ist nur für Architektur, ENV-Variablen und Agentenleitfaden vorhanden; übrige Pflichtdokumente fehlen (`docs/architecture.md:13`, `docs/env.md:1`, `AGENTS.md:1`).
- Build-/CI-Setup ist aktiv: `Makefile` liefert `make api-schema` für OpenAPI-Generierung (`Makefile:1-6`), und `.github/workflows/ci.yml` führt Formatierung, Linting, Tests, Schema-Checks sowie Supply-Chain-Schritte aus (`.github/workflows/ci.yml:1-196`). Playwright-E2E-Tests laufen in der `web-ui`-Jobkette über `npm run test:e2e` (`.github/workflows/ci.yml:107-147`). Offene Punkte: SBOM-Signaturen benötigen Secrets, zusätzliche Security-Cases sind zu ergänzen (`.github/workflows/ci.yml:149-196`).
- Governance-Themen wie Rotations-Webhook und Audit-Log-Streaming fehlen weiterhin; die Telemetrie-Policy wird inzwischen über `CAVE_OTEL_SAMPLING_RATE` im Daemon ausgewertet (`crates/cave-daemon/src/main.rs:48`).

## Phase-0 Verpflichtungen
- [ ] CAVE-Kernel & Sandbox Runtime (Namespaces, cgroups v2, seccomp, FS-Overlay) produktionsreif mit Integrationstests deployt.  
  Status: Kern-API existiert, Isolation ist ein Prozess-Shim ohne Low-Level-Schutz & Integrationstests (`crates/cave-kernel/src/lib.rs:1`). Ein neuer HTTP-Lifecycle-Test orchestriert `create/start/exec/stop` über den Daemon und prüft Status-/Execution-Persistenz gegen SQLite (`crates/cave-daemon/src/server.rs`); Low-Level-Isolation & Seccomp fehlen weiterhin.
- [ ] Persistente `bkg_db` mit Row-Level-Security betriebsbereit und angebunden.
  Status: SQLite-Backed Prototyp speichert API-Keys und RLS-Policies inkl. WAL-Recovery (`crates/bkg-db/src/lib.rs:169`, `crates/bkg-db/src/executor.rs:44`). Postgres-Migrationen liefern Seeds & Policies (`crates/bkg-db/migrations_postgres/0005-0008`), Planner/Executor-Tests prüfen Namespace-RLS (`crates/bkg-db/src/executor.rs:720-808`); Service-Wiring zum Daemon & produktive PG-Deployments bleiben offen (`docs/architecture.md:16`).
- [ ] Web-UI (admin & user) mit Minimalfunktionen live; Phasenabschluss dokumentiert.
  Status: Next.js Admin- & Namespace-Portale vorhanden (`web/admin`, `web/app`) inkl. Telemetrie-/Lifecycle-Views und Playwright-E2E-Tests; produktionsnahe Styling/SSR-Validierung & Backend-Anbindung mit echten Tokens stehen weiter aus.

> Hinweis: Ohne abgeschlossene Phase-0 keine Aktivierung von P2P, Distributed Inference, Marketplace oder Multi-Agent-Features.

## Dokumentation & Templates
- [x] `docs/security.md` erstellt (Threat-Matrix, CI-Hinweis auf `pytest tests/security/`).
  Status: Erstfassung vorhanden; Tests & weitere Szenarien ergänzen.
- [x] Fehlende Pflichtdokumente ergänzt:  
  `docs/api.md`, `docs/cli.md`, `docs/deployment.md`, `docs/operations.md`, `docs/testing.md`, `docs/governance.md`, `docs/compatibility.md`.  
  Status: Skeletons vorhanden, müssen inhaltlich ausgebaut werden (markiert in Backlog).
- [x] `AGENTS.md` angelegt (zentrales Agenten-Playbook); Cross-Linking im Repo aktuell (`AGENTS.md:1`, `docs/architecture.md:11`, `PROMPT.md:63`).
- [x] `docs/bkg-db.md` aktualisiert; enthält nun den vollständigen Supabase/BKG-DB Prompt & Roadmap (ersetzt `docs/PROMPT_BKG_DB.md`).
- [x] `docs/roadmap.md` hinzugefügt (Phase-0 Roadmap & Priorisierung); bei Status-Updates als Referenz nutzen.
- [ ] `docs/FEATURE_ORIGINS.md` von Draft auf vollständige Einträge mit Commit/PR-Referenzen & Reviewer-Signoff erweitern.  
  Status: Nur Draft-Einträge ohne Commits/Signoff (`docs/FEATURE_ORIGINS.md:18`).
- [ ] `docs/architecture.md`, `docs/env.md`, vorhandene Dokumente auf Konsistenz mit v1.8.2 prüfen.  
  Status: Dokumente existieren, Review ausstehend (`docs/architecture.md:1`, `docs/env.md:1`).

## CI, Tests & Artefakte
- [x] `make api-schema` in CI einbinden und `openapi-cli validate openapi.yaml` ausführen.
  Status: `api-schema` Job ruft `make api-schema`, prüft via `git diff` und validiert das Ergebnis mit `openapi-cli validate` (`.github/workflows/ci.yml:46-74`).
- [x] `cave.yaml` Validierung im CI sicherstellen (`ajv validate -s schema/cave.schema.json -d cave.yaml`).
  Status: ajv-Validierung in CI vorhanden (überspringt, wenn `cave.yaml` fehlt).
- [x] UI-E2E-Tests (Playwright) in CI einbinden.
  Status: `web-ui` Workflow-Job lintet/buildet beide Next.js-Apps und führt Playwright-Mocks der `/api/v1`-Flows aus (`.github/workflows/ci.yml`). Zusätzliche Szenarien decken jetzt Schlüsselrotation, Modellverwaltung und Audit-Filter ab, der CI-Job lädt den HTML-Report als Artefakt hoch.
- [ ] SBOM/SLSA Pipeline komplettieren: `make sbom`, `make slsa`, `cosign sign-blob <SBOM> --key cosign.key`; Secrets-Management dokumentieren.
  Status: `supply-chain` Job ruft `make sbom`/`make slsa` auf und lädt Artefakte hoch; cosign Signatur weiterhin optional (`.github/workflows/ci.yml:149-196`).
- [x] SQLite-Migrationen für Test-Harness idempotent absichern.
  Status: `Database::connect` toleriert doppelt ausgeführte Einträge in `_sqlx_migrations` (SQLite-Codes `1555`/`2067`), wodurch die Rotationstests grün laufen (`crates/bkg-db/src/lib.rs:65-76`, `crates/cave-daemon/src/main.rs:1025-1165`).
- [x] Threat-Matrix Tests (`pytest tests/security/`) verpflichtend machen.
  Status: `security-tests` Job setzt `pytest tests/security` um; weitere Szenarien folgen (`.github/workflows/ci.yml:83-106`).

## Governance & Betrieb
- [x] Schlüssel-Rotation und Webhook-Handling implementieren (inkl. HMAC-Signaturprüfung, Audit-Logging).
  Status: Admin-Rotation (`POST /api/v1/auth/keys/rotate`) erstellt ein neues Token, markiert das alte als `revoked`, persistiert `rotated_from`/`rotated_at` und legt das HMAC-signierte Webhook-Event in der Outbox ab; `/api/v1/auth/keys/rotated` prüft den Header `X-Cave-Webhook-Signature` (`crates/cave-daemon/src/main.rs:147-210`, `crates/cave-daemon/src/auth.rs:57-310`, `crates/bkg-db/src/lib.rs:90-220`).
- [x] API-Schlüssel persistent speichern (SQLite/Postgres) statt ausschließlich In-Memory, damit Restarts keinen Re-Issue erfordern.
  Status: `AuthService` nutzt `bkg_db::Database` für Ausgabe, Rotation und Nutzungstracking (Hashing, Revocation, Touch) über SQLite (`crates/cave-daemon/src/auth.rs:66-210`, `crates/bkg-db/src/lib.rs:45-228`). Follow-up: Postgres-Migration & Verschlüsselungsstrategie definieren (`docs/architecture.md:16`).
- [x] RBAC & Rate-Limits im Gateway konfigurieren (Admin 1000/min, Namespace 100/min, Session 50/min, Model-Access 200/min).
  Status: Middleware-basierte Limits erzwingen die Vorgaben für Admin/Namespace/Session-Routen (`crates/cave-daemon/src/middleware/rate_limit.rs`), inklusive Tests für Klassifizierung & Kontingentverbrauch.
- [x] Telemetrie-Policy einführen: `CAVE_OTEL_SAMPLING_RATE` pro Umgebung abstimmen und monitoren.
  Status: `telemetry::init` initialisiert OTLP-Export mit Sampling & Graceful-Fallback (`crates/cave-daemon/src/telemetry.rs`), Dokumentation in `docs/telemetry.md` ergänzt.
- [x] Audit-Log Format (signierte JSON-Lines) implementieren und überprüfen.
  Status: `AuditLogWriter` schreibt HMAC-signierte JSONL und ist durch Tests abgesichert (`crates/cave-kernel/src/audit.rs:200-257`).
- [ ] Seccomp Profile und erweiterte Namespace-Isolation integrieren, um Bubblewrap-Fallback vollständig zu ersetzen.  
  Status: Der Prozess-Runtime setzt OverlayFS + Seccomp-Allowlist ohne Bubblewrap um (`crates/cave-kernel/src/lib.rs:660-1043`); fertige Bubblewrap-Profile zur Namespace-Härtung müssen noch ergänzt werden.
- [x] Sandbox-Defaultlimits final abnehmen (README & `config/sandbox_config.toml` jetzt auf 2 vCPU / 1 GiB / 120 s / 1 GiB Disk, Overrides erlaubt).  
  Status: Werte synchronisiert; Governance-Team hat Freigabe erteilt.

## BKG-DB Voll-Stack Aufbau
- [ ] Kernel & Storage: MVCC, WAL, Checkpoints, Crash-Recovery.  
  Status: In-Memory Prototype (`InMemoryStorageEngine`) mit WAL-Staging & Tests vorhanden (`crates/bkg-db/src/kernel.rs`); durable WAL/Checkpoints & Recovery stehen aus.
- [ ] SQL-Pipeline (Parser → Planner → Executor) mit SQL92-Kompatibilität.  
  Status: Parser (sqlparser), Planner und Executor unterstützen `INSERT`, `SELECT *` mit `WHERE`-Filtern (AND/OR, Vergleichs-Operatoren), `SELECT COUNT(*)`, sowie `UPDATE`/`DELETE` inkl. WAL-Logging (`crates/bkg-db/src/sql.rs`, `planner.rs`, `executor.rs`). Joins, Aggregationen jenseits von COUNT(*) und komplexere Optimierungen sind offen.
- [ ] Auth/RLS: JWT-Issuer, Policy Engine, Row-Level Security Evaluator.
  Status: HMAC-basierter JWT Issuer/Validator (`JwtHmacAuth`) implementiert; `DatabasePolicyEngine` lädt Postgres-Policies und enforced Claims im Planner/Executor (`crates/bkg-db/src/rls.rs`, `crates/bkg-db/src/executor.rs`, `crates/bkg-db/src/api.rs`). `EmbeddedRestApi` validiert `issue`/`verify`-Payloads strikt gegen aktive API-Keys, inklusive kanonisiertem Namespace-Scope-Abgleich für prefixed/unprefixed Werte (`crates/bkg-db/src/api.rs:188-237`) und aktualisiert Nutzungsmetadaten samt Fehlerpfad-Tests (`crates/bkg-db/src/api.rs:384-908`). Erweiterte Expressions, Admin-UI Hooks und Daemon-Wiring stehen aus.
- [ ] Postgres/RLS Migration entwerfen (Wechsel von SQLite-Prototyp zu Postgres mit Policies & Seeds).  
  Status: Konzept ausstehend; Migration-Tooling/Docs fehlen (`docs/bkg-db.md`).
- [ ] API-Layer: HTTP (`/query`, `/auth`, `/policy`, `/schema`), pgwire, gRPC.  
  Status: Nur Platzhalter-Module vorgesehen, keine Server-Implementierung (`crates/bkg-db`).
- [ ] Realtime/CDC: WAL-basierte Subscriptions via WebSocket/SSE.  
  Status: Kein Realtime-Hub implementiert.
- [ ] Objekt-Storage: Buckets, presigned URLs, Backend-Abstraktion.  
  Status: Nicht gestartet (`storage.rs` fehlt).
- [ ] Admin-UI (`web/admin`): Next.js Dashboard mit Tabs *Overview · Policies · Users · Telemetry · Audit*.
  Status: Next.js Dashboard mit Lifecycle-/Key-/Telemetry-Ansichten umgesetzt (`web/admin/src/app`); Audit-Tabs & tiefe Integration mit produktiven Backends fehlen.
- [ ] Telemetry & Audit: OTEL-Export, cosign-signierte JSONL-Logs.  
  Status: Keine Module (`telemetry.rs`, `audit.rs`) vorhanden.
- [ ] CI & Supply Chain: Make Targets (`lint`, `test`, `sbom`, `slsa`, `sign`, `api-validate`) und pipeline scripts.  
  Status: Makefile/CI-Konfiguration nicht vorhanden; `docs/bkg-db.md` definiert Zielzustand.

## Offene Fragen / Klärungsbedarf
- Wer verantwortet Phase-0 Abnahme und Dokumentation?
- Welche Secrets-Management-Lösung wird für `cosign.key` und API-Keys genutzt?
- Status der Clean-Room Vorgaben für neue Adapter/Bindings?
