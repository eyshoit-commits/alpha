#!/usr/bin/env python3
"""Generate an SBOM using the syft CLI and write it to disk."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path


def run_syft() -> str:
    command = os.environ.get("SYFT_CMD", "syft")
    target = os.environ.get("SYFT_TARGET", "dir:.")
    args = [command, target, "-o", "json"]
    try:
        completed = subprocess.run(
            args,
            check=True,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError as exc:  # pragma: no cover - defensive branch
        raise SystemExit(
            "syft CLI not found. Install syft or set SYFT_CMD to an alternative binary."
        ) from exc
    except subprocess.CalledProcessError as exc:
        message = exc.stderr.strip() or exc.stdout.strip() or str(exc)
        raise SystemExit(f"syft command failed: {message}") from exc
    return completed.stdout


def main() -> int:
    output = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("sbom.json")
    sbom_content = run_syft()
    # Validate the content to ensure we write JSON to disk even if syft prints warnings.
    json.loads(sbom_content)
    output.write_text(sbom_content + "\n", encoding="utf-8")
    print(f"SBOM written to {output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
