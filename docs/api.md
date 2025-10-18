# docs/api.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

---

## Zweck
REST, WebSocket und pgwire Schnittstellenübersicht für die BKG-Plattform. Ergänzt `README.md` und `docs/bkg-db.md`.

---

## HTTP Endpunkte (Auszug)
- `/api/v1/sandboxes` – POST (create), GET (list).  
- `/api/v1/sandboxes/{id}/start|exec|stop|status|executions` – Lifecycle.  
- `/api/v1/auth/keys` – Issue/List.  
- `/api/v1/auth/keys/{id}` – DELETE revoke.  
- `/api/v1/auth/keys/rotated` – Webhook (HMAC).  
- `/api/v1/admin/llm/models/*` – Admin LLM Management.  
- `/healthz`, `/metrics` – Liveness/Telemetry.

> TODO: Ergänze detailierte Request/Response Schemas und Fehlercodes; generiere `openapi.yaml` innerhalb `make api-schema`.

---

## WebSocket / SSE
- `/api/v1/sandboxes/{id}/logs` (geplant) – Stream von stdout/stderr.  
- `/api/v1/llm/chat` – Token-Streaming (SSE/WS).  
- `/realtime` – WAL-basierte Events (siehe `docs/bkg-db.md`).

---

## pgwire (geplant)
- Listener auf Standard-Port (z. B. 54321).  
- Unterstützt Simple Queries (`SELECT`, `INSERT`, `UPDATE`, `DELETE`).  
- Auth via JWT (Namespace/Scope).  
- Erweiterte Features (Portals, Batches) nach RLS-Abnahme.

---

## gRPC (Backlog)
- Services: `SandboxService`, `AuthService`, `RealtimeService`.  
- IDL/PB-Dateien im Verzeichnis `proto/` (anzulegen).  
- SLSA Artefakte müssen generierte Clients signieren.

---

## Tests & Validierung
- `openapi-cli validate openapi.yaml` in CI.  
- Integrationstests (`cargo test -p cave-daemon`) für kritische Endpunkte.  
- Negative Tests (unauthentifizierte Anfragen, Scope-Verletzungen).  
- `pytest security/` – prüft Auth-/RLS-Bedingungen.

---

SPDX-License-Identifier: Apache-2.0
