# BKG Coding Prompt (v1.8.2)

**Quelle:** `README.md` (verbindlicher System-Prompt, Patch v1.8.2)  
**Ziel:** Konsistente Anweisungen für Coding-Workflows innerhalb der BKG Microsandbox.

## Prioritäten & Reihenfolge
- **Phase-0 muss zuerst fertig sein**: CAVE-Kernel/Sandbox Runtime, persistente `bkg_db` (RLS), Web-UI (admin & user). Erst danach P2P, verteilte Inferenz, Marketplace, Multi-Agent.
- P2P nur opt-in und ausschließlich zwischen autorisierten Admin-CAVEs.
- LLM-Hosting ist Admin-CAVE-only; Clean-Room-Implementierungen, kein Code-Reuse.

## Codex-Agent Regeln
- README v1.8.2 ist verbindlicher System-Prompt; Änderungen nur via PR mit Tests/SBOM/SLSA.
- Arbeite ausschließlich innerhalb der Microsandbox-Richtlinien: keine destruktiven Git-Befehle, `apply_patch` für zielgerichtete Edits, ASCII bevorzugen.
- Nutze das Plan-Tool bei mehrschrittigen Aufgaben, halte den Plan aktuell und schließe ihn ab.
- Prüfe vor jedem Arbeitsschritt `Progress.md` und halte Status/Owners aktuell.
- Erstelle oder aktualisiere notwendige Dokumentation sofort (z. B. PROMPT.md, Progress.md, docs/*).
- Führe Tests/Lints aus, wenn sie verfügbar sind; dokumentiere, wenn Tests nicht liefen oder fehlen.
- Respektiere Sandbox-Limits und führe Cleanup-Kommandos, falls genutzt, nach der Arbeit aus.

### BKG-DB Fokus
- Folge zusätzlich den Vorgaben in `docs/bkg-db.md` (Supabase-ähnlicher Vollstack).  
- Vor jeder Arbeit an `crates/bkg-db` oder `web/admin` prüfen, ob Kernel/SQL/Auth/Realtime Deliverables eingeplant sind und in `Progress.md` dokumentiert wurden.  
- CI-Ziele aus dem BKG-DB Prompt (`make lint/test/sbom/slsa/sign`, `api-validate`) einplanen, sobald die entsprechenden Komponenten existieren.

## Arbeitsworkflow für Codex
1. **Kontext holen** – README, PROMPT, `Progress.md` und relevante Docs lesen; offene Fragen notieren.
2. **Planen** – Kurzen Arbeitsplan formulieren, Abstimmung mit Phase-0 Prioritäten prüfen.
3. **Implementieren** – Änderungen mit `apply_patch` (oder passenden Tools) durchführen, Regeln & Limits beachten.
4. **Validieren** – Tests/Checks ausführen, Ergebnisse zusammenfassen; bei fehlenden Tests Alternativen dokumentieren.
5. **Dokumentieren** – `Progress.md` aktualisieren, ggf. weitere Statusfiles (Agents, Docs) pflegen.
6. **Berichten** – Abschlussnachricht mit Änderungen, Status, offenen Follow-ups und empfohlenen nächsten Schritten verfassen.

## Pflichtverhalten für Microsandbox-Assistenten
- API-Key abfragen (`export BKG_API_KEY=...`), kein `--dev` in Produktion.
- Sandbox-Typ ermitteln (PythonSandbox | NodeSandbox) und Berechtigungslevel (ADMIN | NAMESPACE).
- Ausführungsmodus festlegen: `bkg exe` (temporary) vs. `bkg init` → `bkg add` → `bkgr` (persistent).
- Limits anwenden (CPU 1 vCPU, RAM 512 MiB, Timeout 60 s, Disk 500 MiB) und Cleanup erzwingen (`sandbox.stop()`).
- MCP-Integration via `/mcp` JSON-RPC (`sandbox_start`, `sandbox_run_code`, `sandbox_stop`) mit Bearer Auth.
- Ablauf: Key prüfen → Modus bestätigen → Limits setzen → ausführen → stdout/stderr sammeln → auditieren → Sandbox stoppen → Cleanup bestätigen.

## Kernsubsystme & Governance
- Admin-CAVEs verwalten Model Registry, Download/Cache, Inference-Adapter (Rust), Native Acceleration und Resource Budgeting.
- Governance: Schlüsselrotation (Admin 90d, Namespace 30d, Model-Access 1h, Session 1h), Revocation via `/api/v1/auth/keys/{id}/revoke`, optional Rotation-Webhook (`/api/v1/auth/keys/rotated`, HMAC-signiert).
- RBAC Defaults: Admin 1000 req/min, Namespace 100 req/min, Session 50 req/min, Model-Access 200 req/min.
- BKG-DB Deliverables: MVCC Kernel, SQL-Pipeline, RLS, pgwire/HTTP APIs, Realtime/CDC, Objekt-Storage, Admin-UI, Telemetry/Audit, CI/Supply Chain – siehe `docs/bkg-db.md` für Details.

## API, Health & Telemetrie
- Sandbox-Endpoints: `/api/v1/sandboxes` (create/start/stop/exec/status/logs/delete).
- Auth & Keys: Signup/Login/Refresh/Keys CRUD inkl. Rotation-Webhook.
- Admin LLM: Modellverwaltung, Download-Jobs, Key-Issuance, Revoke, Replicate.
- Inferenz: `/api/v1/llm/chat|embed|batch_infer` (Bearers mit Model-Access-Key, `X-Cave-Namespace` Header); Streaming über WS/SSE mit Backpressure.
- P2P/Peers: `add`, `connect`, `status`.
- MCP: `/mcp` JSON-RPC.
- Health & Metrics: `/healthz` (200 vs. 503) & `/metrics` (Prometheus).
- Telemetrie: `CAVE_OTEL_SAMPLING_RATE` (Float 0.0–1.0, Default 1.0) pro Umgebung steuern.

## Release, Tests & Artefakte
- Builds generieren `openapi.yaml` via `make api-schema`; in CI `openapi-cli validate openapi.yaml`.
- CI Jobs: `ajv validate -s schema/cave.schema.json -d cave.yaml`, `make sbom`, `make slsa`, `cosign sign-blob <SBOM> --key cosign.key`.
- Threat-Matrix aus `docs/security.md` wird via `pytest security/` geprüft.
- Release-Artefakte müssen SBOM/SLSA enthalten und signiert werden (cosign).

## Dokumentation & Templates
- Pflichtdokumente: `docs/architecture.md`, `docs/api.md`, `docs/cli.md`, `docs/deployment.md`, `docs/operations.md`, `docs/testing.md`, `docs/security.md`, `docs/governance.md`, `docs/compatibility.md`, `docs/FEATURE_ORIGINS.md`, `docs/env.md`, `schema/cave.schema.json`.
- Agenten-Playbook: maßgeblich `AGENTS.md` im Repo-Wurzelverzeichnis.
- `docs/FEATURE_ORIGINS.md` mit vollständigen, verifizierten Einträgen (Commit/PR-Referenzen, Reviewer-Signoff).
- `cave.yaml` validieren gegen `schema/cave.schema.json`.

## Arbeitsweise & Verbote
- README nicht bearbeiten; Änderungen nur via PR mit aktualisierten Tests, SBOM, SLSA.
- Security-first: Audit-Logs als signierte JSON-Lines, Peer Auth streng nach Spezifikation, Clean-Room Vorgaben respektieren.
- Vor Implementierung stets Phase-0 Verpflichtungen und Dokumentationsstatus prüfen.
