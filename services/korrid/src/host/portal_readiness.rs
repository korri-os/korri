//! One post-response catalog reflow per verified kiosk window instance.
use super::*;
use crate::host::{
    compositor_focus::FocusCandidate,
    display_placement::{self, SurfaceDisplay, SurfaceDisplayScope},
};

/// RG DS cold900.log: the successful repair was a second 900-high restore
/// about 500 ms after the first finished, not the startup request's own restore.
/// This spacing is a measured workaround, not a client-frame acknowledgement.
const CATALOG_READINESS_REFLOW_DELAY: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(super) struct CatalogReadiness {
    seen: Option<FocusCandidate>,
    pending: Option<CatalogReadinessRestore>,
}

pub(crate) struct CatalogReadinessRestore {
    instance: FocusCandidate,
    generation: Arc<()>,
    completed_at: Instant,
}
impl CatalogReadinessRestore {
    pub(crate) fn deadline(&self) -> Instant {
        self.completed_at + CATALOG_READINESS_REFLOW_DELAY
    }
    pub(crate) fn cancelled(&self, reason: &str) {
        eprintln!("<6>korrid: PortalReadinessRestore {{ outcome: Cancelled, pid: {}, con_id: {}, reason: {reason} }}", self.instance.pid, self.instance.node_id);
    }
}

impl HostSessionControl {
    pub(super) fn cached_catalog_rectangles(
        &self,
        pids: &std::collections::BTreeSet<i32>,
    ) -> Option<Vec<SurfaceDisplay>> {
        match self
            .placement_applied
            .lock()
            .expect("placement mutex poisoned")
            .as_ref()
        {
            Some(AppliedDisplayPlacement::Catalog {
                instance,
                rectangles,
                ..
            }) if pids.contains(&instance.pid) => Some(rectangles.clone()),
            _ => None,
        }
    }

    pub(super) fn invalidate_catalog_placement(&self) {
        let mut applied = self
            .placement_applied
            .lock()
            .expect("placement mutex poisoned");
        if matches!(*applied, Some(AppliedDisplayPlacement::Catalog { .. })) {
            *applied = None;
        }
    }

    pub(super) fn record_catalog_placement(
        &self,
        instance: FocusCandidate,
        rectangles: Vec<SurfaceDisplay>,
    ) {
        let generation = Arc::new(());
        *self
            .placement_applied
            .lock()
            .expect("placement mutex poisoned") = Some(AppliedDisplayPlacement::Catalog {
            instance: instance.clone(),
            rectangles,
            generation: generation.clone(),
        });
        let mut readiness = self
            .catalog_readiness
            .lock()
            .expect("catalog readiness mutex poisoned");
        if readiness.seen.as_ref() != Some(&instance) {
            if let Some(previous) = readiness.pending.take() {
                previous.cancelled("newer_kiosk_instance");
            }
            readiness.seen = Some(instance.clone());
            readiness.pending = Some(CatalogReadinessRestore {
                instance,
                generation,
                completed_at: Instant::now(),
            });
        }
    }

    pub(crate) fn take_catalog_readiness_restore(&self) -> Option<CatalogReadinessRestore> {
        self.catalog_readiness
            .lock()
            .expect("catalog readiness mutex poisoned")
            .pending
            .take()
    }

    pub(crate) fn restore_catalog_after_reply(&self, plan: CatalogReadinessRestore) {
        let mut state = self.lock_state_named("portal_readiness_restore");
        // Cancellation is read-only; it must not reconcile power/focus or thaw
        // the portal after a newer game operation acquired native authority.
        state.reconcile = false;
        if self.shutting_down.load(std::sync::atomic::Ordering::SeqCst) {
            plan.cancelled("shutdown");
            return;
        }
        let current = self.placement_applied.lock().expect("placement mutex poisoned").as_ref().is_some_and(|applied| {
            matches!(applied, AppliedDisplayPlacement::Catalog { instance, generation, .. } if instance == &plan.instance && Arc::ptr_eq(generation, &plan.generation))
        });
        if !current {
            plan.cancelled("newer_placement");
            return;
        }
        let validated = (|| {
            let pids = self.portal_pids()?;
            let compositor = self
                .compositor
                .as_ref()
                .ok_or("portal compositor unavailable")?;
            if crate::host::compositor_focus::portal_window(&compositor.tree()?, &pids)?
                != plan.instance
            {
                return Err("portal_instance_changed".to_string());
            }
            let displays = display_placement::configured_displays()?;
            Ok((pids, compositor, displays))
        })();
        let (pids, compositor, displays) = match validated {
            Ok(context) => context,
            Err(reason) => {
                plan.cancelled(&reason);
                return;
            }
        };
        let result = (|| {
            eprintln!("<6>korrid: PortalReadinessRestore {{ outcome: Started, pid: {}, con_id: {}, since_initial_restore_ms: {} }}", plan.instance.pid, plan.instance.node_id, plan.completed_at.elapsed().as_millis());
            let rectangles = display_placement::place_surface(
                compositor.as_ref(),
                &displays,
                SurfaceDisplayScope::All,
                None,
                &Default::default(),
                &pids,
            )?;
            *self
                .display_content
                .lock()
                .expect("display content mutex poisoned") = rectangles
                .iter()
                .map(|display| display.id.clone())
                .collect();
            if let Some(AppliedDisplayPlacement::Catalog {
                rectangles: cached, ..
            }) = self
                .placement_applied
                .lock()
                .expect("placement mutex poisoned")
                .as_mut()
            {
                *cached = rectangles;
            }
            Ok::<_, String>(())
        })();
        if let Err(reason) = result {
            // No mutation retry. A later explicit request may make a fresh
            // synchronous placement, but this instance never arms twice.
            self.invalidate_catalog_placement();
            eprintln!("<6>korrid: PortalReadinessRestore {{ outcome: Failed, pid: {}, con_id: {}, reason: {reason} }}", plan.instance.pid, plan.instance.node_id);
        }
    }
}
