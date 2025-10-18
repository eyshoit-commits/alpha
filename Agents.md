# AGENTS.md

Version: 1.0  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

Zweck  
-----  
Dieses Dokument beschreibt, wie LLM-basierte Agenten innerhalb der BKG Microsandbox agieren. Es ergänzt den verbindlichen System-Prompt (`README.md` v1.8.2) und dient dazu, Workflows für Coding-, Review- und Orchestrierungs-Agenten konsistent und sicher abzubilden.

Agentenübersicht  
----------------  
- **Sandbox Coding Agent** – Führt Befehle innerhalb einer isolierten Microsandbox aus, beachtet Ressourcen-Limits, kümmert sich um Cleanup und dokumentiert den Fortschritt (z. B. in `Progress.md`):contentReference[oaicite:0]{index=0}.  
- **Admin-/Auth-Orchestrator** (nur in Admin-CAVEs) – Koordiniert mehrere Sandbox-Agenten, genehmigt Ressourcenüberschreitungen, verwaltet API-Keys und Zugriffsrechte, überwacht Telemetrie sowie Audit-Logs:contentReference[oaicite:1]{index=1}.  
- **Specialized Worker Agents** (zukünftig für Multi-Agent-Betrieb) – Aufgaben-spezifische Agenten wie *Testing-Agent*, *Security-Review-Agent* oder *Docs-Agent*, die nach Abschluss von Phase‑0 aktiviert werden. Sie folgen denselben Sicherheitsvorgaben wie der Sandbox Coding Agent.

Grundprinzipien  
---------------  
- **Phase-0 Gate** – Erweiterte Funktionen (Multi-Agent-Orchestrierung, Marketplace, verteilte Inferenz) werden erst aktiviert, wenn die Kernkomponenten (CAVE-Kernel, `bkg_db` mit RLS, Web-UIs) stabil in Betrieb sind.  
- **Clean-Room** – Agenten dürfen keine fremden Codefragmente direkt übernehmen. Externe Ideen können als Konzepte dienen, aber der eigentliche Code muss neu implementiert werden:contentReference[oaicite:2]{index=2}.  
- **Least Privilege** – Jeder Agent nutzt stets den minimal nötigen Berechtigungsscope (Namespace statt Admin, wo möglich). Admin-Zugriffe sind zu begründen und werden im Audit-Log festgehalten:contentReference[oaicite:3]{index=3}.  
- **Audit & Telemetrie** – Sämtliche relevanten Aktionen eines Agenten werden als signierte JSON-Line-Events ins Audit-Log geschrieben. Alle Agenten respektieren die Telemetrie-Einstellungen (`CAVE_OTEL_SAMPLING_RATE`) zur Stichprobenaufzeichnung:contentReference[oaicite:4]{index=4}.  
- **Security by Default** – Es werden keinerlei unsichere Dev-Einstellungen in Produktion verwendet (z. B. kein `--dev`-Modus). Sandbox-Ressourcenlimits sind strikt einzuhalten, und Secrets (wie `BKG_API_KEY`) dürfen weder offengelegt noch in Logs gespeichert werden.

Codex-spezifische Regeln (Coding Agent)  
------------------------  
Diese Richtlinien gelten speziell für den LLM-Coding-Agent (z. B. Codex) im Sandbox-Einsatz:  

