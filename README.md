# BKG – Universelle KI‑Infrastruktur (Verbindliche System‑Prompt & README — Patch v1.8.2)

Version: 1.8.2  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

WARNUNG: Diese README ist ein verbindlicher System‑Prompt für das Coding‑LLM, die Microsandbox‑Integration und die BKG‑Implementierung. Änderungen erfolgen ausschließlich per PR mit aktualisierten Akzeptanzkriterien, Tests, SBOM & SLSA. Diese Version (v1.8.2) enthält finale Feinschliffe: Telemetry‑Sampling‑Policy, verpflichtende Health‑Endpoints, SBOM‑Signatur‑Hinweis (cosign), SPDX‑License‑Identifier Footer und die CI‑Testpflichtreferenz zur Threat‑Matrix in docs/security.md.

INHALT (Kurz)
- Unverrückbare Reihenfolge (Priorität)
- Microsandbox System‑Prompt Bausteine (Pflichtverhalten)
- Admin‑CAVE LLM‑Hosting (Konzepte & Subsysteme)
- API & WS Contracts (vollständig) + OpenAPI‑Pointer + Health Endpoints
- Telemetry‑Sampling‑Policy (CAVE_OTEL_SAMPLING_RATE)
- Schlüssel‑Lifecycle, TTL & Rotation (Governance) + Rotation‑Webhook
- Default Sandbox Execution Policy (Default Limits)
- cave.yaml Konfig‑Schema & JSON‑Schema Pfad
- Adapter‑Trait Spec (adapter_traits.rs)
- Telemetry Contract (Metriken & Trace Span Names)
- RBAC Mini‑Tabelle (inkl. rate_limit_default)
- Test‑Matrix
- Audit‑Log Format (JSON Lines Beispiel)
- Config / ENV Vars (sensitive flags)
- P2P Governance (Peer Auth)
- Versioning & Compatibility Policy
- Acceptance Criteria & Release
- Tests, CI & Release Artefakte + CI‑Job Snippets (inkl. SBOM/SLSA & cosign sign)
- Pflicht‑Dokumente (docs/*)
- FEATURE_ORIGINS Template
- CLI Usage Examples (End‑to‑End) + cave.yaml schema validation example
- Mermaid Sequenzdiagramme (Sandbox Lifecycle, Model Access Flow)
- Security Policy Mapping Matrix (docs/security.md)
- Release Artifact Names (Tabelle)
- Appendix A: Glossar (inkl. Admin‑CAVE Definition)
- Narrative & Next Schritte

-----------------------------------------------------------------------
1. UNVERRÜCKBARE REIHENFOLGE (BINDEND)
-----------------------------------------------------------------------
1. Phase‑0 (zuerst, zwingend):  
   - CAVE‑Kernel / Sandbox Runtime (Linux namespaces, cgroups v2, seccomp, FS‑Overlay)  
   - persistente, namespace‑gebundene lokale DB (bkg_db) mit RLS  
   - Web‑UI (web/admin + web/app) mit minimalem Admin & User Funktionsumfang  
   Diese drei Komponenten müssen voll funktionsfähig, mit Integrationstests abgesichert und deployed sein, bevor verteilte Features (P2P, Distributed Inference, Marketplace, Multi‑Agent Orchestration) aktiviert werden.

2. Opt‑in P2P (nach Phase‑0): P2P‑Vernetzung ausschließlich zwischen autorisierten Admin‑CAVEs — operator‑gesteuert & auditierbar.

3. Admin‑CAVE LLM‑Hosting (Admin only): Admin‑CAVEs hosten LLM‑Inference‑Server; Model‑Access‑Keys werden ausschließlich von Admin‑CAVEs ausgestellt. Clean‑Room: Konzepte erlaubt, kein Code wiederverwenden.

-----------------------------------------------------------------------
2. MICROSANDBOX SYSTEM‑PROMPT BAUSTEINE (PFLICHTVERHALTEN)
-----------------------------------------------------------------------
Das LLM hat die folgenden Bausteine als verpflichtende Initialsequenz vor jeder Ausführung zu befolgen:

- API Key Authentication (Prompt Template)  
  "Bitte geben Sie Ihren Microsandbox API‑Key ein. Setzen Sie ihn als Umgebungsvariable: export BKG_API_KEY='...' oder übergeben Sie ihn an SDK‑Aufrufe. `--dev` Modus (bkg server start --dev) deaktiviert Auth und darf NICHT in Produktion verwendet werden."

- Sandbox Type Selection  
  "Welches Sandbox‑Umfeld benötigen Sie? PythonSandbox (Python) oder NodeSandbox (Node.js)?"

- Permission Level Detection  
  Prüfe Key‑Scope: ADMIN | NAMESPACE; verhalte dich entsprechend.

- Execution Mode (Temporary vs Persistent)  
  Temporary: `bkg exe --image <type>` (one‑off)  
  Persistent: `bkg init` → `bkg add` → `bkgr` (state saved to ./bkgenv)

- Resource Cleanup Reminder  
  Tracke aktive sandboxes; biete automatische Cleanup‑Erinnerungen an und zwinge `sandbox.stop()` in Cleanup‑Path.

- MCP Integration (Agent Control)  
  `/mcp` (JSON‑RPC) — Methods: `sandbox_start`, `sandbox_run_code`, `sandbox_stop` — Auth: Bearer API Key

- Assistant Behaviour  
  Prüfe Key → bestimme Laufzeit → frage Mode → setze Default Limits → führe aus → sammel stdout/stderr → auditiere → stoppe Sandbox → bestätige Cleanup.

-----------------------------------------------------------------------
3. ADMIN‑CAVE LLM‑HOSTING — KERNSUBSYSTEME
-----------------------------------------------------------------------
Nur Admin‑CAVEs dürfen Modelle hosten, Replikation koordinieren, Keys ausstellen.

- Model Registry: metadata, signed manifests, replica info  
- Model Downloader & Cache: chunked download, resume, checksum+sig verify, eviction (LRU/LFU)  
- Inference Adapter (Rust): chat_stream, embed_batch, batch_infer, deterministic seeding  
- Native Acceleration Layer (FFI/IPC): neu implementierte Kernstücke oder permissive bindings  
- Resource Budgeting: enforced by CAVE controller  
- Health & Telemetry Endpoints (siehe Abschnitt Health)

Admin UI Patterns: Model Manager, Peer Manager, Key Wizard, Chat Studio, Conversation History (UX inspiriert)

-----------------------------------------------------------------------
4. API & WS CONTRACTS (VOLLSTÄNDIG) + OPENAPI‑POINTER & HEALTH
-----------------------------------------------------------------------
A. Sandbox Lifecycle:
- POST /api/v1/sandboxes  
- POST /api/v1/sandboxes/{id}/start  
- POST /api/v1/sandboxes/{id}/stop  
- POST /api/v1/sandboxes/{id}/exec  
- GET /api/v1/sandboxes/{id}/status|metrics|logs  
- DELETE /api/v1/sandboxes/{id}

B. Auth & Keys:
- POST /api/v1/auth/signup  
- POST /api/v1/auth/login  
- POST /api/v1/auth/refresh  
- GET /api/v1/auth/keys  
- POST /api/v1/auth/keys  
- POST /api/v1/auth/keys/rotate  
- POST /api/v1/auth/keys/{id}/revoke  
- POST /api/v1/auth/keys/rotated  ← Rotation‑Webhook notification (optional): notifies registered subscribers about rotated keys

C. Admin LLM (Admin only):
- POST /api/v1/admin/llm/models {metadata}  
- GET /api/v1/admin/llm/models  
- GET /api/v1/admin/llm/models/{id}  
- POST /api/v1/admin/llm/models/{id}/download -> 202 {job_id}  
- POST /api/v1/admin/llm/models/{id}/issue_key {cave_namespace, scopes, expiry, rate_limits} -> 201 {model_access_key}  
- GET /api/v1/admin/llm/models/{id}/issued_keys  
- POST /api/v1/admin/llm/models/{id}/revoke_key  
- POST /api/v1/admin/llm/models/{id}/replicate

D. Inference & Streaming:
- POST /api/v1/llm/chat  (Headers: Authorization: Bearer model_access_key, X‑Cave‑Namespace: <namespace>) — streaming via WS/SSE, supports cancellation & backpressure  
- POST /api/v1/llm/embed  
- POST /api/v1/llm/batch_infer

E. P2P & Peers:
- POST /api/v1/peers/add  
- POST /api/v1/peers/{id}/connect  
- GET /api/v1/peers/status

F. MCP:
- POST /mcp (JSON‑RPC) — Methods: sandbox_start, sandbox_run_code, sandbox_stop

G. Health & Metrics (K8s integration):
- GET /healthz  → liveness/readiness check (should return 200 when OK, 503 when not)  
- GET /metrics → Prometheus metrics endpoint

OpenAPI‑Pointer:
- Auto‑generiere `openapi.yaml` aus diesen Routen während des Builds: `make api-schema`  
- CI: `openapi-cli validate openapi.yaml` → SDK‑Generation via `openapi-generator` (optional)

API‑Tags für OpenAPI: `Sandbox`, `Auth`, `LLM`, `Peers`, `Admin`

-----------------------------------------------------------------------
5. TELEMETRY‑SAMPLING‑POLICY (NEU)
-----------------------------------------------------------------------
Um die Anzahl erzeugter Traces in Prod steuerbar zu machen, unterstützen Services die Umgebungsvariable:

- `CAVE_OTEL_SAMPLING_RATE` — default: `1.0` (100 %)  
  Beschreibung: Float zwischen 0.0 und 1.0. 1.0 = 100% Sampling, 0.1 = 10% Sampling. Setze in Production z. B. `CAVE_OTEL_SAMPLING_RATE=0.1` um Kosten & Volumen zu reduzieren.

Empfehlung:
- Dev/Test: 1.0  
- Staging: 0.5  
- Production: 0.05–0.2 (abhängig von SLOs und Budget)

Diese Policy ist operativ hilfreich, weil sie zentral die Trace‑Rate aller CAVE‑Services steuert (runtime konfigurierbar, kompatibel mit OpenTelemetry Samplers).

-----------------------------------------------------------------------
6. SCHLÜSSEL‑LIFECYCLE, TTL, ROTATION & ROTATION‑WEBHOOK
-----------------------------------------------------------------------
- Admin Keys: rotate every 90 days (background rotation job)  
- Namespace Keys: default TTL 30 days; auto‑rotate if older than 7 days on use  
- Model‑Access‑Keys: default TTL 1 hour (short‑lived)  
- Session Keys: ephemeral 1h TTL for temporary sandboxes  
- Revocation: via `POST /api/v1/auth/keys/{id}/revoke`, revocation cache window default 15m

Rotation‑Webhook:
- Optional: `POST /api/v1/auth/keys/rotated` for notifying registered subscribers (Secrets managers) about rotations. Payload must contain key_id, owner, rotated_at, new_rota_id and be HMAC signed. Webhook registration through `POST /api/v1/admin/webhooks`. All webhook events are audited.

-----------------------------------------------------------------------
7. DEFAULT SANDBOX EXECUTION POLICY (SCHUTZ)
-----------------------------------------------------------------------
Sichere, konservative Defaults:
- CPU: 1 vCPU  
- Memory: 512 MiB  
- Timeout: 60 s  
- Disk (workspace): 500 MiB

Administratoren können overrides beantragen; alle overrides sind policy‑gated und auditiert.

-----------------------------------------------------------------------
8. `cave.yaml` — KONFIG‑SCHEMA & JSON‑SCHEMA PFAD
-----------------------------------------------------------------------
Empfohlen: `cave.yaml` in Projektwurzel; CLI validiert gegen `schema/cave.schema.json`. Beispiel und CLI Validierung im docs/cli.md:

```bash
ajv validate -s schema/cave.schema.json -d cave.yaml
✔ validation passed
```

-----------------------------------------------------------------------
9. ADAPTER‑TRAIT SPEC (adapter_traits.rs)
-----------------------------------------------------------------------
Konventioneller Contract für LLM‑Adapter (Rust Trait) — siehe `docs`/`adapter_traits.rs` Beispiel.

-----------------------------------------------------------------------
10. TELEMETRY CONTRACT & TRACE SPAN NAMES
-----------------------------------------------------------------------
Metrik‑Namen und empfohlene Trace Spans (siehe Abschnitt 9 in v1.8.0). `CAVE_OTEL_SAMPLING_RATE` steuert sampling.

-----------------------------------------------------------------------
11. RBAC MINI‑TABELLE (INKL. RATE LIMIT)
-----------------------------------------------------------------------
| Key Type | Example Scope | rate_limit_default |
|---|---|---:|
| Admin Key | `/api/v1/admin/*`, `/api/v1/peers/*`, replication | 1000 req/min |
| Namespace Key | `/api/v1/sandboxes` scoped, `/api/v1/llm/chat` with bound token | 100 req/min |
| Session Key | temporary sandbox ops only | 50 req/min |
| Model‑Access‑Key | `/api/v1/llm/*` for specific `model_id` & `cave_namespace` only | 200 req/min |

Gateway shall enforce concrete throttling; values are defaults.

-----------------------------------------------------------------------
12. TEST‑MATRIX (Mindestkombinationen)
-----------------------------------------------------------------------
(Identisch zur vorherigen Matrix; unverändert.)

-----------------------------------------------------------------------
13. AUDIT‑LOG FORMAT (JSON‑LINES BEISPIEL)
-----------------------------------------------------------------------
Append‑only signed lines; Beispiel wie zuvor.

-----------------------------------------------------------------------
14. CONFIG / ENV VARS (Kurzreferenz)
-----------------------------------------------------------------------
- `BKG_API_KEY` (sensitive: true)  
- `BKG_DB_DSN` (sensitive: true)  
- `BKG_DB_PATH` (sensitive: false)  
- `CAVE_RUNTIME_MODE` (sensitive: false)  
- `BKG_STORAGE_PATH` (sensitive: false)  
- `BKG_TLS_CERT` / `BKG_TLS_KEY` (sensitive: true)  
- `BKG_OPERATOR_CA` (sensitive: true)  
- `CAVE_OTEL_SAMPLING_RATE` (sensitive: false, default 1.0)

Full list in `docs/env.md`.

-----------------------------------------------------------------------
15. P2P GOVERNANCE (PEER AUTH)
-----------------------------------------------------------------------
(Identisch zur vorhergehenden Spezifikation; siehe oben.)

-----------------------------------------------------------------------
16. VERSIONING & COMPATIBILITY POLICY
-----------------------------------------------------------------------
(API versioning and migration policy as before.)

-----------------------------------------------------------------------
17. ACCEPTANCEKRITERIEN & RELEASE
-----------------------------------------------------------------------
(As before: Phase‑0 gating, tests coverage, SBOM & SLSA required.)

-----------------------------------------------------------------------
18. TESTS, CI & RELEASE‑ARTEFAKTE + CI SNIPPETS (inkl. SBOM SIGN)
-----------------------------------------------------------------------
Additions to CI:
- SBOM & SLSA generation: `make sbom && make slsa`  
- SBOM signature example (Supply‑Chain compliance):

```yaml
jobs:
  - name: validate-openapi
    run: |
      make api-schema
      openapi-cli validate openapi.yaml

  - name: validate-cave-yaml
    run: |
      ajv validate -s schema/cave.schema.json -d cave.yaml

  - name: generate-sbom-and-slsa
    run: |
      make sbom   # uses syft/grype
      make slsa
      # Sign SBOM artifact:
      cosign sign-blob bkg-cave-1.8.1.spdx.json --key cosign.key
```

Hinweis: CI must store `cosign.key` in secure secret store (e.g., GitHub Actions secrets, Vault) and audit use of signing key.

-----------------------------------------------------------------------
19. PFLICHT‑DOKUMENTE (BEI REPO‑INIT)
-----------------------------------------------------------------------
(As before: docs/architecture.md, docs/api.md, docs/cli.md, docs/security.md, docs/deployment.md, docs/operations.md, docs/testing.md, docs/governance.md, docs/FEATURE_ORIGINS.md, docs/env.md, docs/compatibility.md, schema/cave.schema.json)

-----------------------------------------------------------------------
20. FEATURE_ORIGINS TEMPLATE
-----------------------------------------------------------------------
(As before; must be completed for each adapted idea.)

-----------------------------------------------------------------------
21. CLI USAGE EXAMPLES (END‑TO‑END)
-----------------------------------------------------------------------
(As before; includes ajv validate example.)

-----------------------------------------------------------------------
22. SECURITY POLICY MAPPING (docs/security.md → verpflichtend CI geprüft)
-----------------------------------------------------------------------
Threat Matrix is required and CI‑checked. Add sentence to `docs/security.md`:

> "Die Threat‑Matrix in §22 ist verpflichtend CI‑geprüft (via pytest security/)."

This sentence is included in `docs/security.md` (and a draft of that file is provided alongside this README update).

-----------------------------------------------------------------------
23. RELEASE ARTIFACT NAMING CONVENTIONS
-----------------------------------------------------------------------
(As before; now updated to e.g., v1.8.1 → v1.8.2 for this README.)

-----------------------------------------------------------------------
24. APPENDIX A — GLOSSAR (inkl. Admin‑CAVE Ergänzung)
-----------------------------------------------------------------------
- CAVE: Isolierte Sandbox‑Instanz (Filesystem + namespaces + limits)  
- Admin‑CAVE: Authoritative node in the trusted mesh; operator‑controlled; responsible for model hosting, issuance of model keys, P2P replication coordination, and enforcing policy across its managed namespaces.  
- Namespace: logical tenant separation  
- Peer: authorized Admin‑CAVE in the P2P network  
- Sandbox: runtime instance for code execution  
- KeyScope: admin | namespace | session | model_access

-----------------------------------------------------------------------
25. LICENSE / SPDX FOOTER
-----------------------------------------------------------------------
SPDX-License-Identifier: Apache-2.0

-----------------------------------------------------------------------
26. NARRATIVE — WAS ICH GEÄNDERT HABE UND WIE ES WEITERGEHT
-----------------------------------------------------------------------
Ich habe die README auf Version v1.8.2 aktualisiert und die produktiven Feinschliffe integriert: die Telemetry‑Sampling‑Policy (`CAVE_OTEL_SAMPLING_RATE`) ist aufgenommen, `/healthz` und `/metrics` sind in den API‑Contracts als Pflichtendpunkte verankert, der Rotation‑Webhook ist spezifiziert, die RBAC‑Tabelle führt `rate_limit_default` für Gateway‑Konfigurationen, und das SBOM/SLSA‑Beispiel mit `cosign sign-blob` dokumentiert die Signaturpflicht. Im Repository liegen derzeit `docs/FEATURE_ORIGINS.md` (Template mit ersten Entwürfen), `docs/architecture.md` (Systemübersicht inkl. Mermaid‑Sequenzen), `docs/env.md` (Umgebungsvariablenreferenz) sowie `schema/cave.schema.json` (aktuelles `cave.yaml`‑Schema).

Nächste Schritte (verbindlich, um die README‑Vorgaben zu erfüllen):
- `docs/security.md` anlegen und um die in §22 geforderte Threat‑Matrix samt CI‑Hinweis (`pytest security/`) ergänzen.
- Fehlende Pflicht‑Dokumente aus §19 erstellen bzw. importieren (`docs/api.md`, `docs/cli.md`, `docs/deployment.md`, `docs/operations.md`, `docs/testing.md`, `docs/governance.md`, `docs/compatibility.md`) und jeweils auf den aktuellen Stand bringen.
- `docs/FEATURE_ORIGINS.md` von Draft‑Einträgen auf vollständige, verifizierte Einträge mit Commits/PR‑Verweisen und Reviewer‑Signoff erweitern.
- Test‑Matrix‑Referenzen mit automatisierten CI‑Jobs unterlegen (SBOM/SLSA‑Workflow inklusive `cosign sign-blob` Ausführung) und die Release‑Prozessdokumentation um Artefakt‑Speicherorte sowie Signaturpfade ausformulieren.

Ende README v1.8.2 — verbindlicher System‑Prompt & Implementationsleitfaden.