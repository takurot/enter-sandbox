from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_PATH = REPO_ROOT / ".github" / "workflows" / "testbench.yml"


def _workflow_text() -> str:
    return WORKFLOW_PATH.read_text(encoding="utf-8")


def test_testbench_workflow_has_expected_triggers():
    text = _workflow_text()
    assert "pull_request:" in text
    assert "schedule:" in text
    assert "cron: \"0 18 * * *\"" in text
    assert "workflow_dispatch:" in text
    assert "inputs:" in text
    assert "mode:" in text
    assert "options:" in text
    assert "- quick" in text
    assert "- full" in text
    assert "- perf" in text


def test_testbench_workflow_runs_quick_full_and_perf_modes():
    text = _workflow_text()
    assert "Run quick mode (PR)" in text
    assert "--mode quick --json-output artifacts/tb-quick.json" in text
    assert "Run full mode (nightly)" in text
    assert "--mode full --json-output artifacts/tb-full.json" in text
    assert "Run perf mode (nightly)" in text
    assert "--mode perf --json-output artifacts/tb-perf.json" in text
    assert "Run selected mode (manual)" in text
    assert '--mode "${{ inputs.mode }}"' in text


def test_testbench_workflow_uploads_json_artifacts():
    text = _workflow_text()
    assert "uses: actions/upload-artifact@v4" in text
    assert "tier1-testbench-${{ github.run_id }}" in text
    assert "artifacts/*.json" in text
