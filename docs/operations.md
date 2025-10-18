# docs/operations.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

---

## Zweck
Operational Runbooks für den Betrieb der BKG-Plattform (Sandbox, Daemon, bkg-db, Telemetrie).

---

## 1. Tagesgeschäft (Daily Ops)
- Prüfe Dashboards (OTEL, Prometheus, Sandbox-Limits).  
- Bestätige Rotation Jobs (API Keys, JWT, cosign).  
- Kontrolliere Pending Alerts (Healthz, Telemetrie, Storage).  
- Review Audit-Log Feed auf ungewöhnliche Aktionen.

---

## 2. Incident Response (Kurzfassung)
| Incident | Sofortmaßnahme | Follow-up |
|----------|----------------|-----------|
| Sandbox Breakout Verdacht | Box stoppen, Logs sichern, Audit export | Forensics, Policy Update |
| Telemetrie Ausfall | Sampling Rate prüfen, Exporter Restarts | Document in Progress, adjust alerts |
| RLS Fehlkonfiguration | Reads/Mutations blockieren, RLS Policies laden | Migration fix, add tests |
| SBOM Signatur fehlgeschlagen | Release stoppen, cosign.key prüfen | Rotate Key, regenerate SBOM |

Detail-Runbooks in Vorbereitung (siehe Backlog).

---

## 3. Maintenance Tasks
- **Wöchentlich**: Sandbox Cleanup, Audit-Log Archivierung, Telemetrie-Sampling überprüfen.  
- **Monatlich**: RLS Policy Review, Rate-Limit Tuning, Security Tests (`pytest security/`).  
- **Quartalsweise**: Incident-Drills, Disaster Recovery Tests, Postgres Upgrade Evaluation.

---

## 4. Monitoring & Alerts
- CPU/RAM/Disk pro Sandbox (Grenzen: 2 vCPU, 1 GiB RAM, 1 GiB Disk).  
- OTEL Exporter Status (`cave_otlp_exporter_up`).  
- Audit-Log Signaturjob (cosign verify).  
- Postgres Health (Replication Lag, RLS Policy Drift).  
- Object Storage (Bucket WORM, Retention).

---

## 5. Backlog
- [ ] Vollständige Incident-Runbooks erstellen.  
- [ ] Automatisierte Audit-Review Dashboard.  
- [ ] Postgres Failover Simulation dokumentieren.  
- [ ] Seeding-Skripte für Dev/Staging Environments.

---

SPDX-License-Identifier: Apache-2.0
