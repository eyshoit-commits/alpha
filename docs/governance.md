# docs/governance.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

---

## Zweck
Dieses Dokument beschreibt Governance-, Secrets- und Betriebsrichtlinien für die BKG-Plattform. Es ergänzt `README.md`, `AGENTS.md`, `docs/roadmap.md` und `docs/Progress.md`.

---

## 1. Secrets & Schlüsselverwaltung
- **Secrets Store**: Verwende einen zentralen Secret-Manager (z. B. HashiCorp Vault oder Cloud KMS). GitHub/GitLab Secrets dienen ausschließlich als Transportmittel.  
- **Vertrauliche Werte**: `BKG_API_KEY`, `BKG_DB_DSN`, `JWT_SECRET`, `cosign.key`, TLS-Zertifikate, OTEL-Tokens.  
- **Rotation**:
  - Admin-Schlüssel: alle 90 Tage.  
  - Namespace-Schlüssel: alle 30 Tage (Auto-Rotation bei Verwendung älter als 7 Tage).  
  - Model-Access & Session Keys: stündlich.  
  - cosign-Schlüssel: jährlich oder bei Incident.  
- **Webhooks**: `POST /api/v1/auth/keys/rotated` muss per HMAC signiert, auditiert und versionsverwaltet werden.
- **Dokumentation**: Jede Rotation in `docs/Progress.md` notieren (Datum, Owner, betroffene Services).

---

## 2. Telemetrie & Monitoring
- Sampling-Richtlinien (`CAVE_OTEL_SAMPLING_RATE`):  
  - Dev: 1.0  
  - Staging: 0.5  
  - Production: 0.05–0.2  
- Alle Services müssen `/healthz` (200/503) und `/metrics` (Prometheus) bereitstellen.  
- Monitoring aktualisieren, um neue Sandbox-Defaults (2 vCPU / 1 GiB RAM / 120 s / 1 GiB Disk) zu reflektieren.  
- Alerts für Sandbox-Quota-Verbrauch, fehlgeschlagene Rotationen, OTEL-Exporter und cosign-Verifikation konfigurieren.

---

## 3. Postgres/RLS Migration (von SQLite Prototyp)
1. **Vorbereitung**  
   - Provisioniere verwaltetes Postgres-Cluster mit RLS-Unterstützung.  
   - Sichere Zugangsdaten über Secrets-Manager (siehe Abschnitt 1).  
   - Erstelle Infrastructure-as-Code (Helm/Terraform) inkl. Storage, Backups, Monitoring.  
2. **Migrations-Entwurf**  
   - Portiere `crates/bkg-db/migrations/0001_init.sql` nach Postgres Syntax inkl. RLS-Policies.  
   - Füge Seeds für Admin/Namespace Accounts und Feature-Flags hinzu.  
   - Ergänze Migrationstests (`sqlx migrate run --database-url $BKG_DB_DSN`).  
3. **Dual-Write Phase**  
   - Implementiere optionalen Dual-Write Layer im Daemon (`BKG_DB_DUAL_WRITE=true`).  
   - Vergleiche Konsistenz via nightly Jobs (`cargo test -p bkg-db --features postgres`).  
4. **Cutover**  
   - Stoppe Scheduler, aktiviere Postgres URL via `BKG_DB_DSN`.  
   - Führe Migrations-Lauf (`sqlx migrate run`) und Backups (`pg_dump`).  
   - Aktualisiere `docs/Progress.md` mit Ergebnis & Rollback-Plan.  
5. **Nachsorge**  
   - Deaktiviere SQLite Fallback (`BKG_DB_PATH` nur lokal).  
   - Passe Monitoring an (PG metrics, WAL Lag, RLS Audit).  
   - Aktualisiere `docs/bkg-db.md` und `docs/env.md` mit Produktionsparametern.

---

## 4. Auditing & Compliance
- Audit-Logs müssen als signierte JSON-Lines vorliegen (cosign).
- Bewahre Logs in S3/Blob-Speicher mit WORM (Write Once Read Many) auf.
- Führe monatliche Audit-Reviews durch (Rotationen, Sandbox-Ausreißer, Telemetrie).
- Erstelle Incident-Response-Playbooks für Schlüsselverlust, Audit-Manipulation, Telemetrie-Ausfall.

---

## 5. Build- & Lieferketten-Automatisierung
- **OpenAPI-Schema**: `make api-schema` erzeugt `openapi.yaml` per `cargo run --bin export-openapi`; CI ruft das Target auf und prüft mittels `openapi-cli validate`, dass der Commit-Stand eingecheckt bleibt (`.github/workflows/ci.yml`). Die Quelle dafür sind die `utoipa`-Annotationen an den Axum-Handlern (`crates/cave-daemon/src/server.rs`).
- **Schema-Drift**: Jeder CI-Lauf führt `git diff --exit-code -- openapi.yaml` aus; Abweichungen brechen den Build und verweisen auf nachzuholende Commits.
- **SBOM**: `supply-chain`-Job nutzt Syft (`sbom.json`). Ergebnis bleibt Artefakt; Signatur erfolgt nur, wenn `COSIGN_KEY_B64` Secret gesetzt oder `cosign.key` eingecheckt ist.
- **Cosign**: Signing-Step überspringt sich ohne Secret, vermeidet Fehlalarme. Bereitgestellte Schlüssel werden aus dem Secret Base64-dekodiert und nicht im Repo persistiert.
- **SLSA**: Platzhalter (`echo "TODO: invoke make slsa once implemented"`). Sobald `make slsa` existiert, Schritt ersetzen und Signaturpfad dokumentieren.
- **SBOM/SLSA Verification**: Downstream-Checks müssen `cosign verify-blob sbom.json --signature sbom.sig --key cosign.pub` berücksichtigen; `sbom.sig` wird nur erzeugt, wenn Signing-Secret vorliegt.

---

## 6. Governance-Backlog
- [ ] Vault/KMS-Integration fertigstellen (Secret Lifecycle, Access Policies).
- [ ] RLS-Policy-Library dokumentieren (Templates, Approval-Prozess).
- [ ] Monitoring-Playbooks für neue Sandbox-Limits finalisieren.
- [ ] Incident-Runbooks (Sandbox-Ausfall, RLS-Durchbruch, Cosign-Fehler) erweitern.

---

SPDX-License-Identifier: Apache-2.0
