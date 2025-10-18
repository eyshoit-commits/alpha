# docs/security.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

---

## Zweck
Threat-Matrix und Sicherheitsrichtlinien für BKG. CI muss `pytest tests/security/` gegen die Matrix ausführen (siehe README §22).

---

## Threat Matrix (Auszug)

| Kategorie | Bedrohung | Auswirkung | Gegenmaßnahmen | Tests |
|-----------|-----------|------------|----------------|-------|
| Sandbox Isolation | Breakout via fehlende seccomp Regeln | Remote Code Execution auf Host | seccomp Profile, Namespaces, cgroups, FS Overlay | `pytest tests/security/test_threat_matrix.py::test_category_mitigations_include_required_terms` |
| API Auth | Kompromittierte Namespace Keys | Cross-Tenant Access | Kurze TTLs, Rotation Webhook, RLS | `pytest tests/security/test_threat_matrix.py::test_category_mitigations_include_required_terms` |
| Supply Chain | Manipulierte SBOM/SLSA Artefakte | Supply-Chain Angriff | SBOM (Syft), SLSA Provenance, cosign Signaturen, Sigstore Verification | `make sbom`, `make slsa`, `cosign verify` |
| Telemetrie | Exfiltration über OTEL Export | Datenabfluss | Sampling Policy (`CAVE_OTEL_SAMPLING_RATE`), Exporter Egress-Allowlist | `pytest tests/security/test_threat_matrix.py::test_threat_matrix_rows_have_test_references` |
| Logging | Audit-Log Manipulation | Incident Response unmöglich | Signierte JSONL-Logs, WORM Storage | `pytest tests/security/test_threat_matrix.py::test_threat_matrix_rows_have_test_references` |

> TODO: Ergänze weitere Zeilen (P2P Replikation, LLM Key Issuance, Storage Buckets, Realtime).

---

## Security Controls
- Sandbox Limits: 2 vCPU / 1 GiB RAM / 120 s / 1 GiB Disk.  
- Clean-Room Implementierung; Inspirationsquellen in `docs/FEATURE_ORIGINS.md` dokumentieren.  
- API Keys: Admin 90d, Namespace 30d, Model Access 1h, Session 1h; Rotation via `/api/v1/auth/keys/rotated` (HMAC).  
- Audit Logs: JSONL + cosign Signaturen, Speicherung in WORM-Bucket.  
- Telemetrie: Sampling-Rate je Umgebung setzen, Exporter whitelisten.  
- Secrets: In Vault/KMS speichern (siehe `docs/governance.md`).  
- OTEL & Metrics: `/healthz` und `/metrics` verpflichtend, Alerts auf Ausfälle konfigurieren.

---

## Security Testing Roadmap
- [x] Python Test-Suite in `tests/security/` anlegen (`pytest`).
- [x] Sektion in CI Workflow (`pytest tests/security/`) aktivieren sobald Tests existieren.
- [ ] Negative Tests: RLS Umgehung, Rate-Limit Bypass, Audit-Log Tampering.  
- [ ] Penetration Test Plan (extern) vorbereiten, wenn Phase-0 abgeschlossen ist.

---

SPDX-License-Identifier: Apache-2.0
