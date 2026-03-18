import re
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
PLAN_PATH = REPO_ROOT / "docs" / "PLAN.md"
DOC_PATH = REPO_ROOT / "docs" / "FIRECRACKER_SNAPSHOT.md"


def test_firecracker_snapshot_doc_covers_restore_policy_and_fallback():
    text = DOC_PATH.read_text(encoding="utf-8")

    assert "Firecracker Snapshot Startup" in text
    assert "## Scope" in text
    assert "## Snapshot Bundle Contract" in text
    assert "## Restore Policy" in text
    assert "rootfs.ext4" in text
    assert "boot_source" in text
    assert "snapshot_id" in text
    assert "fallback" in text.lower()
    assert "## Exit Criteria" in text


def test_plan_marks_p2_005_as_completed():
    text = PLAN_PATH.read_text(encoding="utf-8")

    match = re.search(
        r"\|\s*P2-005\s*\|.*?\|\s*`\[x\]`\s*\|",
        text,
    )
    assert match is not None, "P2-005 should be marked as [x] in docs/PLAN.md"


def test_readmes_reference_firecracker_snapshot_doc():
    readme = (REPO_ROOT / "README.md").read_text(encoding="utf-8")
    readme_ja = (REPO_ROOT / "README.ja.md").read_text(encoding="utf-8")

    assert (
        "[Firecracker Snapshot Startup (FIRECRACKER_SNAPSHOT.md)]"
        "(docs/FIRECRACKER_SNAPSHOT.md)" in readme
    )
    assert (
        "[Firecracker スナップショット起動 (FIRECRACKER_SNAPSHOT.md)]"
        "(docs/FIRECRACKER_SNAPSHOT.md)" in readme_ja
    )
