# docs/api.md

Version: 0.2
Letzte Änderung: 2025-10-18
Maintainer: @bkgoder

---

## Zweck
Dieser Leitfaden dokumentiert die HTTP-Schnittstellen des `cave-daemon` (Sandbox-Lifecycle und API-Key-Verwaltung). Er ergänzt `README.md`, `docs/cli.md`, `docs/operations.md` und verweist auf die generierte OpenAPI-Spezifikation `../openapi.yaml`.

Die Spezifikation wird automatisiert über `make api-schema` erzeugt (`cargo run --bin export-openapi`) und in CI via `openapi-cli validate` geprüft. Die OpenAPI-Daten leiten sich direkt aus den mit `utoipa` annotierten Handlern unter `crates/cave-daemon/src/server.rs` ab; jede API-Änderung muss daher über Rust-Code erfolgen und anschließend das Schema neu generieren.

---

## Authentifizierung & Scopes
- Alle geschützten Endpunkte erwarten einen `Authorization: Bearer <token>` Header.
- Tokens werden durch den Daemon ausgegeben (`POST /api/v1/auth/keys`) und besitzen Scopes:
  - `admin` – Vollzugriff auf alle Ressourcen.
  - `namespace` – Zugriff auf eine konkrete Namespace-ID (`scope.namespace`).
- Der erste Key kann ohne Autorisierung angelegt werden; anschließend ist Admin-Scope erforderlich (`crates/cave-daemon/src/server.rs`).

Standardfehler:
- `401 Unauthorized` – Token fehlt oder ist ungültig (`require_bearer` / `AuthError::InvalidToken`).
- `403 Forbidden` – Token-Scope reicht nicht aus (`AuthError::Unauthorized`).
- `404 Not Found` – Ressourcen-ID unbekannt (`KernelError::NotFound`, `SandboxError::NotFound`, `AuthError::NotFound`).
- `409 Conflict` – Lifecycle-Konflikte (z. B. doppelter Name, bereits laufende Sandbox).

---

## Sandboxes (`/api/v1/sandboxes*`)

### POST `/api/v1/sandboxes`
Erstellt eine neue Sandbox in einem Namespace. Benötigt Namespace-Scope.

```bash
curl -X POST https://cave.example/api/v1/sandboxes \
  -H "Authorization: Bearer $BKG_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
        "namespace": "demo",
        "name": "runner",
        "runtime": "process",
        "limits": {
          "cpu_millis": 750,
          "memory_mib": 1024,
          "disk_mib": 1024,
          "timeout_seconds": 120
        }
      }'
```

**Antwort (201)**
```json
{
  "id": "9f9c9872-2d9c-4c25-9c36-4e45be927834",
  "namespace": "demo",
  "name": "runner",
  "runtime": "process",
  "status": "created",
  "limits": {
    "cpu_millis": 750,
    "memory_mib": 1024,
    "disk_mib": 1024,
    "timeout_seconds": 120
  },
  "created_at": "2025-10-18T12:34:56Z",
  "updated_at": "2025-10-18T12:34:56Z",
  "last_started_at": null,
  "last_stopped_at": null
}
```

Fehler: `400` (fehlende Namespace-Query), `401`, `403`, `409` bei Namensduplikaten (`KernelError::Sandbox`).

### GET `/api/v1/sandboxes?namespace=demo`
Listet alle Sandboxes eines Namespaces. Query-Parameter `namespace` ist Pflicht (`SandboxListQuery`). Rückgabe: Array von `SandboxResponse`.

### GET `/api/v1/sandboxes/{id}/status`
Liefert Metadaten einer Sandbox inklusive Lifecycle-Timestamps. Fehlermeldung `404` bei unbekannter ID.

### POST `/api/v1/sandboxes/{id}/start`
Startet eine Sandbox. Liefert den aktualisierten Datensatz; `409` wenn bereits gestartet (`KernelError::AlreadyRunning`).

