# docs/FEATURE_ORIGINS.md

Version: 0.1  
Letzte Änderung: 2025-10-18  
Maintainer: @bkgoder

Zweck
-----
Dieses Dokument erfasst für jede adaptierte Idee aus externen Inspirations‑Repos eine klare Dokumentation (Feature → Inspirations‑Repo → Rationale → Design → Tests → Commit/PR Nachweis → No‑Copy‑Statement). Es ist verpflichtend: jede Implementierung muss hier einen Eintrag haben, PR‑Beschreibungen müssen auf den jeweiligen Eintrag referenzieren.

Format (Template)
-----------------
- Feature Name:  
- Inspirations‑Repo URL (Quelle):  
- Rationale (Warum adaptieren?):  
- High‑Level Design (Wie neu implementiert?):  
- API/Contracts (Endpunktliste oder Manifest mapping):  
- Tests (Unit / Integration / E2E):  
- Risk / License Notes:  
- Commits / PRs:  
- No‑Copy Statement (Verpflichtend): "No source copying — Implementation written from scratch. I used the repo only as an inspiration for <feature>."  
- Reviewer Signoff:

Initiale Einträge (Draft)
------------------------

1) Feature Name: Sandbox Isolation & MCP (Quick + Persistent Modes)  
- Inspirations‑Repo URL: `https://github.com/microsandbox/microsandbox`  
- Rationale: Microsandbox zeigt praktikable Patterns für fast‑startup MicroVMs (Quick Mode) sowie MCP JSON‑RPC patterns für agent control. Diese Patterns sind für BKG zentral (CAVE).  
- High‑Level Design: Implementiere `plugin_cave` mit zwei runtime profiles: WASM/WASI Quick Mode (<200ms startup target) und Persistent Mode (MicroVM snapshots, backtracking). MCP endpoints `/mcp/init`, `/mcp/execute`, `/mcp/list-tools` implementieren JSON‑RPC methods. Implementierung erfolgt neu in Rust, mit eigene MicroVM orchestration (inspired by ideas, not code).  
- API/Contracts: `/api/v1/sandboxes`, `/mcp` (see docs/api.md).  
- Tests: Unit tests for policy enforcement, integration tests for lifecycle (create->start->exec->stop), e2e for ws streaming; fuzzing for manifest parsers.  
- Risk / License Notes: No code reuse; ensure Clean‑Room.  
- Commits / PRs: TBD  
- No‑Copy Statement: No source copying — Implementation written from scratch. Used microsandbox only for architecture patterns and MCP semantics.  
- Reviewer Signoff: TBD

2) Feature Name: Realtime DB + RLS + Storage Patterns  
- Inspirations‑Repo URL: `https://github.com/supabase/supabase`  
- Rationale: Supabase demonstrates a cohesive design for self‑hosted Postgres backed services with Realtime and Storage APIs; BKG requires RLS and Realtime for workflow telemetry and events.  
- High‑Level Design: `bkg_db` uses Postgres with RLS policies; Realtime implemented as separate event broker service that subscribes to DB WAL or streams events from services; Storage is pluggable with S3‑compat backends. All code implemented in Rust and SQL; no direct reuse.  
- API/Contracts: `/realtime` WS channels, DB pub/sub mapping, `/api/v1/storage/*`.  
- Tests: RLS enforcement tests (simulate cross‑namespace reads/writes), realtime subscription E2E.  
- Risk / License Notes: Inspiration only; check Postgres extensions licensing if used.  
- Commits / PRs: TBD  
- No‑Copy Statement: No source copying — Implementation written from scratch.

3) Feature Name: P2P Model Seeding & DHT Patterns  
- Inspirations‑Repo URL: `https://github.com/exo-explore/exo` and `https://github.com/libp2p/rust-libp2p`  
- Rationale: Exo demonstrates ring partitioning for P2P clusters; rust‑libp2p shows robust building blocks (DHT, PubSub). BKG needs secure model seeding & replica tracking.  
- High‑Level Design: `plugin_p2p` implements a pluggable backend with discovery, pubsub and chunk transfer patterns; manifests sign+checksum; operator CA for peer cert auth. Implementation uses our own abstractions and reimplements only the necessary protocols in Rust with compatible semantics.  
- API/Contracts: `/api/v1/peers/*`, replication jobs.  
- Tests: Multi‑node integration, integrity checks (sha256), NAT traversal simulation.  
- No‑Copy Statement: No source copying — Implementation written from scratch.

4) Feature Name: Local LLM Inference Adapter & Streaming API  
- Inspirations‑Repo URL: `https://github.com/withcatai/node-llama-cpp`, `https://github.com/ggml-org/llama.cpp`, `https://github.com/cm64-studio/LLMule-desktop-client`  
- Rationale: Node‑llama‑cpp and llama.cpp illustrate local inference patterns and optimizations; LLMule provides UI & UX patterns for model management and streaming. BKG requires local inference in Admin‑CAVEs with robust streaming and fallback strategies.  
- High‑Level Design: Implement `bkg_llm` adapter trait (see adapter_traits.rs) with FFI/IPC acceleration backends. Streaming over WS/SSE with backpressure support and deterministic seeding option. Model registry with `.bkg` packaging. Pure implementation in Rust; used external repos for interface inspiration only.  
- API/Contracts: `/api/v1/admin/llm/models`, `/api/v1/llm/chat`, `/api/v1/llm/embed`  
- Tests: Token streaming E2E tests, model download + signature verification tests, fallback path tests.  
- No‑Copy Statement: No source copying — Implementation written from scratch.

Weitere Einträge
----------------
Füge bitte für jede weitere adaptierte Idee (e.g. `wasmtime`, `firecracker`, `wasmer`, `faster-whisper`, `coqui TTS`, `diffusers`, `audiocraft`) einen eigenen Eintrag nach dem obigen Template hinzu. Jede Implementierung muss mit Commits/PRs verknüpft werden.

Prozess‑Anweisung (Pflicht)
---------------------------
- Jede PR, die einen Feature‑Origin adaptiert, muss in PR‑Beschreibung die `Feature‑Origin` Sektion referenzieren.  
- CI führt heuristic "no‑copy" check; PRs die matches finden, werden markiert und erfordern manuellen Nachweis in diesem Dokument.

SPDX-License-Identifier: Apache-2.0