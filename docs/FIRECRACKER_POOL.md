# Firecracker VM Pool Strategy (P2-003)

This document defines the `P2-003` output:

- single-host Firecracker VM pool baseline for Phase 2
- concrete warm capacity and scaling thresholds
- lifecycle and API contract for `P2-004` and `P2-005`

## Scope

- Phase 2 design task only (`P2-003`)
- Assumes a single Linux host with KVM and one Firecracker process per VM
- Uses the pinned Alpine image from `docs/FIRECRACKER_ROOTFS.md` (`rootfs.ext4`)
- Does not implement runtime code yet (`P2-004`) or snapshot restore (`P2-005`)

## Goals

1. Keep Tier 2 acquisition latency predictable by maintaining a small warm pool.
2. Bound host resource usage with explicit per-host capacity and reclaim rules.
3. Preserve a lifecycle model that can switch from full boot to snapshot restore later.

## Baseline Assumptions

- `P2-004` starts from `rootfs.ext4` and a regular Firecracker boot path.
- `P2-005` will replace the slow part of `Creating` with snapshot resume, but the pool
  semantics stay the same.
- The first implementation targets short-lived, stateless Heavy-Sandbox jobs. Session
  persistence remains a later task (`P2-023`).
- Default host sizing assumes each warm VM reserves roughly `1 vCPU` and `1 GiB` RAM.

## Pool Lifecycle

| State | Meaning | Entry condition | Exit condition |
| --- | --- | --- | --- |
| `Creating` | Firecracker process is booting and guest bootstrap is not ready yet | pool refill, demand spike, or replacement after failure | successful health check -> `Warm`; boot/health failure -> discard |
| `Warm` | VM is booted, idle, and ready to accept a lease | `Creating` passed health check or `Draining` scrub finished | `acquire` -> `Leased`; idle reap -> discard |
| `Leased` | VM is assigned to one sandbox run | `acquire` selected a ready VM | `release` -> `Draining`; fatal guest failure -> discard |
| `Draining` | VM is being scrubbed, logs harvested, and network state reset before reuse | `release` completed or lease timeout fired | scrub + health check -> `Warm`; scrub failure -> discard |
| `Failed` | terminal bookkeeping state for metrics/alerts only | any unexpected boot, guest, or control-plane failure | removed from active pool accounting |

Operational notes:

1. A VM is not counted as warm until a guest health check succeeds.
2. `release` never returns a VM directly to `Warm`; it must pass through `Draining`.
3. Snapshot support is optional for `P2-004`, but every state transition must keep enough
   metadata for `P2-005` to record whether the VM came from full boot or snapshot restore.

## Scaling Policy

The first implementation should use conservative single-host defaults:

- minimum warm instances: `2`
- maximum warm instances: `8`
- preferred ready target after refill: `2`
- per-host concurrency cap: `8` active VMs (`Warm` + `Creating` + `Leased` + `Draining`)

`scale-out` rules:

1. If available `Warm` instances drop below the minimum warm floor (`2`), create enough VMs to
   return to the preferred ready target, without exceeding the maximum warm instances /
   per-host cap.
2. If pool utilization reaches `70%` or higher (`Leased` divided by active VMs), start
   background creation even if one warm VM is still available.
3. If a lease request arrives while no warm VM exists and the pool is below cap, place a VM
   into `Creating` immediately and let `acquire` wait for it until the request timeout expires.

`scale-in` rules:

1. If available `Warm` instances stay above `4` for `5 minutes`, reap the oldest surplus
   warm VMs until the pool returns to the preferred ready target.
2. `Creating` VMs are never canceled for scale-in; only idle `Warm` VMs are removed.
3. Any VM with repeated guest health check failure should be discarded instead of recycled.

These defaults are intentionally small so `P2-004` can be validated on a single KVM host
before introducing cross-host schedulers or dynamic host classes.

## Health Check Contract

Every transition into `Warm` requires a guest health check:

1. Firecracker API socket responds and the guest kernel is booted.
2. The guest runtime can execute the minimal Python entrypoint required by Tier 2.
3. Scratch workspace mount/network namespace state is clean.
4. The VM can read the pinned rootfs lineage (`rootfs.ext4` image ID or derived snapshot ID).

The health check must be cheap and deterministic because it runs both after `Creating` and
after `Draining`.

## P2-004 Handoff Contract

`P2-004` should implement a pool manager around three core operations:

```text
acquire(request) -> lease | timeout
release(lease, outcome) -> recycled | discarded
reap(now) -> reap summary
```

Required semantics:

- `acquire`
  - Prefer the oldest `Warm` VM to reduce idle skew.
  - May block on one `Creating` VM if the pool is below cap.
  - Returns a lease handle containing `vm_id`, acquisition timestamp, source (`boot` or
    `snapshot`), and guest connection details.
- `release`
  - Marks the lease complete, moves the VM to `Draining`, and records why the run ended
    (`success`, guest error, timeout, infrastructure failure).
  - Must collect enough metadata for later audit logs and benchmark work.
- `reap`
  - Enforces stale `Creating` / `Draining` timeouts.
  - Removes excess idle warm VMs according to the scale-in policy.
  - Emits counters for `created`, `recycled`, `discarded`, and `failed`.

Supporting data that `P2-004` should expose:

- pool counters: `creating`, `warm`, `leased`, `draining`, `failed`
- queue/latency metrics: `acquire_wait_ms`, `warm_hit`, `cold_miss`
- per-VM metadata: `vm_id`, `started_at`, `reuse_count`, `boot_source`, `last_health_check_at`

## Snapshot Handoff for P2-005

`P2-005` should plug into the existing design instead of inventing a new pool model:

1. `Creating` remains the public state, but creation may be fulfilled by snapshot restore.
2. `boot_source` in the lease/metadata tells benchmarks whether the VM came from `rootfs.ext4`
   boot or snapshot resume.
3. Scale thresholds remain the same; only the time spent in `Creating` should change.

## Exit Criteria

`P2-003` is complete when all items below are satisfied:

1. The pool lifecycle and reuse model are documented with concrete state transitions.
2. Warm pool sizing, `scale-out`, and `scale-in` thresholds are fixed for the first host class.
3. `P2-004` has an explicit `acquire` / `release` / `reap` handoff contract.
4. Snapshot integration points for `P2-005` are defined without blocking the first pool
   implementation.
