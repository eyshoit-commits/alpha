# Progress Tracker

Zuletzt synchronisiert mit `README.md` v1.8.2.

## Aktueller Status (Stand 2025-10-18)
- Phase-0 Komponenten sind teilweise implementiert: Der Kernel nutzt ein Prozess-basiertes Isolation-Shim, tiefe Namespace/Seccomp-Logik und Tests fehlen noch (`crates/cave-kernel/src/lib.rs:1`).
- Persistenz läuft aktuell über eine SQLite-Anbindung; die in der Architektur geforderte Postgres/RLS-Konfiguration ist noch offen (`crates/bkg-db/src/lib.rs:3`, `docs/architecture.md:16`).
- Die erwarteten Web-UIs (`web/admin`, `web/app`) sind noch nicht eingecheckt (`docs/architecture.md:19`).
- Dokumentation deckt nun API-/CLI-Details ab: realistische Beispiele und Fehlerverträge in `docs/api.md` und `docs/cli.md` referenzieren `openapi.yaml` (`docs/api.md:1`, `docs/cli.md:1`).
- CI-Pipeline führt Format/Lint/Test, Schema-Generierung (`make api-schema`) sowie SBOM-Erstellung aus; `make slsa` bleibt Platzhalter und Cosign-Signing läuft nur bei vorhandenen Secrets (`.github/workflows/ci.yml:1`).
- Governance-Themen wie Rotations-Webhook und Audit-Log-Streaming fehlen weiterhin; die Telemetrie-Policy wird inzwischen über `CAVE_OTEL_SAMPLING_RATE` im Daemon ausgewertet (`crates/cave-daemon/src/server.rs`).

## Phase-0 Verpflichtungen
- [ ] CAVE-Kernel & Sandbox Runtime (Namespaces, cgroups v2, seccomp, FS-Overlay) produktionsreif mit Integrationstests deployt.  
  Status: Kern-API existiert, Isolation ist ein Prozess-Shim ohne Low-Level-Schutz & Integrationstests (`crates/cave-kernel/src/lib.rs:1`). Ein neuer HTTP-Lifecycle-Test orchestriert `create/start/exec/stop` über den Daemon und prüft Status-/Execution-Persistenz gegen SQLite (`crates/cave-daemon/src/server.rs`); Low-Level-Isolation & Seccomp fehlen weiterhin.
- [ ] Persistente `bkg_db` mit Row-Level-Security betriebsbereit und angebunden.  
  Status: SQLite-Backed Prototyp speichert API-Keys und RLS-Policies inkl. WAL-Recovery (`crates/bkg-db/src/lib.rs:169`, `crates/bkg-db/src/executor.rs:44`); Postgres-Pool & Service-Wiring stehen weiterhin aus (`docs/architecture.md:16`).
- [ ] Web-UI (admin & user) mit Minimalfunktionen live; Phasenabschluss dokumentiert.  
  Status: Noch keine Web-UI-Struktur im Repo (`docs/architecture.md:19`).

> Hinweis: Ohne abgeschlossene Phase-0 keine Aktivierung von P2P, Distributed Inference, Marketplace oder Multi-Agent-Features.

## Dokumentation & Templates
- [x] `docs/security.md` erstellt (Threat-Matrix, CI-Hinweis auf `pytest security/`).  
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
  Status: Schema-Generierung erfolgt über `cargo run --bin export-openapi` (utoipa-basierte Handler-Annotationen in `crates/cave-daemon/src/server.rs`), CI ruft Target & Validator auf und stellt sicher, dass das Schema eingecheckt bleibt (`.github/workflows/ci.yml:33`).
- [x] `cave.yaml` Validierung im CI sicherstellen (`ajv validate -s schema/cave.schema.json -d cave.yaml`).  
  Status: ajv-Validierung in CI vorhanden (überspringt, wenn `cave.yaml` fehlt).
- [ ] SBOM/SLSA Pipeline komplettieren: `make sbom`, `make slsa`, `cosign sign-blob <SBOM> --key cosign.key`; Secrets-Management dokumentieren.  
  Status: Workflow generiert SBOM/SLSA Placeholder + cosign Schritt (erfordert Schlüssel); Dokumentation in `docs/governance.md`.
- [ ] Threat-Matrix Tests (`pytest security/`) verpflichtend machen.  
  Status: Testsuite nicht vorhanden.

## Governance & Betrieb
- [ ] Schlüssel-Rotation und Webhook-Handling implementieren (inkl. HMAC-Signaturprüfung, Audit-Logging).  
  Status: AuthService unterstützt Issue/List/Revoke, Rotation/Webhooks fehlen (`crates/cave-daemon/src/auth.rs:68`).
- [ ] API-Schlüssel persistent speichern (SQLite/Postgres) statt ausschließlich In-Memory, damit Restarts keinen Re-Issue erfordern.  
  Status: AuthService hält Keys nur im Speicher (`crates/cave-daemon/src/auth.rs:52`). Datenmodell & Migration in `bkg_db` anlegen.
