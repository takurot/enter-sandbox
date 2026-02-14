# CPython WASI Reproducible Asset (P1-070)

This directory defines a pinned CPython WASI artifact for reproducible local/CI runs.

## What is pinned

- Source repository: `brettcannon/cpython-wasi-build`
- Release tag: `v3.13.12`
- Asset: `python-3.13.12-wasi_sdk-24.zip`
- Archive SHA-256: `a83d0a761e67e0bc3cea4a742145dbbea236429b056115ce2c3174157ac206c9`

The lock data lives in `assets/cpython-wasi/manifest.json`.

## Prepare assets (local)

```bash
python3 scripts/prepare_cpython_wasi_assets.py
```

The script performs:

1. Download to `assets/cpython-wasi/downloads/` when missing.
2. Download with timeout + retry for transient network failures.
3. SHA-256 (and size when present) verification against the manifest.
4. If cached archive verification fails, remove it and re-download once.
5. Extraction to `assets/cpython-wasi/runtime/`.
6. Post-extract verification for pinned files (including `python.wasm` hash).

## Verify only (no download/extract)

```bash
python3 scripts/prepare_cpython_wasi_assets.py --check-only
```

This is the mode used to guarantee deterministic inputs once the asset is prepared.

## CI behavior

GitHub Actions runs the same script with the same manifest so both local and CI use
identical input assets.