### POST `/api/v1/sandboxes/{id}/exec`
Führt einen Befehl im laufenden Container aus.

**Request**
```json
{
  "command": "python",
  "args": ["-c", "print('hello')"],
  "timeout_ms": 2000
}
```

**Response (200)**
```json
{
  "exit_code": 0,
  "stdout": "hello\n",
  "stderr": "",
  "duration_ms": 42,
  "timed_out": false
}
```

### POST `/api/v1/sandboxes/{id}/stop`
Stoppt eine Sandbox. Erfolgreich mit HTTP 204. `409` wenn Sandbox nicht läuft (`KernelError::NotRunning`).

### DELETE `/api/v1/sandboxes/{id}`
Löscht Sandbox-Ressourcen und Persistenz-Records. Erfolgreich mit HTTP 204.

### GET `/api/v1/sandboxes/{id}/executions`
Listet die letzten Ausführungen (`limit` Query 1..100, default 20).

**Response**
```json
[
  {
    "command": "python",
    "args": ["-c", "print('hello')"],
    "executed_at": "2025-10-18T12:35:10Z",
    "exit_code": 0,
    "stdout": "hello\n",
    "stderr": "",
    "duration_ms": 55,
    "timed_out": false
  }
]
```

---

## API-Schlüssel (`/api/v1/auth/keys*`)

### POST `/api/v1/auth/keys`
Erzeugt einen API-Key. Ohne vorhandene Keys optional authentifiziert, danach Admin-Scope nötig.

```bash
curl -X POST https://cave.example/api/v1/auth/keys \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
        "scope": { "type": "namespace", "namespace": "demo" },
        "rate_limit": 100,
        "ttl_seconds": 2592000
      }'
```

**Antwort (201)**
```json
{
  "token": "bkg_demo_abcdefghijklmno",
  "info": {
    "id": "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed",
    "scope": { "type": "namespace", "namespace": "demo" },
    "rate_limit": 100,
    "created_at": "2025-10-18T12:00:00Z",
    "last_used_at": null,
    "expires_at": "2025-11-17T12:00:00Z",
    "key_prefix": "bkg_demo_abcd"
  }
}
```

### GET `/api/v1/auth/keys`
Listet alle bekannten Keys (Admin-Scope). Antwort: Array aus `KeyInfo` Objekten (`crates/cave-daemon/src/auth.rs`).

### DELETE `/api/v1/auth/keys/{id}`
Revokiert einen Key. Erfolgreich mit HTTP 204. `404` wenn ID unbekannt (`AuthService::revoke`).

---

## Health & Metrics
- `GET /healthz` → 200 bei Erfolg (leer). In Deployment-Healthchecks verwenden.
- `GET /metrics` → `text/plain` Prometheus-Payload, aktuell Platzhalter `bkg_cave_daemon_up 1`.

---

## Fehlervertrag
Jede JSON-Fehlermeldung folgt dem Schema `{"error": "..."}` (`ErrorBody`). Die genaue Nachricht entspricht dem konkreten Fehlerfall (z. B. `"missing Authorization bearer token"`, `"sandbox ... not found"`). Dies spiegelt die Implementierung in `crates/cave-daemon/src/server.rs` wider (`ApiError`).

---

## Tooling & Tests
- Schema generieren: `make api-schema` → führt `cargo run --bin export-openapi` aus und schreibt `openapi.yaml`.
- Validierung: `openapi-cli validate openapi.yaml` (CI + lokal) sowie `ajv validate` für `cave.yaml`.
- Funktions- & Negativtests: `cargo test -p cave-daemon` deckt Handler- und Limit-Konvertierungen ab.
- Dokumentation aktuell halten: bei Änderungen an Handlern sowohl diese Datei als auch `docs/cli.md`, `docs/governance.md` und `docs/Progress.md` aktualisieren.

---

SPDX-License-Identifier: Apache-2.0
