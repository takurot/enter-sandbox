use crate::vm_pool::{BootSource, CreatedVm, DrainPlan, ReleaseOutcome, VmMetadata, VmProvider};
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotArtifact {
    pub snapshot_id: String,
    pub lineage_id: String,
    pub state_path: PathBuf,
    pub memory_path: PathBuf,
}

pub trait SnapshotControlPlane {
    fn latest_snapshot(&mut self, lineage_id: &str) -> Result<Option<SnapshotArtifact>>;
    fn restore_snapshot(&mut self, snapshot: &SnapshotArtifact) -> Result<CreatedVm>;
    fn create_vm_from_boot(&mut self) -> Result<CreatedVm>;
    fn record_restore_failure(
        &mut self,
        _snapshot: &SnapshotArtifact,
        _error_message: &str,
    ) -> Result<()> {
        Ok(())
    }
    fn health_check(&mut self, vm: &VmMetadata) -> Result<()>;
    fn scrub_vm(&mut self, vm: &VmMetadata, outcome: ReleaseOutcome) -> Result<DrainPlan>;
    fn destroy_vm(&mut self, vm: &VmMetadata) -> Result<()>;
}

pub struct SnapshotAwareProvider<P> {
    inner: P,
    preferred_lineage_id: String,
    quarantined_snapshot_ids: BTreeSet<String>,
}

impl<P> SnapshotAwareProvider<P> {
    pub fn new(inner: P, preferred_lineage_id: impl Into<String>) -> Self {
        Self {
            inner,
            preferred_lineage_id: preferred_lineage_id.into(),
            quarantined_snapshot_ids: BTreeSet::new(),
        }
    }

    pub fn inner(&self) -> &P {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.inner
    }
}

impl<P: SnapshotControlPlane> VmProvider for SnapshotAwareProvider<P> {
    fn create_vm(&mut self) -> Result<CreatedVm> {
        if let Some(snapshot) = self.inner.latest_snapshot(&self.preferred_lineage_id)? {
            if !self
                .quarantined_snapshot_ids
                .contains(snapshot.snapshot_id.as_str())
            {
                match self.inner.restore_snapshot(&snapshot) {
                    Ok(created_vm) => {
                        return Ok(CreatedVm {
                            boot_source: BootSource::Snapshot,
                            lineage_id: snapshot.lineage_id.clone(),
                            snapshot_id: Some(snapshot.snapshot_id.clone()),
                            ..created_vm
                        });
                    }
                    Err(error) => {
                        self.quarantined_snapshot_ids
                            .insert(snapshot.snapshot_id.clone());
                        self.inner
                            .record_restore_failure(&snapshot, &error.to_string())?;
                    }
                }
            }
        }

        let created_vm = self.inner.create_vm_from_boot()?;
        Ok(CreatedVm {
            boot_source: BootSource::Boot,
            snapshot_id: None,
            ..created_vm
        })
    }

    fn health_check(&mut self, vm: &VmMetadata) -> Result<()> {
        self.inner.health_check(vm)
    }

    fn scrub_vm(&mut self, vm: &VmMetadata, outcome: ReleaseOutcome) -> Result<DrainPlan> {
        self.inner.scrub_vm(vm, outcome)
    }

