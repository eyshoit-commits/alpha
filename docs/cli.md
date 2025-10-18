# docs/cli.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

---

## Zweck
Überblick über CLI-Werkzeuge (`bkg`, `cavectl`) zur Verwaltung von Sandboxes, Keys und Deployments.

---

## Kernbefehle (`bkg`)
- `bkg init` – Projektstruktur und `cave.yaml` anlegen.  
- `bkg add <sandbox>` – Sandbox definieren (Runtime, Limits).  
- `bkg exe --image <runtime>` – Einmaliger Sandbox-Lauf.  
- `bkgr` – Persistent Sandbox (Quick Connect).  
- `bkg schema validate` – `cave.yaml` gegen `schema/cave.schema.json` prüfen (TODO implementieren).  
- `bkg auth rotate` – Schlüsselrotation auslösen, gibt neues Token + `rotated_from`/`rotated_at` Metadaten und die HMAC-Signatur des Webhook-Events (`event_id`, `signature`).

## Admin CLI (`cavectl`)
- `cavectl sandbox list|start|stop` – Verwaltungsbefehle (Admin-Scope erforderlich).  
- `cavectl key issue --scope namespace --ttl 30d` – API-Keys.  
- `cavectl telemetry set --sampling 0.1` – OTEL Sampling Rate setzen.  
- `cavectl audit export` – Audit-Log JSONL export.  
- `cavectl migrate postgres` – Wrapper für Postgres-Migrationen (folgt Plan in `docs/governance.md`).

> TODO: CLI-Kommandos verifizieren, wenn Implementierung steht; Auto-Completion & Hilfe hinzufügen.

---

## Auth & Konfiguration
- CLI erwartet `BKG_API_KEY` (Namespace/Admin).  
- CLI liest `cave.yaml` (Projektsettings).  
- Override via Flags (`--cpu`, `--memory`, `--timeout`).  
- Secrets kommen aus Vault/KMS (siehe `docs/governance.md`).

---

## Roadmap / Backlog
- [ ] CLI Tests (snapshot-basiert, z. B. mit `assert_cmd`).  
- [ ] Packaging (Homebrew/Tarballs).  
- [ ] Telemetry Opt-In (CLI Usage Metrics anonymisiert).  
- [ ] Integration mit Admin-UI (Deep Links).

---

SPDX-License-Identifier: Apache-2.0
