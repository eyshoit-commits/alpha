# docs/ci.md

Version: 0.1
Letzte Änderung: 2025-10-18
Maintainer: @bkgoder

---

## Ziel

Dieses Dokument beschreibt die Gatekeeping-Logik der BKG-CI. Alle Pull Requests
gegen `main` müssen die definierten Schutzmechanismen bestehen, bevor sie
gemergt werden dürfen.

---

## Branch Protection

| Regel | Wert |
|-------|------|
| Geschützte Branches | `main` |
| Mergen | Nur per Pull Request, mindestens 1 Review erforderlich |
| Status Checks | Alle unten gelisteten Pflicht-Jobs müssen grün sein |
| Aktualität | PR muss auf dem aktuellen Stand mit `main` sein (keine veralteten Commits) |
| History | Rebase/ Squash erlaubt, Merge-Commits auf `main` deaktiviert |
| Force-Push | Deaktiviert |

---

## Pflicht-Jobs

| Job | Beschreibung | Gate |
|-----|--------------|------|
| `lint-test` | `cargo fmt`, `cargo clippy`, `cargo test` über das gesamte Workspace | Fail bei Format-, Lint- oder Testfehlern |
| `api-schema` | `make api-schema`, `openapi-cli validate`, optionale `cave.yaml` Validierung | Fail, wenn `openapi.yaml` nicht aktuell ist oder die Validierung scheitert |
| `security-tests` | `pytest tests/security` prüft die Threat-Matrix in `docs/security.md` | Fail bei fehlenden Fixtures/Verletzungen der Matrix |
| `web-ui` | Node-Workspaces linten, bauen und Playwright E2E | Fail bei Build- oder E2E-Fehlern |
| `supply-chain` | `make sbom`, `make slsa`, optional `cosign sign-blob` | Fail, wenn Artefakte nicht erzeugt werden können |

Zusätzlich müssen Artefakte `sbom.json`, `slsa.json` (und optional `sbom.sig`)
als Build-Artefakte hochgeladen werden. Die `supply-chain`-Stage schlägt fehl,
wenn `make sbom` oder `make slsa` nicht erfolgreich laufen.

---

## Check-Registrierung

Die obigen Jobs sind als **Required Status Checks** im GitHub Branch-Protection
Dialog zu hinterlegen. Änderungen an der Pipeline erfordern ein Update dieser
Liste sowie eine Anpassung dieses Dokuments.

---

## Secrets & Schlüssel

- `COSIGN_KEY_B64` (optional): Wird genutzt, um die mit `make sbom`
  generierte SBOM zu signieren. Ohne Secret wird der Schritt übersprungen.
- Weitere Secrets (z. B. DB Passwörter, API Keys) dürfen nicht in CI Jobs
  geschrieben werden; stattdessen GitHub Actions Secrets / OIDC nutzen.

---

## Zusammenarbeit

Änderungen an den Checks müssen in `docs/Progress.md` dokumentiert und im Team
abgestimmt werden. Tests, die neu hinzukommen, sind verpflichtend in die
Branch-Protection aufzunehmen.

---

SPDX-License-Identifier: Apache-2.0