    fn destroy_vm(&mut self, vm: &VmMetadata) -> Result<()> {
        self.inner.destroy_vm(vm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm_pool::{
        AcquireOutcome, AcquireRequest, FirecrackerVmPool, GuestConnection, ReleaseOutcome,
        VmPoolConfig,
    };
    use anyhow::bail;
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct FakeSnapshotControlPlane {
        latest_snapshot: Option<SnapshotArtifact>,
        restore_errors: VecDeque<String>,
        restore_attempts: Vec<String>,
        latest_snapshot_requests: Vec<String>,
        boot_creations: usize,
        next_vm_id: usize,
        restore_failure_records: Vec<(String, String)>,
    }

    impl FakeSnapshotControlPlane {
        fn with_snapshot(snapshot_id: &str) -> Self {
            Self {
                latest_snapshot: Some(SnapshotArtifact {
                    snapshot_id: snapshot_id.to_string(),
                    lineage_id: "rootfs.ext4".to_string(),
                    state_path: PathBuf::from(format!("/snapshots/{snapshot_id}.vmstate")),
                    memory_path: PathBuf::from(format!("/snapshots/{snapshot_id}.mem")),
                }),
                ..Self::default()
            }
        }

        fn next_created_vm(&mut self, vm_id_prefix: &str, ready_after: Duration) -> CreatedVm {
            self.next_vm_id += 1;
            let vm_id = format!("{vm_id_prefix}-{}", self.next_vm_id);
            CreatedVm {
                vm_id: vm_id.clone(),
                boot_source: BootSource::Boot,
                guest_connection: GuestConnection {
                    api_socket_path: PathBuf::from(format!("/tmp/{vm_id}.sock")),
                    workload_socket_path: Some(PathBuf::from(format!(
                        "/tmp/{vm_id}.workload.sock"
                    ))),
                    vsock_port: Some(11_000 + self.next_vm_id as u32),
                },
                lineage_id: "boot-lineage".to_string(),
                snapshot_id: None,
                ready_after,
            }
        }
    }

    impl SnapshotControlPlane for FakeSnapshotControlPlane {
        fn latest_snapshot(&mut self, lineage_id: &str) -> Result<Option<SnapshotArtifact>> {
            self.latest_snapshot_requests.push(lineage_id.to_string());
            Ok(self.latest_snapshot.clone())
        }

        fn restore_snapshot(&mut self, snapshot: &SnapshotArtifact) -> Result<CreatedVm> {
            self.restore_attempts.push(snapshot.snapshot_id.clone());
            if let Some(message) = self.restore_errors.pop_front() {
                bail!(message);
            }

            Ok(self.next_created_vm("snapshot-vm", Duration::ZERO))
        }

        fn create_vm_from_boot(&mut self) -> Result<CreatedVm> {
            self.boot_creations += 1;
            Ok(self.next_created_vm("boot-vm", Duration::from_millis(10)))
        }

        fn record_restore_failure(
            &mut self,
            snapshot: &SnapshotArtifact,
            error_message: &str,
        ) -> Result<()> {
            self.restore_failure_records
                .push((snapshot.snapshot_id.clone(), error_message.to_string()));
            Ok(())
        }

        fn health_check(&mut self, _vm: &VmMetadata) -> Result<()> {
            Ok(())
        }

        fn scrub_vm(&mut self, _vm: &VmMetadata, _outcome: ReleaseOutcome) -> Result<DrainPlan> {
            Ok(DrainPlan {
                ready_after: Duration::ZERO,
            })
        }

        fn destroy_vm(&mut self, _vm: &VmMetadata) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn snapshot_aware_provider_prefers_snapshot_restore_when_available() {
        let control_plane = FakeSnapshotControlPlane::with_snapshot("snap-001");
        let mut provider = SnapshotAwareProvider::new(control_plane, "rootfs.ext4");

        let created_vm = provider
            .create_vm()
            .expect("snapshot restore should succeed");

        assert_eq!(created_vm.boot_source, BootSource::Snapshot);
        assert_eq!(created_vm.snapshot_id.as_deref(), Some("snap-001"));
        assert_eq!(created_vm.lineage_id, "rootfs.ext4");
        assert_eq!(provider.inner().restore_attempts, vec!["snap-001"]);
        assert_eq!(provider.inner().boot_creations, 0);
        assert_eq!(
            provider.inner().latest_snapshot_requests,
            vec!["rootfs.ext4".to_string()]
        );
    }

    #[test]
    fn snapshot_aware_provider_falls_back_to_boot_when_restore_fails() {
        let mut control_plane = FakeSnapshotControlPlane::with_snapshot("snap-002");
        control_plane
            .restore_errors
            .push_back("snapshot resume failed".to_string());
        let mut provider = SnapshotAwareProvider::new(control_plane, "rootfs.ext4");

        let created_vm = provider
            .create_vm()
            .expect("provider should fall back to boot");

        assert_eq!(created_vm.boot_source, BootSource::Boot);
        assert_eq!(created_vm.snapshot_id, None);
        assert_eq!(provider.inner().restore_attempts, vec!["snap-002"]);
        assert_eq!(provider.inner().boot_creations, 1);
        assert_eq!(
            provider.inner().restore_failure_records,
            vec![("snap-002".to_string(), "snapshot resume failed".to_string(),)]
        );
    }

    #[test]
    fn snapshot_aware_provider_quarantines_failed_snapshot_ids() {
        let mut control_plane = FakeSnapshotControlPlane::with_snapshot("snap-003");
        control_plane
            .restore_errors
            .push_back("snapshot corrupted".to_string());
        let mut provider = SnapshotAwareProvider::new(control_plane, "rootfs.ext4");

        let first = provider
            .create_vm()
            .expect("first call should fall back to boot after restore failure");
        let second = provider
            .create_vm()
            .expect("quarantined snapshot should be skipped");

        assert_eq!(first.boot_source, BootSource::Boot);
        assert_eq!(second.boot_source, BootSource::Boot);
        assert_eq!(provider.inner().restore_attempts, vec!["snap-003"]);
        assert_eq!(provider.inner().boot_creations, 2);
    }

    #[test]
    fn firecracker_vm_pool_preserves_snapshot_metadata_on_warm_and_leased_vms() {
        let start = Instant::now();
        let control_plane = FakeSnapshotControlPlane::with_snapshot("snap-004");
        let provider = SnapshotAwareProvider::new(control_plane, "rootfs.ext4");
        let mut pool = FirecrackerVmPool::new(provider, VmPoolConfig::default());

        pool.refill(start).expect("refill should succeed");
        let warm_snapshot = pool.snapshot();

        assert_eq!(warm_snapshot.counters.warm, 2);
        assert!(warm_snapshot
            .vms
            .iter()
            .all(|vm| vm.boot_source == BootSource::Snapshot));
        assert!(warm_snapshot
            .vms
            .iter()
            .all(|vm| vm.snapshot_id.as_deref() == Some("snap-004")));

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

        assert_eq!(lease.boot_source, BootSource::Snapshot);
        assert_eq!(lease.snapshot_id.as_deref(), Some("snap-004"));
        assert_eq!(lease.lineage_id, "rootfs.ext4");
    }
}
