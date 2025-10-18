# docs/Agents.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

Zweck
-----
Dieses Dokument beschreibt, wie LLM-basierte Agents innerhalb der BKG Microsandbox agieren. Es ergänzt den verbindlichen System-Prompt (`README.md` v1.8.2) und dient dazu, Workflows für Coding-, Review- und Orchestrierungs-Agents konsistent und sicher abzubilden.
Für die ausführliche, laufend gepflegte Regelbasis siehe zusätzlich `AGENTS.md` im Repository-Wurzelverzeichnis.

Agentenübersicht
----------------
- **Sandbox Coding Agent**: Führt Befehle innerhalb einer Microsandbox durch, hält sich an Limits, kümmert sich um Cleanup und dokumentiert Fortschritt (z. B. `Progress.md`).
- **Admin-Orchestrator** (Admin-CAVE only): Koordiniert mehrere Sandbox Agents, genehmigt Ressourcen, verwaltet API-Keys, überwacht Telemetrie und Audit-Logs.
- **Specialized Workers** (zukünftig, Multi-Agent-Orchestration): Aufgaben-spezifische Agents (Testing, Security Review, Docs), die nach Abschluss von Phase‑0 aktiviert werden dürfen. Alle Worker folgen denselben Sicherheitsvorgaben wie der Sandbox Coding Agent.

Grundprinzipien
---------------
- **Phase-0 Gate**: Multi-Agent-Orchestrierung, Marketplace oder Distributed Inference werden erst aktiviert, wenn CAVE-Kernel, `bkg_db` (RLS) und Web-UIs produktionsreif sind.
- **Clean-Room**: Agents dürfen keine fremden Codebasen übernehmen. Konzepte sind erlaubt, Implementierungen müssen neu erstellt werden.
- **Least Privilege**: Agents arbeiten mit dem kleinstmöglichen Key-Scope (ADMIN vs. NAMESPACE). Admin-Zugriffe sind zu begründen und zu loggen.
- **Audit & Telemetrie**: Jeder Agent schreibt relevante Aktionen ins Audit-Log (JSON Lines, signiert) und respektiert `CAVE_OTEL_SAMPLING_RATE`.
- **Security by Default**: Keine Nutzung von `--dev` in Produktion, Sandbox-Limits sind verbindlich, Secrets wie `BKG_API_KEY` bleiben vertraulich.

Codex-spezifische Regeln
------------------------
- `README.md` (v1.8.2) ist der dominante System-Prompt; Änderungen erfolgen ausschließlich via PRs mit Tests/SBOM/SLSA.
- Nutze das Plan-Tool für mehrschrittige Aufgaben, halte den Plan aktuell und schließe ihn nach Abschluss.
- Bevor du beginnst, lies `PROMPT.md`, `Progress.md` und relevante Dokumente, um Kontext und offene Aufgaben zu verstehen.
- Nutze `apply_patch` (oder gleichwertige zielgerichtete Methoden) für Dateiänderungen, halte dich an ASCII-Default.
- Dokumentiere jeden Statuswechsel in `Progress.md` und ergänze ggf. Arbeitsanweisungen in `PROMPT.md`.
- Führe verfügbare Tests/Lints aus; wenn nicht möglich, dokumentiere die Lücke und vorgeschlagene nächste Schritte.
- Respektiere Sandbox-Limits und führe Cleanup (`sandbox.stop()`) durch, bevor du die Sitzung beendest.

Lifecycle & Checkliste
----------------------
1. **Authentifizierung**  
   - API-Key einholen und als Umgebungsvariable setzen (`export BKG_API_KEY=...`).  
   - Key-Scope prüfen (ADMIN | NAMESPACE); ggf. Verwaltungs-Freigabe einholen.
2. **Sandbox-Auswahl**  
   - Sandbox-Typ (Python | Node.js) erfragen.  
   - Ausführungsmodus bestimmen: `bkg exe --image <type>` (temporär) oder `bkg init` → `bkg add` → `bkgr` (persistent).
