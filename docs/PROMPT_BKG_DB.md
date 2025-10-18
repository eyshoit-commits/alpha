BKG-DB Full Stack Development Prompt (Supabase-Class)

Version: 1.0
Letzte Änderung: 2025-10-18
Maintainer: @bkgoder

Zweck

Dieses Dokument definiert die Entwicklungsrichtlinien für die Datenbankplattform bkg-db,
die funktional mit Supabase vergleichbar ist, aber vollständig als eigene
Implementierung innerhalb der BKG‑Microsandbox entsteht.
Ziel ist ein vollständiger Datenbank‑Stack mit DB‑Kern, SQL‑Schicht,
Authentifizierung und Row-Level-Security (RLS), API‑Layer, Realtime‑Events,
Objekt‑Storage und einem Admin‑UI.
Die nachfolgenden Abschnitte beschreiben die notwendige Ordnerstruktur,
die wichtigsten Features sowie konkrete Implementierungs- und Workflow‑Schritte.

Bezug zum Repository

Alle Pfade und Strukturen beziehen sich auf das BKG‑Repository, wie es in
file.md beschrieben ist. Die wichtigsten Verzeichnisse sind:

./Cargo.lock
./Cargo.toml
./crates
./crates/bkg-db
./crates/cave-daemon
./crates/cave-kernel
./docs
./web


Code für bkg-db liegt in ./crates/bkg-db/.

Migrations werden in crates/bkg-db/migrations/ abgelegt.

Admin‑Panel entsteht im Verzeichnis web/admin/.

Die Implementierung muss diese Struktur strikt einhalten, damit Builds und Tests
funktionieren und andere Agenten (z. B. für Web‑UI) korrekt zugreifen können.

Zielsetzung

Die „bkg-db“ soll als Supabase-äquivalente Datenplattform dienen.
Das umfasst insbesondere:

ACID‑konforme DB-Engine: Vollständige MVCC-Implementierung mit WAL,
Snapshots und Recovery.

SQL‑Pipeline: Parser → Planner → Executor, Unterstützung für
SELECT/INSERT/UPDATE/DELETE, Joins und Aggregationen nach SQL92.

Auth & RLS: JWT-basierte Authentifizierung mit Scope-Prüfung (Admin vs.
Namespace) und JSON-basierten RLS-Policies.

API‑Layer: PostgreSQL-Wire-Kompatibilität sowie REST (HTTP/gRPC).

Realtime/CDC: WAL-basierte Events (Pub/Sub) via WebSocket oder SSE.

Objekt-Storage: Buckets mit eigenen Policies, presigned URLs,
S3-kompatible Backends.

Admin‑UI: React/Next.js‑Dashboard zur Verwaltung von Policies, Benutzern,
Telemetrie und Audit-Logs.

Telemetry & Audit: OTEL-Integration, signierte Audit-Logs mittels
cosign, einstellbare Sampling-Rate (CAVE_OTEL_SAMPLING_RATE).

Feature-Übersicht

Die folgende Liste stellt die wichtigsten Features und Komponenten zusammen.
Dies dient als Checkliste für die Implementierung und die CI-Prüfungen:

Kategorie	Features
DB‑Kern	MVCC, WAL, Snapshots, Recovery
SQL-Layer	Parser (sqlparser crate), Planner, Executor (Iter/Batch)
Auth/RLS	JWT-basierte Auth, Scopes, JSON-Policies
API	PostgreSQL wire (tcp), HTTP/REST (axum), gRPC (tonic)
Realtime	WAL-Tailer, Pub/Sub via WS/SSE
Storage	Pluggable Storage Layer (S3, lokale Files), RLS auf Buckets
Admin-UI	Next.js Dashboard mit Tabs: Overview, Policies, Users, Telemetry, Audit
Telemetry/Audit	OTEL-Tracing, JSON-Lines Audit-Logs, Signaturen
CI/Supply Chain	SBOM und SLSA via make sbom, make slsa, cosign sign-blob
Implementierungsrichtlinien