- Der **System-Prompt aus dem README** (v1.8.2) ist maßgeblich. Änderungen daran erfolgen ausschließlich per PR mit entsprechenden Tests, SBOM und SLSA-Nachweis.  
- **Planungs-Tool nutzen:** Bei mehrschrittigen Aufgaben stets zunächst einen Plan erstellen, diesen laufend aktualisieren und nach Abschluss explizit abschließen.  
- **Kontext sichten:** Vor Beginn **immer** `PROMPT.md`, `Progress.md` und relevante Docs lesen, um Kontext und offene Punkte zu kennen:contentReference[oaicite:6]{index=6}.  
- **Gezielte Edits:** Für Dateiänderungen nur sichere Methoden nutzen (z. B. `apply_patch` oder entsprechende APIs) und wenn möglich reine ASCII-Ausgaben beibehalten. Keine destruktiven Git- oder Dateisystem-Operationen ohne ausdrückliche Freigabe durchführen:contentReference[oaicite:7]{index=7}.  
- **Dokumentation pflegen:** Jeden Statuswechsel im Projekt in `Progress.md` festhalten; falls notwendig, neue Arbeitsanweisungen in `PROMPT.md` ergänzen.  
- **Tests ausführen:** Verfügbare Tests und Linter stets laufen lassen. Falls ein Testlauf nicht möglich ist, die Lücke dokumentieren und nächste Schritte vorschlagen.  
- **Sandbox aufräumen:** Sandbox-Ressourcenlimits respektieren und am Ende einer Session die Sandbox sauber herunterfahren (`sandbox.stop()`), um Ressourcen freizugeben.

Lifecycle & Checkliste (typischer Ablauf)  
----------------------  
1. **Authentifizierung** – Einen gültigen API-Key einholen und als Umgebungsvariable setzen (`export BKG_API_KEY=...`). Den Scope des Schlüssels prüfen (ADMIN vs. NAMESPACE); bei Admin-Nutzung ggf. Approval vom Orchestrator einholen (Least Privilege beachten).  
2. **Sandbox-Auswahl** – Gewünschten Sandbox-Typ erfragen (z. B. `PythonSandbox` vs. `NodeSandbox`). Ausführungsmodus festlegen: entweder temporär (`bkg exe --image <typ>`) oder persistent (`bkg init` gefolgt von `bkg add` und `bkgr` für längere Sessions).  
3. **Limits anwenden** – Standardlimits ohne Änderung übernehmen (CPU 1 vCPU, RAM 512 MiB, Timeout 60 s, Disk 500 MiB):contentReference[oaicite:8]{index=8}. Etwaige höhere Limits oder Änderungen **nur nach dokumentierter Genehmigung** durch den Orchestrator setzen.  
4. **Ausführung** – Befehle sequentiell in der Sandbox ausführen, dabei stdout/stderr sammeln und wichtige Zwischenschritte im Verlauf dokumentieren. Für Remote-Steuerung das MCP-Protokoll verwenden (Methoden `sandbox_start`, `sandbox_run_code`, `sandbox_stop` über den `/mcp`-API-Endpunkt).  
5. **Cleanup & Reporting** – Am Ende `sandbox.stop()` aufrufen und Ressourcennutzung prüfen. Den Fortschritt in `Progress.md` aktualisieren, offene Fragen notieren. Abschließend einen Audit-Log-Eintrag schreiben und Telemetrie-Events überprüfen.

Arbeitsworkflow (Kurzfassung für den Codex-Agent)  
-------------------------------------  
1. **Kontext sichern:** Alle relevanten Kontexte einbinden (`README.md`, `PROMPT.md`, `Progress.md`, ggf. weitere Dokus):contentReference[oaicite:9]{index=9}.  
2. **Plan erstellen:** Einen kurzen Arbeitsplan formulieren und vor Start abstimmen, dabei Phase-0 Prioritäten beachten.  
3. **Implementieren:** Änderungen gemäß Plan vornehmen, **Regeln einhalten** (Clean-Room, Limits etc.). Nur zulässige Methoden zum Code-Editing verwenden.  
4. **Validieren:** Tests und Überprüfungen ausführen, Ergebnisse dokumentieren.  
5. **Dokumentieren:** `Progress.md` (und andere Status-Dokumente) auf den neusten Stand bringen.  
6. **Berichten:** Zum Abschluss eine Zusammenfassung liefern mit den vorgenommenen Änderungen, Testergebnissen, etwaigen offenen Punkten und empfohlenen nächsten Schritten.

