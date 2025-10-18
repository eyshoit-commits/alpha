# docs/env.md

Version: 1.1
Letzte Änderung: 2025-10-19
Maintainer: @bkgoder

Dieses Dokument listet alle relevanten Umgebungsvariablen (ENV) der BKG-Plattform, kennzeichnet sensible Variablen und gibt Hinweise zur sicheren Verwendung in CI/CD und Entwicklung.

WICHTIG: Variablen, die als sensibel markiert sind, dürfen niemals in CI-Logs oder öffentlichen Artefakten landen. Nutze Secret-Stores (Vault, GitHub/GitLab Secrets, AWS Secrets Manager) und aktiviere Masking auf allen Runnern.

Übersicht (Kurz)
----------------

### Kritische Secrets & Verbindungen
| Variable | Sensitiv | Beschreibung |
|----------|----------|--------------|
| `BKG_API_KEY` | ✅ | Primärer API-Key/Session-Token für CLI & SDK. Nur lokal im Dev-Modus setzen, niemals loggen. |
| `BKG_DB_DSN` | ✅ | Postgres-Verbindungsstring für Produktions-/Staging-Deployments. Wird von `Database::connect` bevorzugt. |
| `BKG_TLS_CERT` | ✅ | PEM-Zertifikat für mTLS. Als Secret mounten. |
| `BKG_TLS_KEY` | ✅ | Privater Schlüssel für mTLS; darf nie in Artefakten landen. |
| `BKG_OPERATOR_CA` | ✅ | CA-Bundle für Peer-/Operator-Zertifikate. |
| `CAVE_ROTATION_WEBHOOK_SECRET` | ✅ | Base64-kodierter HMAC-Key für Rotationswebhooks (`AuthService::rotate_key`). |
| `CAVE_AUDIT_LOG_HMAC_KEY` | ✅ | Base64-kodierter HMAC-Key für Audit-Log-Signaturen (`AuditLogWriter`). |
| `COSIGN_KEY` | ✅ | Signierschlüssel für SBOM/SLSA-Artefakte (optional `COSIGN_PASSWORD`). |

> Secrets ausschließlich aus Vault/KMS beziehen, im CI maskieren und Rotationen in `docs/Progress.md` dokumentieren.

### Laufzeit & Infrastruktur
| Variable | Sensitiv | Beschreibung |
|----------|----------|--------------|
| `CAVE_API_ADDR` | ⬜ | Bind-Adresse des Daemons. Default `127.0.0.1:8080`. |
| `CAVE_WORKSPACE_ROOT` | ⬜ | Root für Sandbox-Workspaces (`./.cave_workspaces`). Für Prod verschlüsseln. |
| `BKG_DB_PATH` | ⬜ | SQLite-Dateipfad für lokale Entwicklung. Wird ignoriert, wenn `BKG_DB_DSN` gesetzt ist. |
| `CAVE_RUNTIME_DEFAULT` | ⬜ | Standard-Runtime (`process`). Geplante Alternativen: `wasm`, `vm`. |
| `CAVE_DEFAULT_CPU_MILLIS` | ⬜ | Optionales Override für CPU-Limits (Millis). |
| `CAVE_DEFAULT_MEMORY_MIB` | ⬜ | Override für RAM-Limits (MiB). |
| `CAVE_DEFAULT_DISK_MIB` | ⬜ | Override für Workspace-Limits (MiB). |
| `CAVE_DEFAULT_TIMEOUT_SECONDS` | ⬜ | Override für Exec-Timeouts. |
| `BKG_STORAGE_PATH` | ⬜ | Basisverzeichnis für Modelle/Artefakte. |
| `BKG_FEATURE_FLAGS` | ⬜ | Komma-separierte Feature-Flags (`p2p,optin,...`). |
| `CAVE_OTEL_SAMPLING_RATE` | ⬜ | Float 0.0–1.0. Ungültige Werte werden geklemmt und mit Warnung geloggt. |
| `BKG_SCRUB_LOGS` | ⬜ | `true/false` zur Maskierung sensibler Daten in Logs. |

