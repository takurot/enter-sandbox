# Firecracker Alpine Rootfs Asset

This directory pins the Alpine rootfs input used by `P2-002`.

- Manifest: `assets/firecracker-rootfs/manifest.json`
- Builder script: `scripts/prepare_firecracker_rootfs.py`
- Target artifact: `rootfs.ext4` (built on Linux with `mkfs.ext4`/`mke2fs`)

## Why this exists

`P2-002` requires a reproducible rootfs baseline for Firecracker VM image creation.
The manifest pins:

- exact download URL
- archive SHA-256 (+ size)
- extracted file checksums used as integrity smoke checks
- image contract (`path`, `size_mb`, `label`)

## Usage

Prepare/verify rootfs inputs on any host:

```bash
python3 scripts/prepare_firecracker_rootfs.py --no-build-image
python3 scripts/prepare_firecracker_rootfs.py --check-only --no-build-image
```

Build `rootfs.ext4` on Linux (requires `mkfs.ext4` or `mke2fs`):

```bash
python3 scripts/prepare_firecracker_rootfs.py
python3 scripts/prepare_firecracker_rootfs.py --check-only
```