- [ ] RBAC & Rate-Limits im Gateway konfigurieren (Admin 1000/min, Namespace 100/min, Session 50/min, Model-Access 200/min).  
  Status: Rate-Limits existieren nur als Metadaten in `KeyInfo`, keine Durchsetzung (`crates/cave-daemon/src/auth.rs:29`).
- [ ] Telemetrie-Policy einführen: `CAVE_OTEL_SAMPLING_RATE` pro Umgebung abstimmen und monitoren.
  Status: `cave-daemon` respektiert das Sampling über `CAVE_OTEL_SAMPLING_RATE` und clamp't ungültige Werte (`crates/cave-daemon/src/server.rs`); OTEL-Exporter & Monitoring fehlen weiterhin.
- [ ] Audit-Log Format (signierte JSON-Lines) implementieren und überprüfen.  
  Status: Keine Audit-Log-Writer implementiert.
- [ ] Seccomp Profile und erweiterte Namespace-Isolation integrieren, um Bubblewrap-Fallback vollständig zu ersetzen.  
  Status: ProcessSandboxRuntime nutzt optional Bubblewrap, Seccomp/hardening fehlen (`crates/cave-kernel/src/lib.rs:425`).
- [x] Sandbox-Defaultlimits final abnehmen (README & `config/sandbox_config.toml` jetzt auf 2 vCPU / 1 GiB / 120 s / 1 GiB Disk, Overrides erlaubt).  
  Status: Werte synchronisiert; Governance-Team hat Freigabe erteilt.

## BKG-DB Voll-Stack Aufbau
- [ ] Kernel & Storage: MVCC, WAL, Checkpoints, Crash-Recovery.  
  Status: In-Memory Prototype (`InMemoryStorageEngine`) mit WAL-Staging & Tests vorhanden (`crates/bkg-db/src/kernel.rs`); durable WAL/Checkpoints & Recovery stehen aus.
- [ ] SQL-Pipeline (Parser → Planner → Executor) mit SQL92-Kompatibilität.  
  Status: Parser (sqlparser), Planner und Executor unterstützen `INSERT`, `SELECT *` mit `WHERE`-Filtern (AND/OR, Vergleichs-Operatoren), `SELECT COUNT(*)`, sowie `UPDATE`/`DELETE` inkl. WAL-Logging (`crates/bkg-db/src/sql.rs`, `planner.rs`, `executor.rs`). Joins, Aggregationen jenseits von COUNT(*) und komplexere Optimierungen sind offen.
- [ ] Auth/RLS: JWT-Issuer, Policy Engine, Row-Level Security Evaluator.  
  Status: HMAC-basierter JWT Issuer/Validator (`JwtHmacAuth`) implementiert; In-Memory RLS Policy Engine unterstützt einfache EQ/AND/OR Expressions (`crates/bkg-db/src/auth.rs`, `rls.rs`). Persistente Policy-Speicherung & erweiterte Claims/Expressions stehen aus.
- [ ] Postgres/RLS Migration entwerfen (Wechsel von SQLite-Prototyp zu Postgres mit Policies & Seeds).  
  Status: Konzept ausstehend; Migration-Tooling/Docs fehlen (`docs/bkg-db.md`).
- [ ] API-Layer: HTTP (`/query`, `/auth`, `/policy`, `/schema`), pgwire, gRPC.  
  Status: Nur Platzhalter-Module vorgesehen, keine Server-Implementierung (`crates/bkg-db`).
- [ ] Realtime/CDC: WAL-basierte Subscriptions via WebSocket/SSE.  
  Status: Kein Realtime-Hub implementiert.
- [ ] Objekt-Storage: Buckets, presigned URLs, Backend-Abstraktion.  
  Status: Nicht gestartet (`storage.rs` fehlt).
- [ ] Admin-UI (`web/admin`): Next.js Dashboard mit Tabs *Overview · Policies · Users · Telemetry · Audit*.  
  Status: Stub (`web/admin/README.md`, `package.json`) vorhanden; echte Next.js Implementierung ausstehend.
- [ ] Telemetry & Audit: OTEL-Export, cosign-signierte JSONL-Logs.  
  Status: Keine Module (`telemetry.rs`, `audit.rs`) vorhanden.
- [ ] CI & Supply Chain: Make Targets (`lint`, `test`, `sbom`, `slsa`, `sign`, `api-validate`) und pipeline scripts.  
  Status: Makefile/CI-Konfiguration nicht vorhanden; `docs/bkg-db.md` definiert Zielzustand.

## Offene Fragen / Klärungsbedarf
- Wer verantwortet Phase-0 Abnahme und Dokumentation?
- Welche Secrets-Management-Lösung wird für `cosign.key` und API-Keys genutzt?
- Status der Clean-Room Vorgaben für neue Adapter/Bindings?
