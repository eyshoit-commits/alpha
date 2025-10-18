"""Pytest fixtures for the security test suite."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List

import pytest


@dataclass(frozen=True)
class ThreatRow:
    """Represents a single row in the threat matrix."""

    category: str
    threat: str
    impact: str
    mitigations: str
    tests: str


def _repo_root() -> Path:
    current = Path(__file__).resolve()
    for candidate in current.parents:
        if (candidate / "README.md").exists():
            return candidate
    raise RuntimeError("Unable to locate repository root containing README.md")


def _parse_threat_matrix(lines: Iterable[str]) -> List[ThreatRow]:
    rows: List[ThreatRow] = []
    in_matrix = False
    for raw_line in lines:
        line = raw_line.strip()
        if not line:
            if in_matrix:
                break
            continue
        if line.startswith("| Kategorie"):
            in_matrix = True
            continue
        if not in_matrix:
            continue
        if set(line) <= {"|", "-", " "}:
            # Separator row between header and body
            continue
        parts = [part.strip() for part in line.split("|") if part.strip()]
        if len(parts) != 5:
            raise ValueError(f"Unexpected threat matrix row format: {line}")
        rows.append(ThreatRow(*parts))
    if not rows:
        raise ValueError("Threat matrix table not found in docs/security.md")
    return rows


@pytest.fixture(scope="session")
def repo_root() -> Path:
    """Return the repository root based on the presence of README.md."""

    return _repo_root()


@pytest.fixture(scope="session")
def security_doc(repo_root: Path) -> str:
    """Return the contents of docs/security.md."""

    doc_path = repo_root / "docs" / "security.md"
    if not doc_path.exists():
        raise FileNotFoundError("docs/security.md is required for security checks")
    return doc_path.read_text(encoding="utf-8")


@pytest.fixture(scope="session")
def threat_rows(security_doc: str) -> List[ThreatRow]:
    """Parse and return the threat matrix rows from docs/security.md."""

    return _parse_threat_matrix(security_doc.splitlines())


@pytest.fixture(scope="session")
def threat_index(threat_rows: List[ThreatRow]) -> Dict[str, ThreatRow]:
    """Index threat rows by their category for quick lookup in tests."""

    return {row.category: row for row in threat_rows}
