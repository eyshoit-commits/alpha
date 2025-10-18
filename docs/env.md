# docs/env.md

Version: 1.0  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

Dieses Dokument listet alle relevanten Umgebungsvariablen (ENV) der BKG‑Plattform, kennzeichnet sensible Variablen und gibt Hinweise zur sicheren Verwendung in CI/CD und Entwicklung.

WICHTIG: Variablen, die mit `sensitive: true` markiert sind, dürfen niemals in CI‑Logs ausgegeben oder in öffentlich sichtbaren Artefakten (z. B. PR‑Logs) landen. Verwende Secret‑Stores (Vault, GitHub Secrets, GitLab CI variables, AWS Secrets Manager) und setze masking auf CI‑Runnern.

Übersicht (Kurz)
- `BKG_API_KEY` (sensitive: true)  
  Beschreibung: Haupt‑API‑Key oder session token für CLI/SDK. Niemals in Klartext loggen. Wird toleriert in Dev‑Mode (--dev) nur lokal.  
  Beispiel (Linux): `export BKG_API_KEY="s.xxxxx"`

- `BKG_DB_DSN` (sensitive: true)  
  Beschreibung: Database connection string (Postgres DSN). In Produktionsumgebungen aus Vault laden oder via K8s Secret mounten.  
  Beispiel: `postgres://user:password@127.0.0.1:5432/bkgdb`
  Hinweise: `Database::connect` erkennt `postgres://` bzw. `postgresql://` automatisch, aktiviert den Postgres-Treiber und führt die Migrations aus `crates/bkg-db/migrations_postgres` aus. Verwende `SQLX_OFFLINE=true sqlx migrate run`, um Schema-Änderungen offline vorzubereiten.

- `BKG_DB_PATH` (sensitive: false)  
  Beschreibung: Lokaler Datei‑Pfad (fallback) für einfache /dev Setups. Nicht empfohlen für Prod.
  Hinweise: Bei `sqlite://` Pfaden nutzt `Database::connect` weiterhin die SQLite-Migrationen (`crates/bkg-db/migrations`). So bleibt ein schneller In-Memory/Datei-Workflow für lokale Tests erhalten.

- `CAVE_RUNTIME_MODE` (sensitive: false)  
  Beschreibung: `quick` | `persistent` — default `quick` für fast cold starts (WASM/WASI). In Produktionsclustern default via policy konfiguriert.

- `BKG_STORAGE_PATH` (sensitive: false)  
  Beschreibung: Root Path für modellspeicher & cache.

- `CAVE_API_ADDR` (sensitive: false)  
  Beschreibung: Listen Adresse des `cave-daemon` (z. B. `0.0.0.0:8080`). Dev default `127.0.0.1:8080`.

- `CAVE_WORKSPACE_ROOT` (sensitive: false)  
  Beschreibung: Basisverzeichnis für Sandbox Workspaces. In Prod auf dedizierten Storage mounten (z. B. verschlüsselte Partition, tmpfs).

- `BKG_TLS_CERT` (sensitive: true)  
  Beschreibung: PEM Zertifikat für Service mTLS; in K8s als Secret mounten.

- `BKG_TLS_KEY` (sensitive: true)  
  Beschreibung: Private Key für mTLS; nie als plaintext in CI.

- `BKG_OPERATOR_CA` (sensitive: true)  
  Beschreibung: CA used to sign peer certs; operator‑controlled.

- `BKG_SCRUB_LOGS` (sensitive: false)  
  Beschreibung: Boolean flag, ob PII in Logs automatisch entfernt wird (true/false).

- `CAVE_OTEL_SAMPLING_RATE` (sensitive: false)  
  Beschreibung: Float zwischen 0.0 und 1.0, default `1.0` (100%). Empfohlen: production 0.05–0.2.  
  Beispiel: `export CAVE_OTEL_SAMPLING_RATE=0.1`

- `CAVE_DISABLE_ISOLATION` / `CAVE_DISABLE_NAMESPACES` / `CAVE_DISABLE_CGROUPS` (sensitive: false)  
  Beschreibung: Booleans (`true`/`false`) für kontrollierte Abschaltung der Isolation. Nur für Debugging verwenden.

- `CAVE_ENABLE_NAMESPACES` / `CAVE_ENABLE_CGROUPS` (sensitive: false)  
  Beschreibung: Erzwingt explizit die Aktivierung der jeweiligen Isolation, selbst wenn Defaults deaktiviert würden.

- `CAVE_ISOLATION_NO_FALLBACK` (sensitive: false)  
  Beschreibung: Verhindert automatisches Zurückfallen auf plain Prozessausführung, falls `bwrap` fehlt.

- `CAVE_BWRAP_PATH` (sensitive: false)  
  Beschreibung: Absoluter Pfad zur Bubblewrap Binary. Default: Ergebnis von `which bwrap`.

- `CAVE_CGROUP_ROOT` (sensitive: false)  
  Beschreibung: cgroup‑v2 Root für Sandbox Limits. Beispiel: `/sys/fs/cgroup/bkg` (Operator muss Verzeichnis + Controller konfigurieren).

- `COSIGN_KEY` (sensitive: true)  
  Beschreibung: Signing key for SBOM/SLSA cosign operations in CI. Storage: CI secret store or Vault.

- `BKG_FEATURE_FLAGS` (sensitive: false)  
  Beschreibung: Comma separated feature flags (e.g., `p2p,optin,dev-mode`).

Sicherheits‑Hinweise & Best Practices
1. Secrets niemals in Repository/PRs: Verwende CI‑Secrets oder Vault.  
2. Maskiere sensitive ENV in CI‑Logs: setze `mask: true` in GitHub Actions secrets oder entsprechende Option im CI‑System.  
3. Automatisiere Key Rotation: setze Hintergrundjobs, die `BKG_API_KEY` rotation triggers & webhook events erzeugen.  
4. Für lokale Entwicklung: nutze `--dev` streng nur lokal, nie in Freigabestufen.  
5. Audit: jede Key‑Operation (issue/rotate/revoke) wird in Audit‑Log protokolliert.

CI Integration Hinweise
- Vor CI‑Build schalte sensitive ENV in Secrets des CI‑Providers.  
- Beispiel in GitHub Actions, Secrets: `COSIGN_KEY` als secret, `BKG_DB_DSN` als secret.  
- Vermeide echoing von ENV in logs. Wenn nötig, logge nur masked/hashed values.

Beispiel: Startdev
```bash
export BKG_API_KEY="s.xxx"
export CAVE_OTEL_SAMPLING_RATE=1.0
bkg server start
```

Weitere Variablen werden in `docs/compatibility.md` gelistet (z. B. provider‑specific overrides für cloud storage).

SPDX-License-Identifier: Apache-2.0
