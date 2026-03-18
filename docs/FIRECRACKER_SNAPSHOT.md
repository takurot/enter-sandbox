# Firecracker Snapshot Startup (P2-005)

This document defines the `P2-005` output:

- snapshot-aware startup contract layered onto the existing `P2-004` VM pool
- restore-or-boot fallback semantics for Firecracker snapshot resume
- metadata flow (`boot_source`, optional `snapshot_id`) for later benchmarks and Tier 2 runtime work

## Scope

- Phase 2 startup task only (`P2-005`)
- Reuses the `P2-004` pool lifecycle (`Creating` -> `Warm` -> `Leased` -> `Draining`)
- Builds on the pinned Alpine rootfs lineage from `docs/FIRECRACKER_ROOTFS.md` (`rootfs.ext4`)
- Covers the repository-side control-plane contract today; full Linux/KVM Firecracker execution remains a later runtime integration task

## Snapshot Bundle Contract

Each snapshot restore attempt is keyed by a bundle with the following minimum metadata:

- `snapshot_id`: immutable identifier for the snapshot artifact
- `lineage_id`: rootfs or derived lineage the snapshot belongs to
- `memory_path`: guest memory snapshot file
- `state_path`: Firecracker VM state file

Operational rules:

1. Snapshot bundles are selected by `lineage_id`, starting from the preferred `rootfs.ext4` lineage.
2. A restore-capable provider may expose only the latest healthy snapshot for a lineage.
3. Pool metadata must preserve both `boot_source` and optional `snapshot_id` so later benchmarks can distinguish cold boot from snapshot resume.

## Restore Policy

`create_vm` should follow this sequence:

1. Query the snapshot catalog for the latest healthy snapshot of the preferred lineage.
2. If no snapshot exists, perform a cold boot from `rootfs.ext4`.
3. If a snapshot exists, attempt snapshot restore before trying a cold boot.
4. On successful restore, the created VM is tagged with `boot_source=Snapshot`, `lineage_id` from the snapshot bundle, and `snapshot_id` from the same bundle.
5. On restore failure, record the failure, quarantine that `snapshot_id` for the current process, and fallback to a cold boot immediately.
6. A cold-boot fallback always reports `boot_source=Boot` and `snapshot_id=None`.

The quarantine rule prevents a corrupted snapshot from being retried on every refill/acquire loop until a newer snapshot appears.

## Pool Integration

Snapshot startup does not change the public pool model introduced in `P2-004`:

- `Creating` still represents either cold boot or snapshot restore in progress.
- Every restored VM must still pass the existing guest health check before entering `Warm`.
- `release` / `reap` / `scale-in` semantics remain unchanged.
- `VmMetadata` and `VmLease` now carry optional `snapshot_id` in addition to `boot_source` and `lineage_id`.

This keeps `P2-020` and `P2-042` free to focus on real Firecracker runtime execution and latency measurement without redefining pool behavior.

## Repository Implementation

The repository implementation for `P2-005` is:

- `agentbox-core/src/snapshot.rs`: `SnapshotAwareProvider` and `SnapshotControlPlane`
- `agentbox-core/src/vm_pool.rs`: propagation of optional `snapshot_id` through `CreatedVm`, `VmMetadata`, and `VmLease`
- Rust tests that verify restore preference, restore failure fallback, failed-snapshot quarantine, and pool metadata propagation

## Exit Criteria

`P2-005` is complete when all items below are satisfied:

1. Snapshot-aware startup prefers restore when a healthy snapshot exists for the preferred lineage.
2. Restore failures are recorded and immediately fallback to cold boot without breaking the pool.
3. Failed `snapshot_id` values are quarantined in-process to avoid repeated restore attempts.
4. Pool snapshots and lease handles preserve `boot_source` plus optional `snapshot_id`.
5. `docs/PLAN.md` marks `P2-005` as complete and README files link to this document.