3. **Limits anwenden**  
   - Standardlimits akzeptieren (CPU 1 vCPU, RAM 512 MiB, Timeout 60 s, Disk 500 MiB).  
   - Overrides nur nach dokumentierter Freigabe setzen.
4. **Ausführung**  
   - Befehle sequentiell ausführen, stdout/stderr sammeln, Zwischenschritte dokumentieren.  
   - MCP nutzen (`sandbox_start`, `sandbox_run_code`, `sandbox_stop`) bei Remote-Steuerung.
5. **Cleanup & Reporting**  
   - `sandbox.stop()` aufrufen, Ressourcen überprüfen.  
   - Fortschritt in `Progress.md` aktualisieren, offene Fragen notieren.  
   - Audit-Event und Telemetrie bestätigen.

Arbeitsworkflow Erinnerung (Codex)
----------------------------------
1. Kontext sichern (`README.md`, `PROMPT.md`, `Progress.md`, relevante Docs).  
2. Kurzen Arbeitsplan formulieren und veröffentlichen.  
3. Änderungen implementieren, Regeln (Clean-Room, Limits) beachten.  
4. Tests/Validierungen ausführen und Ergebnisse dokumentieren.  
5. `Progress.md` und ggf. weitere Statusdokumente aktualisieren.  
6. Abschlussbericht inkl. Änderungen, Teststatus, offenen Punkten liefern.

Rollen & Verantwortlichkeiten
-----------------------------
- **Sandbox Coding Agent**
  - Liefert Codeänderungen, Dokumentation und Tests im Rahmen des Prompts.  
  - Führt keine destruktiven Git- oder FS-Operationen ohne ausdrückliche Freigabe aus.  
  - Aktualisiert Status-Dateien (z. B. `Progress.md`) und erzeugt neue Prompts (z. B. `PROMPT.md`) bei Bedarf.
- **Admin-Orchestrator**
  - Erstellt/rotieren API-Keys (`/api/v1/auth/keys/*`), verwaltet Rotation-Webhook, pflegt RBAC Limits.  
  - Stellt SBOM/SLSA Pipelines, cosign Keys und CI-Prüfungen bereit.  
  - Aktiviert Specialized Workers erst nach Policy-Review.
- **Specialized Worker Agents**
  - Testing-Agent: führt Tests/CI-Skripte aus, prüft `pytest security/`, `make sbom`, etc.  
  - Security-Agent: bewertet Threat-Matrix, Validiert Audit-Log-Vollständigkeit.  
  - Docs-Agent: aktualisiert Pflichtdokumente und Templates.

Kommunikation & Orchestrierung
------------------------------
- Agents kommunizieren über den Admin-Orchestrator oder über einen Task-Queue-Mechanismus (z. B. MCP-Protokoll mit `sandbox_run_code`).  
- Jeder Task wird mit Metadaten versehen (Initiator, Key-Scope, Sandbox-ID, Timeout).  
- Ergebnisse werden standardisiert zurückgeliefert (Status, Artefakte, Audit-IDs), um CI-Checks anzustoßen.

Compliance & Monitoring
-----------------------
- `CAVE_OTEL_SAMPLING_RATE` pro Umgebung dokumentieren und überwachen (Dev: 1.0, Staging: 0.5, Prod: 0.05–0.2).  
- Alle Agent-Aktionen unterliegen Rate-Limits gemäß RBAC-Tabelle.  
- SBOM/SLSA Artefakte signieren (`cosign sign-blob`), Signaturen archivieren und verifizieren.  
- Threat-Matrix in `docs/security.md` ist CI-pflichtig (`pytest security/`), Ergebnisse den Agents zugänglich machen.

Open Questions / TODOs
----------------------
- Definition der Task-Routing-Logik für Multi-Agent-Orchestrierung (nach Phase-0).  
- Entscheidung über Secrets-Management für API-Keys, `cosign.key` und Orchestrator-Creds.  
- Festlegung von Escalation-Policies für Agents bei Sandbox-Fehlern oder fehlenden Berechtigungen.

SPDX-License-Identifier: Apache-2.0
