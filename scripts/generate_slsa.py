#!/usr/bin/env python3
"""Generate a lightweight SLSA provenance statement."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from contextlib import suppress
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
import hashlib
from pathlib import Path


def _git(args: list[str]) -> str:
    try:
        return subprocess.check_output(args, text=True).strip()
    except subprocess.CalledProcessError as exc:  # pragma: no cover - defensive
        raise SystemExit(f"git command failed: {' '.join(args)}: {exc}") from exc


def _default_commit() -> str:
    commit = os.environ.get("GITHUB_SHA")
    if commit:
        return commit
    with suppress(SystemExit):
        return _git(["git", "rev-parse", "HEAD"])
    return "UNKNOWN"


def _default_branch() -> str:
    ref = os.environ.get("GITHUB_REF")
    if ref:
        return ref
    with suppress(SystemExit):
        return _git(["git", "rev-parse", "--abbrev-ref", "HEAD"])
    return "HEAD"


@dataclass
class ProvenanceStatement:
    schema_version: str
    builder: str
    build_type: str
    invocation_id: str
    started_at: str
    finished_at: str
    materials: list


def build_provenance() -> ProvenanceStatement:
    repository = os.environ.get("GITHUB_REPOSITORY")
    repo_uri: str | None = None
    if repository:
        repo_uri = f"https://github.com/{repository}"
    else:
        with suppress(SystemExit):
            repo_uri = _git(["git", "config", "--get", "remote.origin.url"])
    if not repo_uri:
        repo_uri = Path.cwd().as_uri()

    commit = _default_commit()
    branch = _default_branch()
    timestamp = datetime.now(timezone.utc).isoformat()
    commit_digest = hashlib.sha256(commit.encode("utf-8")).hexdigest()

    return ProvenanceStatement(
        schema_version="https://slsa.dev/provenance/v0.2",
        builder="scripts/generate_slsa.py",
        build_type="https://github.com/BinaryKitsuneGuild/bkg-ci",
        invocation_id=f"{repo_uri}@{commit}",
        started_at=timestamp,
        finished_at=timestamp,
        materials=[
            {
                "uri": repo_uri,
                "digest": {"sha256": commit_digest},
                "branch": branch,
            }
        ],
    )


def main() -> int:
    output = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("slsa.json")
    statement = build_provenance()
    output.write_text(json.dumps(asdict(statement), indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"SLSA provenance written to {output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