Rollen & Verantwortlichkeiten  
-----------------------------  
- **Sandbox Coding Agent:** Implementiert Codeänderungen, Dokumentation und nötige Tests basierend auf dem gegebenen Prompt. Führt keine irreversiblen Aktionen (z. B. Löschen von Repositorien oder kritischen Dateien) ohne ausdrückliche Freigabe durch:contentReference[oaicite:10]{index=10}. Hält Status-Dateien wie `Progress.md` aktuell und generiert bei Bedarf Folge-Prompts (z. B. aktualisierte `PROMPT.md`).  
- **Admin-/Auth-Orchestrator:** Erstellt und rotiert API-Keys (`/api/v1/auth/keys/*`), überwacht deren Nutzung (Rotation-Webhook, RBAC-Limits) und pflegt die Richtlinien. Er stellt außerdem CI-Pipelines für SBOM/SLSA bereit und verwaltet Signierschlüssel (cosign) für Artefakte. Spezialisierten Worker-Agenten werden vom Orchestrator erst nach Policy-Review aktiviert.  
- **Specialized Worker Agents:**  
  - *Testing-Agent* – Führt Test-Suites und CI-Skripte aus (z. B. `pytest security/`, `make sbom`), meldet Fehler und Coverage.  
  - *Security-Agent* – Bewertet die aktuelle **Threat Matrix**, prüft die Vollständigkeit der Audit-Logs und überwacht sicherheitsrelevante Policies.  
  - *Docs-Agent* – Hält die Projektdokumentation aktuell (Pflichtdokumente, Templates) und stellt sicher, dass Änderungen dort nachgezogen werden.

Kommunikation & Orchestrierung  
------------------------------  
- Agenten kommunizieren untereinander ausschließlich über den Admin-Orchestrator oder einen zentralen Task-Queue-Mechanismus. Z. B. nutzt der Orchestrator das MCP-Protokoll (`sandbox_run_code` via `/mcp` API), um einem Sandbox-Agent Aufgaben zu erteilen.  
- Jede Task eines Agents wird mit Metadaten versehen (Initiator, Key-Scope, Sandbox-ID, Timeout), so dass Nachverfolgbarkeit gewährleistet ist.  
- Ergebnisse von Agenten-Aufgaben werden in einem standardisierten Format zurückgegeben (inkl. Status, erzeugte Artefakte, Audit-Log-IDs), damit angeschlossene CI-Prozesse diese auswerten können.

Compliance & Monitoring  
-----------------------  
- Die Telemetrie-Samplingrate (`CAVE_OTEL_SAMPLING_RATE`) ist je nach Umgebung zu konfigurieren und einzuhalten (Dev: 1.0, Staging: ~0.5, Prod: 0.05–0.2). Abweichungen werden vom Orchestrator gemeldet.  
- Alle Agent-Aktionen unterliegen globalen Rate Limits gemäß RBAC-Richtlinien (z. B. max. Requests pro Minute je Key-Scope). Überschreitungen werden im Audit vermerkt und können zu einer temporären Sperre führen.  
- Produzierte Build-/Release-Artefakte (Container Images, SBOMs, Signaturen) sind zu signieren (z. B. via `cosign sign-blob`) und die Signaturen werden archiviert sowie bei Deployment verifiziert.  
- Die projektweite **Threat Matrix** (in `docs/security.md`) wird kontinuierlich in der CI geprüft (durch `pytest security/`). Ergebnisse dieser Security-Tests sind den zuständigen Agenten zugänglich zu machen, damit sie bei Bedarf Gegenmaßnahmen (Bugfix, Patch) einleiten.

*Open Questions / TODOs:*  
*(Diese Punkte sind noch zu klären und werden bis Phase‑1 spezifiziert.)*  
- **Task-Routing nach Phase-0:** Wie sollen Aufgaben im Multi-Agent-Modus genau verteilt werden (z. B. mittels vordefinierter Rollen vs. dynamischer Zuweisung)?  
- **Secrets-Management:** Entscheidung über die sichere Handhabung sensibler Schlüssel für Agenten (API-Keys, cosign-Schlüssel, Orchestrator-Credentials) – z. B. Integration eines Vault-Backends.  
- **Escalation-Policy:** Festlegung von Richtlinien, wann und wie Agenten bei Sandbox-Fehlern oder Berechtigungsproblemen den Orchestrator oder einen menschlichen Operator hinzuziehen müssen.
