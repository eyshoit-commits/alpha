# docs/cli.md

Version: 0.2
Letzte Änderung: 2025-10-18
Maintainer: @bkgoder

---

## Zweck
Beschreibt die CLI-Workflows (`bkg`, `cavectl`) zur Interaktion mit der Cave-API. Ergänzt `docs/api.md`, `docs/operations.md` sowie die OpenAPI-Spezifikation (`../openapi.yaml`).

Alle Befehle verwenden denselben Fehlervertrag wie die REST-API (`{"error": "..."}`) und reichen HTTP-Codes direkt durch. Fehlermeldungen aus dem Daemon werden ohne zusätzliche Verarbeitung angezeigt.

---

## Vorbereitung
1. API-Key besorgen (`cavectl key issue --scope namespace --ttl 30d`). Alternativ via REST (`POST /api/v1/auth/keys`).
2. `BKG_API_KEY` exportieren oder `--token` Flag verwenden.
3. Workspace-Konfiguration (`cave.yaml`) per `bkg init` erzeugen; optional Limits setzen (`bkg config set limits.cpu 750`).

---

## BKG CLI (`bkg`)

| Befehl | Beschreibung | HTTP Endpoint |
|--------|---------------|---------------|
| `bkg sandbox create` | Legt Sandbox im Namespace an. | `POST /api/v1/sandboxes` |
| `bkg sandbox ls --namespace demo` | Listet Sandboxes (erfordert Namespace-Query). | `GET /api/v1/sandboxes?namespace=demo` |
| `bkg sandbox start <id>` | Startet Sandbox. | `POST /api/v1/sandboxes/{id}/start` |
| `bkg sandbox exec <id> -- "python" -c 'print("hi")'` | Führt Prozess aus. | `POST /api/v1/sandboxes/{id}/exec` |
| `bkg sandbox stop <id>` | Stoppt Sandbox (204). | `POST /api/v1/sandboxes/{id}/stop` |
| `bkg sandbox rm <id>` | Entfernt Sandbox. | `DELETE /api/v1/sandboxes/{id}` |
| `bkg sandbox executions <id> --limit 10` | Zeigt Historie. | `GET /api/v1/sandboxes/{id}/executions` |

### Beispiele

```bash
# Sandbox anlegen (siehe docs/api.md für vollständigen Payload)
bkg sandbox create --namespace demo --name runner --runtime process \
  --cpu 750 --memory 1024 --disk 1024 --timeout 120

# Laufenden Container nutzen
bkg sandbox start 9f9c9872-2d9c-4c25-9c36-4e45be927834
bkg sandbox exec 9f9c9872-2d9c-4c25-9c36-4e45be927834 -- python -c 'print("hello")'
```

Bei HTTP-Fehlern gibt die CLI den API-Response aus. Beispiel (`409 Conflict`):

```text
sandbox 'runner' already exists in namespace 'demo'
```

---

## Admin CLI (`cavectl`)

| Befehl | Zweck | HTTP Endpoint |
|--------|-------|---------------|
| `cavectl key issue --scope namespace --ttl 30d` | Erstellt Namespace-Key (gibt Token einmalig aus). | `POST /api/v1/auth/keys` |
| `cavectl key list` | Listet Keys samt Präfixen. | `GET /api/v1/auth/keys` |
| `cavectl key revoke <uuid>` | Revokiert Key. | `DELETE /api/v1/auth/keys/{id}` |
| `cavectl health` | Prüft Liveness. | `GET /healthz` |
| `cavectl metrics` | Holt Prometheus-Payload. | `GET /metrics` |

### Ausgabeformate
- `cavectl key issue` schreibt Token + `KeyInfo`-JSON (siehe `docs/api.md`).
- `cavectl key list --format table` mappt `KeyInfo`-Felder (`id`, `scope.type`, `rate_limit`, `last_used_at`).
- `cavectl metrics` gibt Rohtext zurück; für `jq`/`yq` ungeeignet.

---

## Fehlbehandlung & Retries
- `401/403` → CLI schlägt fehl, Hinweis auf Scope oder Token-Gültigkeit.
- `404` → Ressource nicht gefunden, ID prüfen.
- `409` → Lifecycle-Konflikt. Bei `start`/`stop` 1x Retry möglich; bei `create` Namespace/Name anpassen.
- `5xx` → CLI beendet mit Nicht-Null-Exitcode, loggt Response Body.

---

## Automatisierung
- Schema-Aktualisierung: `make api-schema` (ruft `cargo run --bin export-openapi` und damit die `utoipa`-Annotationen des Daemons auf).
- Validation in CI: `openapi-cli validate openapi.yaml` (siehe `.github/workflows/ci.yml`).
- CLI-E2E-Tests (Backlog): `cargo test -p cave-daemon` + künftige Integrationstests (`assert_cmd`).

---

SPDX-License-Identifier: Apache-2.0
