# docs/roadmap.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

Ziel
----
Dieses Dokument priorisiert die anstehenden Arbeiten für BKG mit Fokus auf den Phase-0 Abschluss. Es übersetzt die Anforderungen aus `README.md` v1.8.2, `PROMPT.md` und `Progress.md` in konkrete Arbeitspakete, inklusive Abhängigkeiten und erwarteter Artefakte.

## Phase-0 Roadmap (verbindlich)

### 1. CAVE-Kernel & Sandbox Runtime (kritisch)
- **Deliverables**
  - Namespaces, Seccomp-Profile, FS-Overlay und cgroups v2 vollständig implementieren.
  - Integrationstests für Lifecycle (`create → start → exec → stop`) sowie Ressourcenlimits.
  - Audit-Log-Writer (signierte JSON-Lines) mit Events für Lifecycle & Exec.
- **Abhängigkeiten**
  - Erweiterung `cave_kernel` Runtime (`crates/cave-kernel`) mit Low-Level Isolation.
  - Erweiterung `ProcessSandboxRuntime` für seccomp/namespace Attachments.
- **Aktionen**
  1. Isolation-Modul erweitern (`crates/cave-kernel/src/isolation.rs`).
  2. Seccomp-Profile definieren & in Tests abdecken.
  3. Audit-Log Writer implementieren und Deployment-Hooks ergänzen.

### 2. Persistente `bkg_db` mit RLS
- **Deliverables**
  - Wechsel auf Postgres Backend mit Row-Level Security Policies.
  - Migrations & Seeds für sandboxes, api_keys, audit_events.
  - Integrationstests für RLS (cross-namespace Access denied).
- **Abhängigkeiten**
  - Deployment-Plan für Postgres (docker-compose, Helm chart).
- **Aktionen**
  1. Neues DB-Modul für Postgres Verbindungen implementieren.
  2. SQL-Migrationen anpassen (Postgres kompatibel).
  3. Test-Suite (`cargo test --features postgres`) aufsetzen.

### 3. Web-UIs (Admin & User)
- **Deliverables**
  - Admin UI: Model Manager, Key Wizard, Peer Dashboard.
  - User UI: Sandbox Lifecycle Steuerung, Chat Studio, Dokumentationslinks.
  - End-to-End Tests (Playwright/Cypress) für kritische Flows.
- **Abhängigkeiten**
  - API-Endpoints stabil (siehe Punkt 1 & 2).
- **Aktionen**
  1. Repository-Struktur (`web/admin`, `web/app`) anlegen.
  2. Design Tokens & Auth-Flow (API-Key Eingabe) implementieren.
  3. CI Build + Lint Pipeline ergänzen.

## Unterstützende Arbeitspakete

### CI & Automatisierung
- `make api-schema`, `openapi-cli validate`, `ajv validate` in CI integrieren.
- SBOM/SLSA Pipeline (`make sbom`, `make slsa`, `cosign sign-blob`) aufsetzen.
- `pytest security/` für Threat-Matrix Validierung hinzufügen.

### Governance & Sicherheit
- Schlüsselrotation + Webhook (`/api/v1/auth/keys/rotated`) implementieren.
- Rate-Limit Durchsetzung im Gateway (Admin 1000/min, Namespace 100/min, etc.).
- Telemetrie (`CAVE_OTEL_SAMPLING_RATE`) in Services einbinden.
- Seccomp/Namespace Hardening dokumentieren (Host-Anforderungen, Profile, Tests) und in Kernel-Roadmap aufnehmen.
- Aktualisierte Sandbox-Defaultlimits (2 vCPU / 1 GiB RAM / 120 s / 1 GiB Disk) in Monitoring, Alerting & Governance-Playbooks hinterlegen.

### Dokumentation
- Fehlende Pflichtdokumente erstellen (`docs/api.md`, `docs/cli.md`, `docs/security.md`, …).
- `docs/FEATURE_ORIGINS.md` mit echten Einträgen und Reviewer-Signoff füllen.
 - Onboarding-Anleitung für Agents ergänzen (Querverweis zu `AGENTS.md`, `PROMPT.md`).

### 4. BKG-DB Voll-Stack (Supabase-Klasse)
- **Deliverables**
  - MVCC-Kernel mit WAL/Checkpointing (`crates/bkg-db/src/kernel.rs`, `storage.rs`).
  - SQL-Pipeline (Parser, Planner, Executor), RLS/JWT Auth und Policy Engine.
  - API-Layer: HTTP `/query|auth|policy|schema`, pgwire-Server, gRPC-Schnittstelle.
  - Realtime/CDC (`realtime.rs`), Objekt-Storage (`storage.rs`), Telemetry/Audit (`telemetry.rs`, `audit.rs`).
  - Admin-UI (`web/admin`) mit Tabs Overview · Policies · Users · Telemetry · Audit.
  - CI Targets (`make lint/test/sbom/slsa/sign`, `api-validate`) + cosign Signaturen.
- **Abhängigkeiten**
  - Stabile Infrastruktur aus Phase-0 (Kernel Isolation, Postgres/RLS, Auth Keys).
  - Dokumentation & Governance (siehe `docs/bkg-db.md`, `docs/governance.md`).
- **Aktionen**
  1. Modulstruktur in `crates/bkg-db/src` anlegen und Kern-Interfaces definieren (Kernel, SQL, RLS, API).
  2. Admin-UI Projekt (`web/admin`) bootstrappen (Next.js) inkl. API-Client.
  3. Realtime/CDC und Objekt-Storage Services implementieren; Telemetry/Audit + CI-Pipeline aktivieren.
  4. Fortschritt in `docs/Progress.md` dokumentieren und Make-Targets in CI integrieren.

## Reihenfolge & Checkpoints
1. **Security Foundations**: Kernel Isolation + Audit Logs (Blocker für weitere Arbeit).
2. **Data Integrity**: Postgres/RLS + Migrations (erforderlich für UI & P2P).
3. **User Experience**: Web-UIs + CLI-Validierungen.
4. **Operations**: CI/CD, SBOM/SLSA, Threat-Matrix Tests.
5. **Governance**: Schlüsselrotation, Rate-Limits, Telemetrie-Policy.

Jeder Abschnitt sollte nach Fertigstellung in `Progress.md` reflektiert werden. Iterationen sind erlaubt, solange Blocker aus den vorherigen Phasen nicht offen bleiben.

SPDX-License-Identifier: Apache-2.0
