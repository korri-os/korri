use std::{sync::Arc, time::Duration};

use tokio::sync::Semaphore;

use crate::{
    cgroup_sandbox::{ActionCgroupBackend, FsActionCgroupBackend},
    direct_runner::DirectRunner,
};

pub use crate::direct_runner::{set_parent_non_dumpable, ActionOutcome, ActionOutput};

pub use crate::action_catalog::{
    action_entry, commands_from_environment, ActionCommand, ActionCommands, ActionConfigError,
    ActionId, ActionRoutes, DispatchMode, ACTION_CATALOG,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionIdentity {
    pub uid: u32,
    pub gid: u32,
    pub control_gid: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionLimits {
    pub max_concurrency: usize,
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

impl Default for ActionLimits {
    fn default() -> Self {
        Self {
            max_concurrency: 2,
            timeout: Duration::from_secs(10),
            max_output_bytes: 8 * 1024,
        }
    }
}

#[derive(Clone)]
pub struct ActionDispatcher {
    commands: Arc<ActionCommands>,
    identity: ActionIdentity,
    limits: ActionLimits,
    permits: Arc<Semaphore>,
    runner: DirectRunner,
}

impl ActionDispatcher {
    pub fn new(
        commands: ActionCommands,
        identity: ActionIdentity,
        limits: ActionLimits,
    ) -> Result<Self, ActionConfigError> {
        let backend = Arc::new(
            FsActionCgroupBackend::delegated()
                .map_err(|error| ActionConfigError::ContainmentUnavailable(error.to_string()))?,
        );
        Self::with_cgroup_backend(commands, identity, limits, backend)
    }

    pub fn with_cgroup_backend(
        commands: ActionCommands,
        identity: ActionIdentity,
        limits: ActionLimits,
        backend: Arc<dyn ActionCgroupBackend>,
    ) -> Result<Self, ActionConfigError> {
        if limits.max_concurrency == 0 {
            return Err(ActionConfigError::ZeroConcurrency);
        }
        if limits.timeout.is_zero() {
            return Err(ActionConfigError::ZeroTimeout);
        }
        if limits.max_output_bytes == 0 {
            return Err(ActionConfigError::ZeroOutputLimit);
        }
        if identity.uid == 0 || identity.gid == 0 {
            return Err(ActionConfigError::PrivilegedActionIdentity);
        }
        if identity.gid == identity.control_gid {
            return Err(ActionConfigError::ControlGroupRetained);
        }
        Ok(Self {
            commands: Arc::new(commands),
            identity,
            limits,
            permits: Arc::new(Semaphore::new(limits.max_concurrency)),
            runner: DirectRunner::new(backend),
        })
    }

    pub async fn dispatch(&self, action_id: ActionId) -> ActionOutcome {
        let Some(command) = self.commands.get(action_id).cloned() else {
            return ActionOutcome::Unconfigured;
        };
        let permit = match self.permits.try_acquire() {
            Ok(permit) => permit,
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                return ActionOutcome::ConcurrencyLimited;
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return ActionOutcome::SpawnFailed("action dispatcher closed".into());
            }
        };
        let outcome = self
            .runner
            .execute(action_id, command, self.identity, self.limits)
            .await;
        drop(permit);
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cgroup_sandbox::ActionCgroup;
    use std::{collections::BTreeMap, io, sync::Arc};

    #[derive(Default)]
    struct EmptyCgroups;

    impl ActionCgroupBackend for EmptyCgroups {
        fn create(&self, _action: ActionId) -> io::Result<ActionCgroup> {
            unreachable!()
        }
        fn attach(&self, _cgroup: &ActionCgroup, _pid: u32) -> io::Result<()> {
            unreachable!()
        }
        fn contains(&self, _cgroup: &ActionCgroup, _pid: u32) -> io::Result<bool> {
            unreachable!()
        }
        fn kill(&self, _cgroup: &ActionCgroup) -> io::Result<()> {
            unreachable!()
        }
        fn populated(&self, _cgroup: &ActionCgroup) -> io::Result<bool> {
            unreachable!()
        }
        fn remove(&self, _cgroup: &ActionCgroup) -> io::Result<()> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn concurrency_limit_rejects_instead_of_queueing_an_action() {
        let uid = unsafe { libc::geteuid() }.max(1);
        let gid = unsafe { libc::getegid() }.max(1);
        let control_gid = if gid == u32::MAX { gid - 1 } else { gid + 1 };
        let mut commands = ActionCommands::default();
        commands.insert(
            ActionId::WorkspaceNext,
            ActionCommand::new(
                std::fs::canonicalize(
                    std::env::var_os("KORRI_TEST_TRUE")
                        .expect("immutable true fixture from the inputd dev shell"),
                )
                .unwrap(),
                [],
                BTreeMap::new(),
            )
            .unwrap(),
        );
        let dispatcher = ActionDispatcher::with_cgroup_backend(
            commands,
            ActionIdentity {
                uid,
                gid,
                control_gid,
            },
            ActionLimits {
                max_concurrency: 1,
                ..ActionLimits::default()
            },
            Arc::new(EmptyCgroups),
        )
        .unwrap();
        let _active = dispatcher.permits.acquire().await.unwrap();

        assert_eq!(
            dispatcher.dispatch(ActionId::WorkspaceNext).await,
            ActionOutcome::ConcurrencyLimited
        );
    }

    #[test]
    fn parent_dump_protection_is_set_and_proven() {
        set_parent_non_dumpable().unwrap();
        assert_eq!(unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) }, 0);
    }
}