### Isolation & Runtime-Schalter
- `CAVE_DISABLE_ISOLATION` – Deaktiviert Namespaces & cgroups vollständig (nur Debug).
- `CAVE_DISABLE_NAMESPACES` / `CAVE_ENABLE_NAMESPACES` – Erzwingt oder verbietet Namespace-Nutzung.
- `CAVE_DISABLE_CGROUPS` / `CAVE_ENABLE_CGROUPS` – Erzwingt oder verbietet cgroups.
- `CAVE_ISOLATION_NO_FALLBACK` – Verhindert Start, wenn Bubblewrap fehlt.
- `CAVE_BWRAP_PATH` – Absoluter Pfad zur `bubblewrap`-Binary.
- `CAVE_CGROUP_ROOT` – cgroup v2 Root (Default `/sys/fs/cgroup/bkg`).

### Audit & Governance
- `CAVE_AUDIT_LOG_ENABLED` / `CAVE_AUDIT_LOG_DISABLED` – Aktiviert bzw. deaktiviert JSONL-Audit-Logs.
- `CAVE_AUDIT_LOG_PATH` – Zielpfad der Audit-Logs (Default `./logs/audit.jsonl`).
- `CAVE_ROTATION_WEBHOOK_SECRET` – Siehe Tabelle; Pflicht für signierte Rotationsmeldungen.

### Supply Chain & Migrationen
- `COSIGN_KEY` – Siehe Tabelle; für `make sbom`/`make slsa` Signaturen.
- `SQLX_OFFLINE` – Für Offline-Migrationen (`SQLX_OFFLINE=true sqlx migrate run`).

Sicherheits-Hinweise & Best Practices
-------------------------------------
1. Secrets niemals ins Repository oder in PRs committen.
2. Maskiere alle sensiblen ENV in CI-Logs (`mask: true`, Hashing statt Klartext).
3. Automatisiere Schlüsselrotation inkl. webhook Event (`/api/v1/auth/keys/rotated`).
4. Aktiviertes Audit-Logging (`CAVE_AUDIT_LOG_ENABLED=true` + HMAC-Key) ist Pflicht vor Staging/Prod.
5. Für lokale Entwicklung nur `--dev` nutzen, niemals in freigegebenen Umgebungen.
6. Dokumentiere Abweichungen oder Overrides in `docs/Progress.md` inkl. Datum & Owner.

CI Integration Hinweise
-----------------------
- Secrets ausschließlich über den CI-Secret-Store einspielen (`BKG_DB_DSN`, `COSIGN_KEY`, ...).
- `make api-schema`, `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` und `make sbom`/`make slsa` nutzen dieselben ENVs wie Produktions-Builds.
- Rotation-Webhooks in CI testen: Signatur via `CAVE_ROTATION_WEBHOOK_SECRET` injizieren und `curl` gegen `/api/v1/auth/keys/rotated` ausführen.

Beispiel: Startdev
```bash
export BKG_API_KEY="s.xxx"
export CAVE_OTEL_SAMPLING_RATE=1.0
export CAVE_AUDIT_LOG_ENABLED=true
cargo run -p cave-daemon
```

Compliance
----------
- **Audit-Trail**: Aktiviertes Audit-Logging + HMAC ist Voraussetzung für regulatorische Nachweise. Logs nach Deployment signieren (`cosign`) und in WORM-Storage speichern.
- **Schlüsselverwaltung**: `CAVE_ROTATION_WEBHOOK_SECRET` darf nur aus Secret-Stores stammen; Rotationen sind in `docs/Progress.md` festzuhalten.
- **Datenschutz**: `BKG_SCRUB_LOGS=true` in produktiven Umgebungen setzen, Sampling-Rate gemäß `docs/architecture.md` anpassen.
- **Änderungskontrolle**: Jede Änderung an ENV-Defaults erfordert ein Update dieses Dokuments plus Vermerk in `docs/Progress.md`.

Weitere Variablen werden in `docs/compatibility.md` gelistet (z. B. provider-spezifische Overrides für Cloud-Storage).

SPDX-License-Identifier: Apache-2.0
