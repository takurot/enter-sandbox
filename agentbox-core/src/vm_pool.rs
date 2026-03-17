use anyhow::{anyhow, bail, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuestConnection {
    pub api_socket_path: PathBuf,
    pub workload_socket_path: Option<PathBuf>,
    pub vsock_port: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootSource {
    Boot,
    Snapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmState {
    Creating,
    Warm,
    Leased,
    Draining,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseOutcome {
    Success,
    GuestError,
    Timeout,
    InfrastructureFailure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatedVm {
    pub vm_id: String,
    pub boot_source: BootSource,
    pub guest_connection: GuestConnection,
    pub lineage_id: String,
    pub snapshot_id: Option<String>,
    pub ready_after: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrainPlan {
    pub ready_after: Duration,
}

pub trait VmProvider {
    fn create_vm(&mut self) -> Result<CreatedVm>;
    fn health_check(&mut self, vm: &VmMetadata) -> Result<()>;
    fn scrub_vm(&mut self, vm: &VmMetadata, outcome: ReleaseOutcome) -> Result<DrainPlan>;
    fn destroy_vm(&mut self, vm: &VmMetadata) -> Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquireRequest {
    pub timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmLease {
    pub vm_id: String,
    pub acquired_at: Instant,
    pub boot_source: BootSource,
    pub guest_connection: GuestConnection,
    pub lineage_id: String,
    pub snapshot_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AcquireOutcome {
    Acquired(VmLease),
    TimedOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseDisposition {
    Recycled,
    Discarded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmMetadata {
    pub vm_id: String,
    pub state: VmState,
    pub started_at: Instant,
    pub state_changed_at: Instant,
    pub ready_at: Instant,
    pub reuse_count: u32,
    pub boot_source: BootSource,
    pub last_health_check_at: Option<Instant>,
    pub lineage_id: String,
    pub snapshot_id: Option<String>,
    pub guest_connection: GuestConnection,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PoolCounters {
    pub creating: usize,
    pub warm: usize,
    pub leased: usize,
    pub draining: usize,
    pub failed: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PoolMetrics {
    pub created: u64,
    pub recycled: u64,
    pub discarded: u64,
    pub failed: u64,
    pub warm_hit: u64,
    pub cold_miss: u64,
    pub acquire_wait_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PoolSnapshot {
    pub counters: PoolCounters,
    pub metrics: PoolMetrics,
    pub vms: Vec<VmMetadata>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefillSummary {
    pub started: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReapSummary {
    pub recycled: u64,
    pub discarded: u64,
    pub failed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmPoolConfig {
    pub minimum_warm_instances: usize,
    pub maximum_warm_instances: usize,
    pub preferred_ready_target: usize,
    pub per_host_concurrency_cap: usize,
    pub scale_out_utilization_percent: u8,
    pub scale_in_warm_threshold: usize,
    pub scale_in_after: Duration,
    pub create_timeout: Duration,
    pub drain_timeout: Duration,
}

impl Default for VmPoolConfig {
    fn default() -> Self {
        Self {
            minimum_warm_instances: 2,
            maximum_warm_instances: 8,
            preferred_ready_target: 2,
            per_host_concurrency_cap: 8,
            scale_out_utilization_percent: 70,
            scale_in_warm_threshold: 4,
            scale_in_after: Duration::from_secs(5 * 60),
            create_timeout: Duration::from_secs(30),
            drain_timeout: Duration::from_secs(30),
        }
    }
}

pub struct FirecrackerVmPool<P: VmProvider> {
    provider: P,
    config: VmPoolConfig,
    metrics: PoolMetrics,
    vms: BTreeMap<String, VmMetadata>,
    last_observed_at: Option<Instant>,
    warm_surplus_since: Option<Instant>,
}

impl<P: VmProvider> FirecrackerVmPool<P> {
    pub fn new(provider: P, config: VmPoolConfig) -> Self {
        Self {
            provider,
            config,
            metrics: PoolMetrics::default(),
            vms: BTreeMap::new(),
            last_observed_at: None,
            warm_surplus_since: None,
        }
    }

    pub fn snapshot(&self) -> PoolSnapshot {
        let mut vms = self.vms.values().cloned().collect::<Vec<_>>();
        vms.sort_by(|left, right| {
            left.started_at
                .cmp(&right.started_at)
                .then_with(|| left.vm_id.cmp(&right.vm_id))
        });

        let counters = self.counters();
        PoolSnapshot {
            counters,
            metrics: self.metrics.clone(),
            vms,
        }
    }

    pub fn refill(&mut self, now: Instant) -> Result<RefillSummary> {
        self.advance_to(now)?;
        let started = self.start_background_creations(now)?;
        self.advance_to(now)?;
        Ok(RefillSummary { started })
    }

    pub fn acquire(&mut self, request: AcquireRequest, now: Instant) -> Result<AcquireOutcome> {
        self.advance_to(now)?;
        if let Some(vm_id) = self.oldest_warm_vm_id() {
            self.metrics.warm_hit = self.metrics.warm_hit.saturating_add(1);
            let lease = self.lease_vm(&vm_id, now)?;
            self.start_background_creations(now)?;
            self.advance_to(now)?;
            return Ok(AcquireOutcome::Acquired(lease));
        }

        self.metrics.cold_miss = self.metrics.cold_miss.saturating_add(1);
        let request_start = now;
        let deadline = add_duration(now, request.timeout)?;
        let mut cursor = now;

        if self.creating_count() == 0 && self.active_count() < self.config.per_host_concurrency_cap
        {
            self.start_creation(cursor)?;
            self.advance_to(cursor)?;
        }

        loop {
            if let Some(vm_id) = self.oldest_warm_vm_id() {
                let waited_ms = duration_to_millis(cursor.saturating_duration_since(request_start));
                self.metrics.acquire_wait_ms =
                    self.metrics.acquire_wait_ms.saturating_add(waited_ms);
                let lease = self.lease_vm(&vm_id, cursor)?;
                self.start_background_creations(cursor)?;
                self.advance_to(cursor)?;
                return Ok(AcquireOutcome::Acquired(lease));
            }

            if let Some(next_ready_at) = self
                .next_transition_ready_at()
                .filter(|candidate| *candidate <= deadline)
            {
                cursor = next_ready_at;
                self.advance_to(cursor)?;

                if self.oldest_warm_vm_id().is_none()
                    && self.creating_count() == 0
                    && self.active_count() < self.config.per_host_concurrency_cap
                    && cursor < deadline
                {
                    self.start_creation(cursor)?;
                    self.advance_to(cursor)?;
                }
                continue;
            }

            let waited_ms = duration_to_millis(deadline.saturating_duration_since(request_start));
            self.metrics.acquire_wait_ms = self.metrics.acquire_wait_ms.saturating_add(waited_ms);
            return Ok(AcquireOutcome::TimedOut);
        }
    }

    pub fn release(
        &mut self,
        lease: VmLease,
        outcome: ReleaseOutcome,
        now: Instant,
    ) -> Result<ReleaseDisposition> {
        self.advance_to(now)?;

        let leased_metadata = self
            .vms
            .get(&lease.vm_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown lease vm_id={}", lease.vm_id))?;
        if leased_metadata.state != VmState::Leased {
            bail!("vm_id={} is not currently leased", lease.vm_id);
        }

        let drain_plan = match self.provider.scrub_vm(&leased_metadata, outcome) {
            Ok(plan) => plan,
            Err(_) => {
                self.discard_vm(&lease.vm_id, true)?;
                self.start_background_creations(now)?;
                self.update_warm_surplus_marker(now);
                return Ok(ReleaseDisposition::Discarded);
            }
        };

        let ready_at = add_duration(now, drain_plan.ready_after)?;
        let record = self
            .vms
            .get_mut(&lease.vm_id)
            .ok_or_else(|| anyhow!("unknown lease vm_id={}", lease.vm_id))?;
        record.state = VmState::Draining;
        record.state_changed_at = now;
        record.ready_at = ready_at;

        self.advance_to(now)?;
        self.start_background_creations(now)?;
        self.advance_to(now)?;

        if self.vms.contains_key(&lease.vm_id) {
            Ok(ReleaseDisposition::Recycled)
        } else {
            Ok(ReleaseDisposition::Discarded)
        }
    }

    pub fn reap(&mut self, now: Instant) -> Result<ReapSummary> {
        let before = self.metrics.clone();
        self.advance_to(now)?;
        self.scale_in_if_needed(now)?;
        self.start_background_creations(now)?;
        self.advance_to(now)?;

        Ok(ReapSummary {
            recycled: self.metrics.recycled.saturating_sub(before.recycled),
            discarded: self.metrics.discarded.saturating_sub(before.discarded),
            failed: self.metrics.failed.saturating_sub(before.failed),
        })
    }

    fn advance_to(&mut self, now: Instant) -> Result<()> {
        self.observe_time(now)?;
        self.discard_stale_transitions(now)?;
        self.promote_ready_vms(now)?;
        self.update_warm_surplus_marker(now);
        Ok(())
    }

    fn observe_time(&mut self, now: Instant) -> Result<()> {
        if let Some(last_observed_at) = self.last_observed_at {
            if now < last_observed_at {
                bail!(
                    "vm pool time cannot move backwards: now={now:?} last_observed_at={last_observed_at:?}"
                );
            }
        }
        self.last_observed_at = Some(now);
        Ok(())
    }

    fn start_background_creations(&mut self, now: Instant) -> Result<usize> {
        let target = self.background_creation_target();
        let mut started = 0;
        for _ in 0..target {
            if !self.start_creation(now)? {
                break;
            }
            started += 1;
        }
        self.advance_to(now)?;
        Ok(started)
    }

    fn start_creation(&mut self, now: Instant) -> Result<bool> {
        let counts = self.counters();
        if self.active_count() >= self.config.per_host_concurrency_cap {
            return Ok(false);
        }
        if counts.warm + counts.creating >= self.config.maximum_warm_instances {
            return Ok(false);
        }

        let created_vm = self.provider.create_vm()?;
        let ready_at = add_duration(now, created_vm.ready_after)?;
        let metadata = VmMetadata {
            vm_id: created_vm.vm_id.clone(),
            state: VmState::Creating,
            started_at: now,
            state_changed_at: now,
            ready_at,
            reuse_count: 0,
            boot_source: created_vm.boot_source,
            last_health_check_at: None,
            lineage_id: created_vm.lineage_id,
            snapshot_id: created_vm.snapshot_id,
            guest_connection: created_vm.guest_connection,
        };
        self.metrics.created = self.metrics.created.saturating_add(1);
        self.vms.insert(metadata.vm_id.clone(), metadata);
        Ok(true)
    }

    fn promote_ready_vms(&mut self, now: Instant) -> Result<()> {
        let ready_vm_ids = self
            .vms
            .iter()
            .filter(|(_, metadata)| {
                matches!(metadata.state, VmState::Creating | VmState::Draining)
                    && metadata.ready_at <= now
            })
            .map(|(vm_id, _)| vm_id.clone())
            .collect::<Vec<_>>();

        for vm_id in ready_vm_ids {
            let Some(current_metadata) = self.vms.get(&vm_id).cloned() else {
                continue;
            };
            let was_draining = current_metadata.state == VmState::Draining;
            let mut warmed_metadata = current_metadata.clone();
            warmed_metadata.state = VmState::Warm;
            warmed_metadata.state_changed_at = now;
            warmed_metadata.ready_at = now;
            warmed_metadata.last_health_check_at = Some(now);
            if was_draining {
                warmed_metadata.reuse_count = warmed_metadata.reuse_count.saturating_add(1);
            }

            if self.provider.health_check(&warmed_metadata).is_err() {
                self.discard_vm(&vm_id, true)?;
                continue;
            }

            self.vms.insert(vm_id.clone(), warmed_metadata);
            if was_draining {
                self.metrics.recycled = self.metrics.recycled.saturating_add(1);
            }
        }

        Ok(())
    }

    fn discard_stale_transitions(&mut self, now: Instant) -> Result<()> {
        let stale_vm_ids = self
            .vms
            .iter()
            .filter_map(|(vm_id, metadata)| {
                let timeout = match metadata.state {
                    VmState::Creating => self.config.create_timeout,
                    VmState::Draining => self.config.drain_timeout,
                    VmState::Warm | VmState::Leased => return None,
                };
                let timeout_deadline = metadata.state_changed_at.checked_add(timeout)?;
                if now >= timeout_deadline && metadata.ready_at > timeout_deadline {
                    Some(vm_id.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for vm_id in stale_vm_ids {
            self.discard_vm(&vm_id, true)?;
        }
        Ok(())
    }

    fn scale_in_if_needed(&mut self, now: Instant) -> Result<()> {
        let Some(surplus_since) = self.warm_surplus_since else {
            return Ok(());
        };
        if self.warm_count() <= self.config.scale_in_warm_threshold {
            self.warm_surplus_since = None;
            return Ok(());
        }
        if now.saturating_duration_since(surplus_since) < self.config.scale_in_after {
            return Ok(());
        }

        while self.warm_count() > self.config.preferred_ready_target {
            let Some(vm_id) = self.oldest_warm_vm_id() else {
                break;
            };
            self.discard_vm(&vm_id, false)?;
        }
        self.update_warm_surplus_marker(now);
        Ok(())
    }

    fn background_creation_target(&self) -> usize {
        let counts = self.counters();
        let warm_or_creating = counts.warm + counts.creating;
        let active = counts.creating + counts.warm + counts.leased + counts.draining;

        let floor_deficit = if counts.warm < self.config.minimum_warm_instances {
            self.config
                .preferred_ready_target
                .saturating_sub(warm_or_creating)
        } else {
            0
        };

        let should_proactively_scale = active > 0
            && counts.warm > 0
            && counts.leased.saturating_mul(100)
                >= active.saturating_mul(self.config.scale_out_utilization_percent as usize);
        let proactive_deficit = if should_proactively_scale {
            1usize.saturating_sub(counts.creating)
        } else {
            0
        };

        let available_cap = self
            .config
            .per_host_concurrency_cap
            .saturating_sub(self.active_count());
        let available_warm_slots = self
            .config
            .maximum_warm_instances
            .saturating_sub(warm_or_creating);

        floor_deficit
            .max(proactive_deficit)
            .min(available_cap)
            .min(available_warm_slots)
    }

    fn lease_vm(&mut self, vm_id: &str, acquired_at: Instant) -> Result<VmLease> {
        let lease = {
            let metadata = self
                .vms
                .get_mut(vm_id)
                .ok_or_else(|| anyhow!("unknown vm_id={vm_id}"))?;
            if metadata.state != VmState::Warm {
                bail!("vm_id={vm_id} is not warm");
            }

            metadata.state = VmState::Leased;
            metadata.state_changed_at = acquired_at;
            metadata.ready_at = acquired_at;

            VmLease {
                vm_id: metadata.vm_id.clone(),
                acquired_at,
                boot_source: metadata.boot_source,
                guest_connection: metadata.guest_connection.clone(),
                lineage_id: metadata.lineage_id.clone(),
                snapshot_id: metadata.snapshot_id.clone(),
            }
        };

        self.update_warm_surplus_marker(acquired_at);

        Ok(lease)
    }

    fn discard_vm(&mut self, vm_id: &str, failed: bool) -> Result<()> {
        let metadata = self
            .vms
            .remove(vm_id)
            .ok_or_else(|| anyhow!("unknown vm_id={vm_id}"))?;
        self.provider.destroy_vm(&metadata)?;
        self.metrics.discarded = self.metrics.discarded.saturating_add(1);
        if failed {
            self.metrics.failed = self.metrics.failed.saturating_add(1);
        }
        Ok(())
    }

    fn oldest_warm_vm_id(&self) -> Option<String> {
        self.vms
            .iter()
            .filter(|(_, metadata)| metadata.state == VmState::Warm)
            .min_by(|(left_id, left), (right_id, right)| {
                left.state_changed_at
                    .cmp(&right.state_changed_at)
                    .then_with(|| left.started_at.cmp(&right.started_at))
                    .then_with(|| left_id.cmp(right_id))
            })
            .map(|(vm_id, _)| vm_id.clone())
    }

    fn next_transition_ready_at(&self) -> Option<Instant> {
        self.vms
            .values()
            .filter(|metadata| matches!(metadata.state, VmState::Creating | VmState::Draining))
            .map(|metadata| metadata.ready_at)
            .min()
    }

    fn counters(&self) -> PoolCounters {
        let mut counters = PoolCounters::default();
        for metadata in self.vms.values() {
            match metadata.state {
                VmState::Creating => counters.creating += 1,
                VmState::Warm => counters.warm += 1,
                VmState::Leased => counters.leased += 1,
                VmState::Draining => counters.draining += 1,
            }
        }
        counters.failed = self.metrics.failed;
        counters
    }

    fn active_count(&self) -> usize {
        self.vms.len()
    }

    fn creating_count(&self) -> usize {
        self.counters().creating
    }

    fn warm_count(&self) -> usize {
        self.counters().warm
    }

    fn update_warm_surplus_marker(&mut self, now: Instant) {
        if self.warm_count() > self.config.scale_in_warm_threshold {
            if self.warm_surplus_since.is_none() {
                self.warm_surplus_since = Some(now);
            }
        } else {
            self.warm_surplus_since = None;
        }
    }
}

fn add_duration(now: Instant, duration: Duration) -> Result<Instant> {
    now.checked_add(duration)
        .ok_or_else(|| anyhow!("duration overflow while advancing Instant"))
}

fn duration_to_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{anyhow, bail};
    use std::collections::{HashSet, VecDeque};

    #[derive(Default)]
    struct FakeVmProvider {
        next_vm_id: usize,
        create_ready_after: VecDeque<Duration>,
        scrub_ready_after: VecDeque<Duration>,
        health_failures: HashSet<String>,
        scrub_failures: HashSet<String>,
        destroyed_vm_ids: Vec<String>,
    }

    impl FakeVmProvider {
        fn with_create_latencies(latencies: &[Duration]) -> Self {
            Self {
                create_ready_after: latencies.iter().copied().collect(),
                ..Self::default()
            }
        }

        fn fail_health_for(&mut self, vm_id: &str) {
            self.health_failures.insert(vm_id.to_string());
        }

        fn fail_scrub_for(&mut self, vm_id: &str) {
            self.scrub_failures.insert(vm_id.to_string());
        }
    }

    impl VmProvider for FakeVmProvider {
        fn create_vm(&mut self) -> Result<CreatedVm> {
            self.next_vm_id += 1;
            let vm_id = format!("vm-{}", self.next_vm_id);
            Ok(CreatedVm {
                guest_connection: GuestConnection {
                    api_socket_path: PathBuf::from(format!("/tmp/{vm_id}.sock")),
                    workload_socket_path: Some(PathBuf::from(format!(
                        "/tmp/{vm_id}.workload.sock"
                    ))),
                    vsock_port: Some(10_000 + self.next_vm_id as u32),
                },
                lineage_id: "rootfs.ext4".to_string(),
                snapshot_id: None,
                ready_after: self
                    .create_ready_after
                    .pop_front()
                    .unwrap_or(Duration::ZERO),
                vm_id,
                boot_source: BootSource::Boot,
            })
        }

        fn health_check(&mut self, vm: &VmMetadata) -> Result<()> {
            if self.health_failures.remove(&vm.vm_id) {
                bail!("health check failed for {}", vm.vm_id);
            }
            Ok(())
        }

        fn scrub_vm(&mut self, vm: &VmMetadata, _outcome: ReleaseOutcome) -> Result<DrainPlan> {
            if self.scrub_failures.remove(&vm.vm_id) {
                return Err(anyhow!("scrub failed for {}", vm.vm_id));
            }

            Ok(DrainPlan {
                ready_after: self.scrub_ready_after.pop_front().unwrap_or(Duration::ZERO),
            })
        }

        fn destroy_vm(&mut self, vm: &VmMetadata) -> Result<()> {
            self.destroyed_vm_ids.push(vm.vm_id.clone());
            Ok(())
        }
    }

    fn vm<'a>(snapshot: &'a PoolSnapshot, vm_id: &str) -> &'a VmMetadata {
        snapshot
            .vms
            .iter()
            .find(|entry| entry.vm_id == vm_id)
            .expect("vm should exist in snapshot")
    }

    #[test]
    fn refill_creates_minimum_warm_instances_from_empty_pool() {
        let start = Instant::now();
        let mut pool = FirecrackerVmPool::new(FakeVmProvider::default(), VmPoolConfig::default());

        let summary = pool.refill(start).expect("refill should succeed");
        let snapshot = pool.snapshot();

        assert_eq!(summary.started, 2);
        assert_eq!(snapshot.counters.creating, 0);
        assert_eq!(snapshot.counters.warm, 2);
        assert_eq!(snapshot.metrics.created, 2);
        assert_eq!(vm(&snapshot, "vm-1").state, VmState::Warm);
        assert_eq!(vm(&snapshot, "vm-2").state, VmState::Warm);
    }

    #[test]
    fn acquire_prefers_oldest_warm_vm_and_backfills_the_warm_floor() {
        let start = Instant::now();
        let mut pool = FirecrackerVmPool::new(FakeVmProvider::default(), VmPoolConfig::default());
        pool.refill(start).expect("refill should succeed");

        let lease = match pool
            .acquire(
                AcquireRequest {
                    timeout: Duration::from_secs(1),
                },
                start + Duration::from_secs(1),
            )
            .expect("acquire should succeed")
        {
            AcquireOutcome::Acquired(lease) => lease,
            AcquireOutcome::TimedOut => panic!("acquire should not time out"),
        };

        let snapshot = pool.snapshot();
        assert_eq!(lease.vm_id, "vm-1");
        assert_eq!(snapshot.counters.leased, 1);
        assert_eq!(snapshot.counters.warm, 2);
        assert_eq!(snapshot.metrics.created, 3);
        assert_eq!(snapshot.metrics.warm_hit, 1);
        assert_eq!(snapshot.metrics.cold_miss, 0);
        assert_eq!(snapshot.metrics.acquire_wait_ms, 0);
        assert_eq!(vm(&snapshot, "vm-1").state, VmState::Leased);
    }

    #[test]
    fn acquire_times_out_while_waiting_for_creating_vm_then_leases_it_once_ready() {
        let start = Instant::now();
        let provider = FakeVmProvider::with_create_latencies(&[Duration::from_millis(250)]);
        let mut pool = FirecrackerVmPool::new(provider, VmPoolConfig::default());

        let first_attempt = pool
            .acquire(
                AcquireRequest {
                    timeout: Duration::from_millis(100),
                },
                start,
            )
            .expect("acquire should return a timeout instead of error");

        assert_eq!(first_attempt, AcquireOutcome::TimedOut);
        let timed_out_snapshot = pool.snapshot();
        assert_eq!(timed_out_snapshot.counters.creating, 1);
        assert_eq!(timed_out_snapshot.counters.warm, 0);
        assert_eq!(timed_out_snapshot.metrics.cold_miss, 1);
        assert_eq!(timed_out_snapshot.metrics.acquire_wait_ms, 100);

        let second_attempt = pool
            .acquire(
                AcquireRequest {
                    timeout: Duration::from_millis(1),
                },
                start + Duration::from_millis(250),
            )
            .expect("second acquire should succeed");

        let lease = match second_attempt {
            AcquireOutcome::Acquired(lease) => lease,
            AcquireOutcome::TimedOut => panic!("second acquire should not time out"),
        };
        let snapshot = pool.snapshot();
        assert_eq!(lease.vm_id, "vm-1");
        assert_eq!(snapshot.counters.leased, 1);
        assert_eq!(snapshot.counters.warm, 2);
    }

    #[test]
    fn release_moves_vm_into_draining_before_it_returns_to_warm() {
        let start = Instant::now();
        let provider = FakeVmProvider {
            scrub_ready_after: VecDeque::from([Duration::from_millis(50)]),
            ..FakeVmProvider::default()
        };
        let mut pool = FirecrackerVmPool::new(provider, VmPoolConfig::default());
        pool.refill(start).expect("refill should succeed");

        let lease = match pool
            .acquire(
                AcquireRequest {
                    timeout: Duration::from_secs(1),
                },
                start + Duration::from_secs(1),
            )
            .expect("acquire should succeed")
        {
            AcquireOutcome::Acquired(lease) => lease,
            AcquireOutcome::TimedOut => panic!("acquire should not time out"),
        };

        let release_time = lease.acquired_at + Duration::from_millis(10);
        let disposition = pool
            .release(lease, ReleaseOutcome::Success, release_time)
            .expect("release should succeed");
        assert_eq!(disposition, ReleaseDisposition::Recycled);

        let draining_snapshot = pool.snapshot();
        assert_eq!(draining_snapshot.counters.draining, 1);
        assert_eq!(vm(&draining_snapshot, "vm-1").state, VmState::Draining);

        pool.reap(release_time + Duration::from_millis(25))
            .expect("reap during drain should succeed");
        assert_eq!(pool.snapshot().counters.draining, 1);

        pool.reap(release_time + Duration::from_millis(50))
            .expect("reap after drain should succeed");
        let warm_snapshot = pool.snapshot();
        assert_eq!(warm_snapshot.counters.draining, 0);
        assert_eq!(vm(&warm_snapshot, "vm-1").state, VmState::Warm);
        assert_eq!(vm(&warm_snapshot, "vm-1").reuse_count, 1);
        assert_eq!(warm_snapshot.metrics.recycled, 1);
    }

    #[test]
    fn reap_discards_stale_creating_vms() {
        let start = Instant::now();
        let config = VmPoolConfig {
            create_timeout: Duration::from_millis(100),
            ..VmPoolConfig::default()
        };
        let provider = FakeVmProvider::with_create_latencies(&[Duration::from_secs(1)]);
        let mut pool = FirecrackerVmPool::new(provider, config);

        let outcome = pool
            .acquire(
                AcquireRequest {
                    timeout: Duration::ZERO,
                },
                start,
            )
            .expect("acquire should return timeout");
        assert_eq!(outcome, AcquireOutcome::TimedOut);

        let summary = pool
            .reap(start + Duration::from_millis(150))
            .expect("reap should succeed");
        let snapshot = pool.snapshot();

        assert_eq!(summary.discarded, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(snapshot.counters.creating, 0);
        assert_eq!(snapshot.metrics.discarded, 1);
        assert_eq!(snapshot.metrics.failed, 1);
    }

    #[test]
    fn reap_scales_in_oldest_surplus_warm_vms_after_threshold() {
        let start = Instant::now();
        let mut pool = FirecrackerVmPool::new(FakeVmProvider::default(), VmPoolConfig::default());
        pool.refill(start).expect("refill should succeed");

        let mut leases = Vec::new();
        for seconds in 1..=3 {
            let lease = match pool
                .acquire(
                    AcquireRequest {
                        timeout: Duration::from_secs(1),
                    },
                    start + Duration::from_secs(seconds),
                )
                .expect("acquire should succeed")
            {
                AcquireOutcome::Acquired(lease) => lease,
                AcquireOutcome::TimedOut => panic!("acquire should not time out"),
            };
            leases.push(lease);
        }

        for (index, lease) in leases.into_iter().enumerate() {
            pool.release(
                lease,
                ReleaseOutcome::Success,
                start + Duration::from_secs(10 + index as u64),
            )
            .expect("release should succeed");
        }

        assert_eq!(pool.snapshot().counters.warm, 5);

        let early = pool
            .reap(start + Duration::from_secs(10 + 60))
            .expect("early reap should succeed");
        assert_eq!(early.discarded, 0);
        assert_eq!(pool.snapshot().counters.warm, 5);

        let late = pool
            .reap(start + Duration::from_secs(10 + 6 * 60))
            .expect("late reap should succeed");
        let snapshot = pool.snapshot();

        assert_eq!(late.discarded, 3);
        assert_eq!(snapshot.counters.warm, 2);
        assert_eq!(snapshot.metrics.discarded, 3);
        assert_eq!(vm(&snapshot, "vm-2").state, VmState::Warm);
        assert_eq!(vm(&snapshot, "vm-3").state, VmState::Warm);
    }

    #[test]
    fn release_discards_vm_when_post_drain_health_check_fails() {
        let start = Instant::now();
        let mut pool = FirecrackerVmPool::new(FakeVmProvider::default(), VmPoolConfig::default());
        pool.refill(start).expect("refill should succeed");

        let lease = match pool
            .acquire(
                AcquireRequest {
                    timeout: Duration::from_secs(1),
                },
                start + Duration::from_secs(1),
            )
            .expect("acquire should succeed")
        {
            AcquireOutcome::Acquired(lease) => lease,
            AcquireOutcome::TimedOut => panic!("acquire should not time out"),
        };

        pool.provider.fail_health_for("vm-1");
        let disposition = pool
            .release(
                lease,
                ReleaseOutcome::Success,
                start + Duration::from_secs(2),
            )
            .expect("release should succeed");

        assert_eq!(disposition, ReleaseDisposition::Discarded);
        let snapshot = pool.snapshot();
        assert_eq!(snapshot.counters.draining, 0);
        assert_eq!(snapshot.metrics.discarded, 1);
        assert_eq!(snapshot.metrics.failed, 1);
    }

    #[test]
    fn release_discards_vm_when_scrub_fails() {
        let start = Instant::now();
        let mut pool = FirecrackerVmPool::new(FakeVmProvider::default(), VmPoolConfig::default());
        pool.refill(start).expect("refill should succeed");

        let lease = match pool
            .acquire(
                AcquireRequest {
                    timeout: Duration::from_secs(1),
                },
                start + Duration::from_secs(1),
            )
            .expect("acquire should succeed")
        {
            AcquireOutcome::Acquired(lease) => lease,
            AcquireOutcome::TimedOut => panic!("acquire should not time out"),
        };

        pool.provider.fail_scrub_for("vm-1");
        let disposition = pool
            .release(
                lease,
                ReleaseOutcome::InfrastructureFailure,
                start + Duration::from_secs(2),
            )
            .expect("release should succeed");

        assert_eq!(disposition, ReleaseDisposition::Discarded);
        let snapshot = pool.snapshot();
        assert_eq!(snapshot.counters.warm, 2);
        assert_eq!(snapshot.metrics.discarded, 1);
        assert_eq!(snapshot.metrics.failed, 1);
        assert_eq!(snapshot.metrics.created, 3);
    }

    #[test]
    fn reap_backfills_minimum_warm_instances_after_discarding_stale_creating_vm() {
        let start = Instant::now();
        let config = VmPoolConfig {
            create_timeout: Duration::from_millis(100),
            ..VmPoolConfig::default()
        };
        let provider = FakeVmProvider::with_create_latencies(&[Duration::from_secs(1)]);
        let mut pool = FirecrackerVmPool::new(provider, config);

        let outcome = pool
            .acquire(
                AcquireRequest {
                    timeout: Duration::ZERO,
                },
                start,
            )
            .expect("acquire should return timeout");
        assert_eq!(outcome, AcquireOutcome::TimedOut);

        let summary = pool
            .reap(start + Duration::from_millis(150))
            .expect("reap should succeed");
        let snapshot = pool.snapshot();

        assert_eq!(summary.discarded, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(snapshot.counters.creating, 0);
        assert_eq!(snapshot.counters.warm, 2);
        assert_eq!(snapshot.metrics.discarded, 1);
        assert_eq!(snapshot.metrics.failed, 1);
        assert_eq!(snapshot.metrics.created, 3);
    }
}
