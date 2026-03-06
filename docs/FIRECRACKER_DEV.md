# Firecracker Dev Environment (P2-001)

This document defines the `P2-001` output:

- Technical comparison: `Firecracker` vs `libkrun`
- Tier 2 baseline decision
- Practical development environments for macOS/Windows contributors

## Scope

- Phase 2 foundation task only (`P2-001`)
- Does not implement Tier 2 execution yet (`P2-002+`)
- Establishes a repeatable developer workflow and decision record

## Firecracker vs libkrun

| Topic | Firecracker | libkrun |
| --- | --- | --- |
| Isolation model | Dedicated VMM process on KVM | Library-embedded KVM virtualization |
| Ecosystem maturity | Strong adoption in agent sandbox products and cloud workflows | Smaller ecosystem, fewer production references |
| Snapshot model | Well-understood VM snapshot workflow, matches Phase 2 goals | Snapshot support depends on surrounding orchestration design |
| Operational model | Explicit VM lifecycle control, good fit for pool management | Tight embedding can reduce overhead but increases integration coupling |
| Tooling/docs | Extensive docs and operational examples | Leaner docs and operational examples |
| Expected fit for EnterSandBox Phase 2 | High | Medium |

## Decision

Use **Firecracker as the Tier 2 VMM baseline**.

Rationale:

1. It aligns with the product specification and existing roadmap language.
2. It has stronger operational precedent for VM pools + snapshot resume workflows.
3. It is easier to staff and review because ecosystem knowledge is broader.

`libkrun` remains a future optimization candidate after `P2-005` once baseline behavior,
benchmarks, and governance integrations are stable.

## Development Environments

Firecracker requires Linux + KVM for full runtime validation. macOS/Windows development is
therefore split between local development ergonomics and Linux KVM validation.

### Linux host (recommended for VM runtime tests)

Use a native Linux machine (or cloud VM) with `/dev/kvm`.

Validation command:

```bash
test -r /dev/kvm && echo "kvm-ok" || echo "kvm-missing"
```

### macOS host

Use local tooling for development, and run KVM-required checks on a Linux host.

1. Start local contributor VM with `Vagrant`:

```bash
vagrant up
vagrant ssh
```

2. Optional editor-first workflow with `Dev Container`:

```bash
devcontainer up --workspace-folder .
```

3. Run Firecracker runtime validation on a Linux machine with KVM (local network or cloud).

### Windows host

Use local tooling via `Vagrant` or a `Dev Container`, then run KVM checks on Linux.

1. Bring up contributor VM:

```bash
vagrant up
vagrant ssh
```

2. Optional editor-first workflow with `Dev Container`:

```bash
devcontainer up --workspace-folder .
```

3. Execute KVM runtime validation on Linux host that exposes `/dev/kvm`.

## Repository Artifacts

- `Vagrantfile`: contributor VM bootstrap for cross-platform local development
- `.devcontainer/devcontainer.json`: consistent Rust/Python developer container
- `docs/FIRECRACKER_ROOTFS.md`: Alpine rootfs image preparation workflow (`P2-002`)

These artifacts support coding, linting, tests, and build preparation. Full Firecracker VM boot
tests still require Linux KVM infrastructure.

## Exit Criteria

`P2-001` is complete when all items below are satisfied:

1. Firecracker vs libkrun trade-offs are documented with an explicit baseline decision.
2. macOS/Windows contributor paths are documented with concrete commands.
3. Vagrant and Dev Container artifacts are committed and referenced from README.
4. `docs/PLAN.md` marks `P2-001` as complete and records implementation notes.
