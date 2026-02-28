---
name: start-task
description: Start and execute implementation tasks in the EnterSandBox repository using docs/PROMPT.md, docs/PLAN.md, and docs/SPEC.md as authoritative sources.
---

# Start Task Skill

This skill defines the standard operating procedure for starting and completing a development task
in the EnterSandBox repository.

## When to use this skill

- User asks to start a task, continue development, or pick the next task.
- You need to select the next valid task from `docs/PLAN.md`.

## Workflow

Follow this order strictly:
1. Explore and plan
2. Implement with TDD
3. Verify locally
4. Update docs
5. Commit, push, PR, and monitor CI

### 1. Explore & Plan

1. Read `docs/PROMPT.md` first (authoritative process and constraints).
2. Read `docs/PLAN.md` and `docs/SPEC.md` to understand phase, dependency, and scope.
3. Optionally read `README.md` for API and operational context.
4. Select the smallest incomplete task whose dependencies are satisfied.
   - Prefer explicit user-requested task over auto-selection.
   - Task IDs in this repo are `P1-xxx`, `P2-xxx`, `CI-xxx`, `CD-xxx`, etc.
5. If scope is ambiguous, ask one concise clarification question. Otherwise proceed.
6. Create a concrete implementation plan before editing.
   - Affected files/modules
   - Acceptance criteria
   - Test plan (Red -> Green -> Refactor)

### 2. Implement & Verify

1. Sync latest `main` and create a feature branch:
   - `feature/<task-id>-<short-description>`
2. Apply TDD:
   - Red: add/extend failing test(s)
   - Green: implement minimal change
   - Refactor: clean up without behavior change
3. Keep diffs scoped to one task.
4. Do not include unrelated local artifacts in commits (for example `skills/`, ad-hoc notes).

### 3. Commit, PR & Document Update

#### Local quality gates (run before commit)

Use project-aligned commands:

```bash
.venv/bin/ruff check python/ scripts/ tests/
.venv/bin/ruff format python/ scripts/ tests/ --check
.venv/bin/pytest -q tests/python
cargo test --manifest-path agentbox-core/Cargo.toml
cargo clippy --manifest-path agentbox-core/Cargo.toml -- -D warnings
cargo fmt --manifest-path agentbox-core/Cargo.toml --all -- --check
cargo build --manifest-path runner-wasm/Cargo.toml --target wasm32-wasip1 --release
```

For performance/regression tasks, also run:

```bash
python3 scripts/check_tier1_benchmarks.py
cargo bench --manifest-path agentbox-core/Cargo.toml --bench cold_start -- --noplot
```

#### Documentation updates

1. Mark the task as complete (`[x]`) in `docs/PLAN.md`.
2. Add an implementation memo line in the PLAN memo section with date and evidence.
3. Update `README.md`, `README.ja.md`, or `docs/SPEC.md` if user-facing behavior changed.

#### Commit, push, PR

1. Use Conventional Commits:
   - `<type>(<scope>): <summary>`
2. Commit only relevant files.
3. Push branch:
   - `git push origin feature/<task-id>-<short-description>`
4. Create PR with clear summary, exit criteria, and validation commands/results.
   - Base is usually `main` unless stacked PR is explicitly required.
5. Monitor CI and auto-fix failures until all required checks are green.
   - `gh pr checks <pr-number>`
   - `gh run watch <run-id> --exit-status`

## Definition of done

- Selected task is fully implemented with tests.
- All required local checks pass.
- `docs/PLAN.md` status and memo are updated.
- PR is created and CI is green.
