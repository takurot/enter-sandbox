import re
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
PLAN_PATH = REPO_ROOT / "docs" / "PLAN.md"
DOC_PATH = REPO_ROOT / "docs" / "FIRECRACKER_POOL.md"


def test_firecracker_pool_doc_covers_pool_states_and_scaling_policy():
    text = DOC_PATH.read_text(encoding="utf-8")

    assert "Firecracker VM Pool Strategy" in text
    assert "## Scope" in text
    assert "## Pool Lifecycle" in text
    assert "## Scaling Policy" in text
    assert "Creating" in text
    assert "Warm" in text
    assert "Leased" in text
    assert "Draining" in text
    assert "snapshot" in text.lower()


def test_firecracker_pool_doc_sets_concrete_thresholds_for_p2_004():
    text = DOC_PATH.read_text(encoding="utf-8")

    assert "minimum warm instances" in text
    assert "maximum warm instances" in text
    assert "scale-out" in text
    assert "scale-in" in text
    assert "2" in text
    assert "8" in text
    assert "70%" in text
    assert "5 minutes" in text


def test_firecracker_pool_doc_defines_p2_004_handoff_contract():
    text = DOC_PATH.read_text(encoding="utf-8")

    assert "## P2-004 Handoff Contract" in text
    assert "acquire" in text
    assert "release" in text
    assert "reap" in text
    assert "health check" in text.lower()
    assert "rootfs.ext4" in text


def test_plan_marks_p2_003_as_completed():
    text = PLAN_PATH.read_text(encoding="utf-8")

    match = re.search(
        r"\|\s*P2-003\s*\|.*?\|\s*`\[x\]`\s*\|",
        text,
    )
    assert match is not None, "P2-003 should be marked as [x] in docs/PLAN.md"


def test_readmes_reference_firecracker_pool_doc():
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    readme_ja = (REPO_ROOT / "README.ja.md").read_text(encoding="utf-8")

    assert "[Firecracker Pool Strategy (FIRECRACKER_POOL.md)](docs/FIRECRACKER_POOL.md)" in readme
    assert "[Firecracker VMプール設計 (FIRECRACKER_POOL.md)](docs/FIRECRACKER_POOL.md)" in readme_ja
