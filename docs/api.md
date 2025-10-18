# docs/api.md

Version: 0.2
Letzte Änderung: 2025-10-18
Maintainer: @bkgoder

---

## Zweck
Dieser Leitfaden dokumentiert die HTTP-Schnittstellen des `cave-daemon` (Sandbox-Lifecycle und API-Key-Verwaltung). Er ergänzt `README.md`, `docs/cli.md`, `docs/operations.md` und verweist auf die generierte OpenAPI-Spezifikation `../openapi.yaml`.

Die Spezifikation wird automatisiert über `make api-schema` erzeugt (`scripts/generate_openapi.py`) und in CI via `openapi-cli validate` geprüft. Änderungen an Handlern unter `crates/cave-daemon/src/server.rs` müssen parallel in dieser Datei und im Schema reflektiert werden.

---

## Authentifizierung & Scopes
- Alle geschützten Endpunkte erwarten einen `Authorization: Bearer <token>` Header.
- Tokens werden durch den Daemon ausgegeben (`POST /api/v1/auth/keys`) und besitzen Scopes:
  - `admin` – Vollzugriff auf alle Ressourcen.
  - `namespace` – Zugriff auf eine konkrete Namespace-ID (`scope.namespace`).
- Der erste Key kann ohne Autorisierung angelegt werden; anschließend ist Admin-Scope erforderlich (`crates/cave-daemon/src/main.rs:534`).

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

**Antwort (200)**
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

**Antwort (200)**
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

Die zurückgegebenen `KeyInfo`-Objekte enthalten Metadaten zur Historie (`rotated_from`, `rotated_at`) sobald ein Schlüssel ersetzt wurde. Die Felder bleiben sonst `null`.

### GET `/api/v1/auth/keys`
Listet alle bekannten Keys (Admin-Scope). Antwort: Array aus `KeyInfo` Objekten (`crates/cave-daemon/src/auth.rs`).

### DELETE `/api/v1/auth/keys/{id}`
Revokiert einen Key. Erfolgreich mit HTTP 204. `404` wenn ID unbekannt (`AuthService::revoke`).

### POST `/api/v1/auth/keys/rotate`
Ersetzt einen bestehenden Key durch ein neues Token. Admin-Scope erforderlich.

```bash
curl -X POST https://cave.example/api/v1/auth/keys/rotate \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
        "key_id": "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed",
        "rate_limit": 150,
        "ttl_seconds": 604800
      }'
```

**Antwort (200)**
```json
{
  "token": "bkg_demo_rotatedtoken",
  "info": {
    "id": "5f86a0ef-55c0-4f50-a1e9-b85a2b3db0fe",
    "scope": { "type": "namespace", "namespace": "demo" },
    "rate_limit": 150,
    "created_at": "2025-10-18T12:30:00Z",
    "last_used_at": null,
    "expires_at": "2025-10-25T12:30:00Z",
    "key_prefix": "bkg_demo_rot",
    "rotated_from": "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed",
    "rotated_at": "2025-10-18T12:30:00Z"
  },
  "previous": {
    "id": "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed",
    "scope": { "type": "namespace", "namespace": "demo" },
    "rate_limit": 100,
    "created_at": "2025-09-10T09:00:00Z",
    "last_used_at": "2025-10-18T12:29:58Z",
    "expires_at": null,
    "key_prefix": "bkg_demo_abcd",
    "rotated_from": null,
    "rotated_at": "2025-10-18T12:30:00Z"
  },
  "webhook": {
    "event_id": "6b4dc7a8-1e5a-4cfa-a2e2-f9d4f2b1c90c",
    "signature": "BASE64_HMAC",
    "payload": {
      "event": "key.rotated",
      "key_id": "5f86a0ef-55c0-4f50-a1e9-b85a2b3db0fe",
      "previous_key_id": "4b2a4d3a-4cbe-4b05-87a3-9528cdf6a1ed",
      "rotated_at": "2025-10-18T12:30:00Z",
      "scope": { "type": "namespace", "namespace": "demo" },
      "owner": "demo",
      "key_prefix": "bkg_demo_rot"
    }
  }
}
```

Fehlerfälle:
- `401` fehlender/ungültiger Token (`AuthError::InvalidToken`).
- `403` Namespace-Keys dürfen nicht rotieren (`AuthError::Unauthorized`).
- `404` unbekannte ID.
- `503` wenn `CAVE_ROTATION_WEBHOOK_SECRET` fehlt (Webhook-Signatur nicht generierbar).

### POST `/api/v1/auth/keys/rotated`
Validiert ein eingehendes Rotation-Webhook-Ereignis. Erwartet identische Payload wie im vorherigen Response und den Header `X-Cave-Webhook-Signature` (Base64-kodiertes HMAC-SHA256 mit `CAVE_ROTATION_WEBHOOK_SECRET`). Erfolgreich mit HTTP 204.

Fehlerfälle:
- `401` ohne Signatur-Header oder mit ungültigem Format (`ApiError::unauthorized`).
- `401` bei Signatur-Mismatch (`AuthError::InvalidSignature`).
- `403` wenn kein Admin-Scope.

---

## Health & Metrics
- `GET /healthz` → 200 bei Erfolg (leer). In Deployment-Healthchecks verwenden.
- `GET /metrics` → `text/plain` Prometheus-Payload, aktuell Platzhalter `bkg_cave_daemon_up 1`.

---

## Fehlervertrag
Jede JSON-Fehlermeldung folgt dem Schema `{"error": "..."}` (`ErrorBody`). Die genaue Nachricht entspricht dem konkreten Fehlerfall (z. B. `"missing Authorization bearer token"`, `"sandbox ... not found"`). Dies spiegelt die Implementierung in `crates/cave-daemon/src/main.rs` wider (`ApiError`).

---

## Tooling & Tests
- Schema generieren: `make api-schema` → schreibt `openapi.yaml`.
- Validierung: `openapi-cli validate openapi.yaml` (CI + lokal) sowie `ajv validate` für `cave.yaml`.
- Funktions- & Negativtests: `cargo test -p cave-daemon` deckt Handler- und Limit-Konvertierungen ab.
- Dokumentation aktuell halten: bei Änderungen an Handlern sowohl diese Datei als auch `docs/cli.md`, `docs/governance.md` und `docs/Progress.md` aktualisieren.

---

SPDX-License-Identifier: Apache-2.0
