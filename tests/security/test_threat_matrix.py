"""Assertions around the Threat Matrix documented in docs/security.md."""

from __future__ import annotations

from typing import List, TYPE_CHECKING

import pytest

if TYPE_CHECKING:  # pragma: no cover - typing only
    from .conftest import ThreatRow


REQUIRED_CATEGORIES = {
    "Sandbox Isolation",
    "API Auth",
    "Supply Chain",
    "Telemetrie",
    "Logging",
}


def _assert_contains_keywords(text: str, keywords: List[str], *, require_all: bool = True) -> bool:
    """Return True if keywords are present in text (case insensitive)."""

    lowered = text.lower()
    checks = [keyword.lower() in lowered for keyword in keywords]
    return all(checks) if require_all else any(checks)


def test_threat_matrix_includes_required_categories(threat_index):
    """Ensure that core categories from the security roadmap remain documented."""

    missing = sorted(REQUIRED_CATEGORIES.difference(threat_index))
    assert not missing, f"Threat Matrix missing categories: {', '.join(missing)}"


def test_threat_matrix_rows_have_test_references(threat_rows):
    """Every threat should point to an executable validation or TODO placeholder."""

    missing_reference = [
        row.category
        for row in threat_rows
        if not _assert_contains_keywords(
            row.tests,
            ["pytest", "make", "tests/security"],
            require_all=False,
        )
    ]
    assert not missing_reference, (
        "Threat Matrix entries must reference pytest or make targets: "
        + ", ".join(missing_reference)
    )


@pytest.mark.parametrize(
    "category, expected_keywords",
    [
        ("Supply Chain", ["sbom", "slsa", "cosign"]),
        ("API Auth", ["rls", "rotation"]),
        ("Sandbox Isolation", ["seccomp", "cgroup"]),
    ],
)
def test_category_mitigations_include_required_terms(threat_index, category, expected_keywords):
    """Mitigation guidance should capture the controls we rely on in docs/security.md."""

    row = threat_index.get(category)
    assert row is not None, f"Threat Matrix missing category {category}"
    assert _assert_contains_keywords(row.mitigations, expected_keywords), (
        f"Mitigations for {category} should mention {', '.join(expected_keywords)}"
    )
