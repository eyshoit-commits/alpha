# docs/deployment.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

---

## Ziel
Deployment-Leitfaden für Sandbox-Kernel, Daemon, bkg-db, Admin-UI und unterstützende Infrastruktur.

---

## 1. Infrastruktur (Empfohlen)
- Kubernetes Cluster (>= 1.27) mit PodSecurityStandards.  
- Managed Postgres (RLS aktiviert, PITR).  
- Object Storage (S3-kompatibel) für Audit-Logs & Modelle.  
- Vault/KMS für Secrets (`BKG_API_KEY`, `cosign.key`, TLS).  
- OTEL Collector + Prometheus + Alertmanager.

---

## 2. Komponenten
- **cave-kernel**: Deployment mit seccomp/cgroups Profilen, Zugriff auf `/sys/fs/cgroup`.  
- **cave-daemon**: Exponiert `/api/v1/*`, `/healthz`, `/metrics`; braucht Secrets (DB DSN, JWT).  
- **bkg-db**: Postgres Operator oder StatefulSet; Migration via `sqlx migrate run`.  
- **Admin-UI**: Next.js App (Static Build), served über CDN oder ingress.  
- **Monitoring**: Scrape Targets `/metrics`, Deployment Alerts (CrashLoop, Latency).

---

## 3. Deployment Schritte (high level)
1. **Bootstrap**  
   - Terraform/Helm Templates anwenden (`deploy/` Verzeichnis anlegen — TODO).  
   - Secrets in Vault/KMS erstellen, GitOps Secrets referenzieren.
2. **Database**  
   - Postgres bereitstellen, Migrationen ausführen (`sqlx migrate run`).  
   - RLS-Policies deployen (`docs/governance.md` Plan).
3. **Services**  
   - Kernel + Daemon deployen (rollout restart).  
   - Admin-UI & Web-App bereitstellen.  
4. **Post-Deploy Checks**  
   - `kubectl port-forward` + `curl /healthz`.  
   - `cargo test --package smoke-tests` (TODO) gegen laufende Umgebung.  
   - OTEL/Prometheus Dashboards prüfen.

---

## 4. Release Workflow
- CI generiert SBOM (`syft`), SLSA, cosign Signaturen.  
- Artefakte (Container, SBOM, SLSA) liegen im Registry/Bucket.  
- Git Tag + Release Notes mit Referenzen auf Artefakte.  
- Deployments via GitOps (ArgoCD/Flux) oder Helm Release.

---

## 5. Backlog
- [ ] Helm Chart & Terraform Module veröffentlichen.  
- [ ] Blue/Green Rollout für Daemon mit Traffic Shadowing.  
- [ ] Disaster Recovery Playbook (Postgres, Object Storage, Vault).  
- [ ] Load Testing Plan (k6/gatling) dokumentieren.

---

SPDX-License-Identifier: Apache-2.0
