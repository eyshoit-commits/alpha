# docs/compatibility.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

---

## Ziel
Kompatibilitätsmatrix für Runtime-Versionen, Datenbanken, Betriebssysteme und API-Level.

---

## Unterstützte Plattformen (Phase-0)
- **Host OS**: Linux (Ubuntu 22.04 LTS, Debian Bullseye).  
- **Container Runtime**: containerd / nerdctl (cgroups v2).  
- **Rust Toolchain**: stable (aktuell 1.74+).  
- **Node.js**: 18 LTS (Admin-UI).  
- **Postgres**: 15.x (RLS aktiviert, wal_level = logical).  
- **Python**: 3.11 (Security Tests).

---

## Sandbox Runtimes
| Runtime | Version | Status |
|---------|---------|--------|
| PythonSandbox | CPython 3.11 | Phase-0 Ziel |
| NodeSandbox | Node.js 20 | Phase-0 Ziel |
| WASM Quick Mode | WASI Preview2 | Prototyp |
| MicroVM Persistent | Firecracker 1.5 | Backlog |

---

## API Versionierung
- HTTP APIs versioniert unter `/api/v1/...`.  
- Breaking Changes: RFC + Update von `docs/api.md` und Release Notes.  
- pgwire Kompatibilität: Simple Query Path zu PostgreSQL 15.  
- gRPC: IDL-Versionierung via Semver (proto-Dateien, Backlog).

---

## Abhängigkeiten & Tooling
- sqlx 0.7 (mit SQLite + Postgres Features).  
- axum 0.7, tokio 1.x, tracing 0.1.  
- syft, cosign, slsa-generator (Supply Chain).  
- ajv-cli, openapi-cli (Schema Checks).

---

## Backlog
- [ ] Windows/macOS Dev-Support evaluieren (Sandbox Feature Flags).  
- [ ] GPU Sandbox Kompatibilität (CUDA, ROCm).  
- [ ] Multi-Cluster P2P Testing (libp2p) dokumentieren.  
- [ ] Version-Matrix automatisiert generieren (CI Artefakt).

---

SPDX-License-Identifier: Apache-2.0