Clean-Room – Keine Übernahme von fremdem Code; Inspiration aus
Supabase ist erlaubt, aber die Implementierung erfolgt neu in Rust (bzw. TypeScript
für das UI). Quell‑Recycling ist strikt untersagt.

Modulare Architektur – Die Struktur in crates/bkg-db/src/ sollte in
Module gegliedert sein, z. B. kernel.rs, sql.rs, planner.rs,
executor.rs, rls.rs, auth.rs, api.rs, realtime.rs, storage.rs,
telemetry.rs, audit.rs. Alle externen Interfaces (Traits) sind sauber
definiert und versioniert.

Test-Driven – Für jeden Kernteil (Parser, Planner, RLS, API) werden
Unit‑ und Integrationstests angelegt. E2E‑Tests prüfen die komplette
API-Funktionalität sowie Policy‑Enforcement.
Die Tests laufen mit cargo test (evtl. mit Features postgres).

API‑Stabilität – Definiere OpenAPI-Spezifikationen (in docs/api.md) und
prüfe sie in CI (openapi-cli validate). Alle Breaking Changes müssen im
Changelog (Teil von docs/bkg-db.md) dokumentiert werden.

Admin-UI – Implementiere ein Next.js‑Projekt unter web/admin mit
Seiten für Overview, Policies, Users, Telemetry und Audit.
Nutze Tailwind & shadcn/ui für Styling; implementiere den Auth‑Flow
(API-Key Eingabe) und RLS-basierte Ansichten.

Telemetry & Audit – Integriere tracing und opentelemetry im
bkg-db-Server. Audit-Logs werden als signierte JSON‑Lines mit cosign
erzeugt. Die Sampling-Rate ist über CAVE_OTEL_SAMPLING_RATE steuerbar.

CI & Supply Chain – Das Projekt liefert SBOM (Syft), SLSA und
signierte Artefakte. CI-Targets sind lint, test, sbom, slsa,
sign (siehe Makefile).

Workflow (Entwickler)

Build & Start – Führe cargo build --workspace aus, starte den Server
mit cargo run -p bkg-db. Überprüfe /healthz und /metrics.

Schema & Migrationen – Lege Migrationen in crates/bkg-db/migrations an.
Nutze ein CLI oder REST‑Endpoint (/migrate/apply) zum Anwenden.

API implementieren – Endpunkte für /query, /auth, /policy,
/realtime bereitstellen. Implementiere auch das pgwire-Protokoll (Tcp)
und teste mit einem Psql-Client.

Admin-UI entwickeln – Wechsle in web/admin, installiere Abhängigkeiten
(npm install) und starte das Dev-Server (npm run dev). Baue die
Komponenten aus shadcn/ui, binde sie an die API an.

Telemetry & Audit testen – Prüfe, ob Log-Events korrekt erfasst und
signiert werden; justiere die Sampling-Rate.

CI vorbereiten – Erstelle Makefile-Targets für Lint, Test, SBOM/SLSA
und Signaturen; füge diese in die CI‑Pipeline ein.

Sonstige Hinweise

Agent-Regeln – Folge den Prozessen und Sicherheitsregeln aus
docs/Agents.md (Least Privilege, Cleanup, Audit). Ändere diese Datei nur
via Pull Request mit Tests.

Dokumentation – Erstelle oder erweitere docs/bkg-db.md mit
detaillierter Architektur, Endpunkten, RLS-Beispielen und Migrations.

Konfigurationsdateien – Nutze .env oder docs/env.md als Leitfaden
für notwendige Variablen (BKG_DB_DSN, CAVE_OTEL_SAMPLING_RATE).

Governance – Behalte die Schlüssel-Rotation, Rate-Limits und RLS
gemäß docs/governance.md im Blick.

Lizenz

SPDX-License-Identifier: Apache-2.0