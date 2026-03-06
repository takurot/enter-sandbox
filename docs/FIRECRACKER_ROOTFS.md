# Firecracker Rootfs Image (P2-002)

This document defines the `P2-002` output:

- Alpine Linux rootfs source pinning
- reproducible archive verification and extraction
- `rootfs.ext4` image build contract for Firecracker

## Scope

- Phase 2 image foundation task only (`P2-002`)
- Does not boot or manage VMs yet (`P2-003+`)
- Establishes repeatable rootfs preparation for future pool/snapshot tasks

## Baseline

- Base distribution: **Alpine Linux** (`alpine-minirootfs`)
- Architecture: `x86_64`
- Pinned manifest: `assets/firecracker-rootfs/manifest.json`
- Preparation script: `scripts/prepare_firecracker_rootfs.py`

## Build Workflow

### Any host (download + verify + extract only)

Use this on macOS/Windows/Linux to prepare validated rootfs contents without image build:

```bash
python3 scripts/prepare_firecracker_rootfs.py --no-build-image
python3 scripts/prepare_firecracker_rootfs.py --check-only --no-build-image
```

### Linux host (build rootfs.ext4)

`rootfs.ext4` build requires Linux tooling (`mkfs.ext4` or `mke2fs`).

```bash
python3 scripts/prepare_firecracker_rootfs.py
python3 scripts/prepare_firecracker_rootfs.py --check-only
```

## Output Artifacts

- Download cache: `assets/firecracker-rootfs/downloads/`
- Extracted rootfs directory: `assets/firecracker-rootfs/rootfs/`
- Image metadata: `assets/firecracker-rootfs/output/rootfs-image.json`
- Firecracker rootfs image: `assets/firecracker-rootfs/output/rootfs.ext4`

`rootfs-image.json` records whether image build happened and which builder was used.

## Operational Notes

- `--check-only` must fail when pinned archive/extraction/image outputs are missing.
- Tar extraction rejects unsafe paths (`/` absolute paths and `..` traversal).
- If cached archive or extracted files mismatch hash expectations, the script re-downloads/re-extracts.
- Default image size is pinned in manifest (`image.size_mb`) and must remain positive.

## Exit Criteria

`P2-002` is complete when all items below are satisfied:

1. Alpine rootfs source and integrity checks are pinned in repository.
2. A single script can prepare validated rootfs inputs on any host.
3. Linux hosts can build `rootfs.ext4` using `mkfs.ext4` (or `mke2fs`).
4. `docs/PLAN.md` marks `P2-002` as complete and records implementation notes.
