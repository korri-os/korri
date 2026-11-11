use super::*;
use crate::launcher::plugin_launch::PluginDisplay;
use crate::linux_viewer::{LinuxViewerError, ViewerSnapshot};
use crate::live_display::{SessionViewerDisplay, SessionViewerDisplayRequest, ViewerDisplayEvent};
use futures::future::BoxFuture;

#[derive(Clone)]
pub(super) struct ClientShape {
    launch: String,
    route: SelectedRemoteRoute,
    local_connection: String,
    report_connection: String,
    offered: PluginDisplay,
    restore: Option<PluginDisplay>,
    last_failure: Option<String>,
    response: Option<SessionViewerDisplay>,
    busy: bool,
}
impl UpstreamRegistry {
    fn shape_client(
        &self,
        selected: &SelectedRemoteSession,
    ) -> Result<NativeClient, UpstreamError> {
        let SelectedRemoteRoute::Native { device_public_key } = &selected.route else {
            return Err(UpstreamError::SourcePeerNotFound);
        };
        let host = self
            .native_host_by_route_key(device_public_key)
            .ok_or(UpstreamError::SourcePeerNotFound)?;
        let RegisteredClient::Native(client) = &host.client else {
            return Err(UpstreamError::SourcePeerNotFound);
        };
        Ok(client.clone())
    }
    fn shape_cache(&self) -> Option<ClientShape> {
        let value = self
            .live_shape
            .lock()
            .expect("viewer shape cache poisoned")
            .clone();
        value.filter(|value| {
            self.selected()
                .is_some_and(|s| s.launch_id == value.launch && s.route == value.route)
        })
    }
    fn publish_shape(&self, value: ClientShape) {
        if self
            .selected()
            .is_some_and(|s| s.launch_id == value.launch && s.route == value.route)
        {
            *self.live_shape.lock().expect("viewer shape cache poisoned") = Some(value);
        }
    }
    async fn shape_event(
        &self,
        selected: &SelectedRemoteSession,
        event: ViewerDisplayEvent,
    ) -> Result<SessionViewerDisplay, UpstreamError> {
        self.shape_client(selected)?
            .viewer_display_report(SessionViewerDisplayRequest {
                expected_launch_id: selected.launch_id.clone(),
                event,
            })
            .await
    }
    pub(super) async fn synchronize_shape(
        &self,
        selected: &SelectedRemoteSession,
    ) -> Result<(), UpstreamError> {
        if selected.viewer.is_none() {
            return Ok(());
        }
        let observation = self
            .viewer
            .snapshot(&selected.launch_id)
            .await
            .ok()
            .flatten();
        let connected = observation.as_ref().and_then(|s| s.connected);
        let Some(observed) = observation.and_then(|s| s.shape) else {
            if self
                .shape_cache()
                .is_none_or(|c| c.launch != selected.launch_id)
            {
                return Ok(());
            }
            // An unavailable snapshot is not a disconnect. Only a confirmed
            // stopped viewer retires presence; an uncertain observation expires.
            if connected == Some(false)
                || matches!(
                    self.viewer.status(&selected.launch_id).await,
                    Ok(LinuxViewerStatus::Stopped)
                )
            {
                if let Some(cache) = self
                    .shape_cache()
                    .filter(|c| c.launch == selected.launch_id)
                {
                    let _ = self
                        .shape_event(
                            selected,
                            ViewerDisplayEvent::Leave {
                                connection: cache.report_connection,
                            },
                        )
                        .await;
                    *self.live_shape.lock().expect("viewer shape cache poisoned") = None;
                }
            } else if connected == Some(true) {
                if let Some(cache) = self
                    .shape_cache()
                    .filter(|c| c.launch == selected.launch_id)
                {
                    // A backgrounded activity can be gone while transport is
                    // still connected. Renew presence without inventing a size.
                    let _ = self
                        .shape_event(
                            selected,
                            ViewerDisplayEvent::Report {
                                connection: cache.report_connection,
                                display: cache.offered,
                            },
                        )
                        .await;
                }
            }
            return Ok(());
        };
        let mut cache = match self
            .shape_cache()
            .filter(|c| c.launch == selected.launch_id && c.local_connection == observed.connection)
        {
            Some(cache) => cache,
            None => ClientShape {
                launch: selected.launch_id.clone(),
                route: selected.route.clone(),
                local_connection: observed.connection.clone(),
                report_connection: crate::generate_launch_id(),
                offered: observed.encoded.clone(),
                response: None,
                restore: None,
                last_failure: None,
                busy: false,
            },
        };
        // The platform reports ready only after its screen-size events settle.
        // Do not add another polling interval to that producer-side debounce.
        if observed.ready && cache.restore.is_none() {
            cache.offered = observed.screen;
        }
        // Preserve the connection token before awaiting the RPC. A lost reply
        // is not a new transport connection and must not steal queue position.
        self.publish_shape(cache.clone());
        let result = self
            .shape_event(
                selected,
                ViewerDisplayEvent::Report {
                    connection: cache.report_connection.clone(),
                    display: cache.offered.clone(),
                },
            )
            .await;
        match result {
            Ok(response) => {
                self.apply_shape_response(selected, cache, response, &observed.encoded)
                    .await
            }
            Err(error) if error.code() == "StaleViewer" => {
                // The holder can expire presence during a network outage or
                // restart. Rejoin at the back, never revive the expired token.
                cache.report_connection = crate::generate_launch_id();
                cache.response = None;
                self.publish_shape(cache);
                Ok(())
            }
            Err(error) => {
                let code = error.code().to_owned();
                if code.starts_with("Display")
                    && cache.response.as_ref().is_some_and(|r| r.owner)
                    && cache.last_failure.as_deref() != Some(code.as_str())
                {
                    let _ = self
                        .viewer
                        .notice(&selected.launch_id, "Couldn't reshape")
                        .await;
                }
                cache.last_failure = Some(code);
                self.publish_shape(cache);
                Err(error)
            }
        }
    }
    async fn apply_shape_response(
        &self,
        selected: &SelectedRemoteSession,
        mut cache: ClientShape,
        mut response: SessionViewerDisplay,
        encoded: &PluginDisplay,
    ) -> Result<(), UpstreamError> {
        let mut encoded = encoded.clone();
        cache.response = Some(response.clone());
        if let Some(previous) = cache.restore.clone() {
            cache.busy = true;
            self.publish_shape(cache.clone());
            self.viewer
                .resize(
                    &selected.launch_id,
                    &cache.local_connection,
                    previous.clone(),
                )
                .await
                .map_err(|_| {
                    viewer_error(
                        "DisplayRollbackFailed",
                        "The previous stream size could not be confirmed",
                    )
                })?;
            encoded = previous;
            cache.restore = None;
        }
        // Presence reports must not undo a future quality controller's lower
        // pixel count at the same aspect. Only a new shape transaction, an
        // explicit rollback notice, or an aspect mismatch selects a new size.
        let was_pending = response.revision.is_some();
        let needs_size =
            was_pending || response.notice.is_some() || !same_aspect(&response.display, &encoded);
        cache.busy = response.owner && needs_size && response.display != encoded;
        self.publish_shape(cache.clone());
        if response.owner {
            let success = !needs_size
                || response.display == encoded
                || self
                    .viewer
                    .resize(
                        &selected.launch_id,
                        &cache.local_connection,
                        response.display.clone(),
                    )
                    .await
                    .is_ok();
            if let Some(revision) = response.revision.clone() {
                response = self
                    .shape_event(
                        selected,
                        if success {
                            ViewerDisplayEvent::Applied {
                                connection: cache.report_connection.clone(),
                                revision,
                            }
                        } else {
                            ViewerDisplayEvent::Refused {
                                connection: cache.report_connection.clone(),
                                revision,
                            }
                        },
                    )
                    .await?;
                if !success {
                    // A lost ACK can mean the encoder changed despite the
                    // refusal. Always confirm the previous encoded size too.
                    if let Some(message) = response.notice.take() {
                        let _ = self.viewer.notice(&selected.launch_id, &message).await;
                    }
                    cache.restore = Some(encoded.clone());
                    cache.response = Some(response.clone());
                    self.publish_shape(cache.clone());
                    self.viewer
                        .resize(
                            &selected.launch_id,
                            &cache.local_connection,
                            encoded.clone(),
                        )
                        .await
                        .map_err(|_| {
                            viewer_error(
                                "DisplayRollbackFailed",
                                "The previous stream size could not be confirmed",
                            )
                        })?;
                    cache.restore = None;
                }
            }
            if !success && !was_pending && cache.restore.is_none() {
                // An orphaned host transaction can report rollback without a
                // revision. Keep its encoder restoration pending until proved.
                cache.restore = Some(response.display.clone());
                cache.busy = true;
                if cache.last_failure.as_deref() != Some("EncoderUnconfirmed") {
                    let _ = self
                        .viewer
                        .notice(&selected.launch_id, "Couldn't reshape")
                        .await;
                }
                cache.last_failure = Some("EncoderUnconfirmed".into());
                self.publish_shape(cache);
                return Err(viewer_error(
                    "DisplayRollbackFailed",
                    "The stream size could not be confirmed",
                ));
            }
            if success {
                let mut active = self
                    .selected_remote_session
                    .lock()
                    .expect("selected remote session mutex poisoned");
                if let Some(active) = active
                    .as_mut()
                    .filter(|s| s.launch_id == selected.launch_id && s.route == selected.route)
                {
                    active.display = Some(response.display.clone());
                }
            }
        }
        if let Some(message) = &response.notice {
            let _ = self.viewer.notice(&selected.launch_id, message).await;
        }
        cache.busy = false;
        cache.last_failure = None;
        cache.response = Some(response);
        self.publish_shape(cache);
        Ok(())
    }
    async fn set_shape_portrait(
        &self,
        launch: &str,
        connection: &str,
        enabled: bool,
    ) -> Result<(), LinuxViewerError> {
        let registry = self
            .resolved()
            .map_err(|_| LinuxViewerError::new("Viewer route unavailable"))?;
        let _guard = registry.remote_prepare_mutation.lock().await;
        let selected = registry
            .selected()
            .filter(|s| s.launch_id == launch)
            .ok_or_else(|| LinuxViewerError::new("Session ended"))?;
        let cache = registry
            .shape_cache()
            .filter(|c| {
                c.launch == launch
                    && c.local_connection == connection
                    && c.response.as_ref().is_some_and(|r| r.owner)
            })
            .ok_or_else(|| LinuxViewerError::new("Only the shape owner can change orientation"))?;
        let encoded = registry
            .viewer
            .snapshot(launch)
            .await?
            .and_then(|s| s.shape)
            .filter(|s| s.connection == connection)
            .ok_or_else(|| LinuxViewerError::new("Viewer connection changed"))?
            .encoded;
        let response = registry
            .shape_event(
                &selected,
                ViewerDisplayEvent::Portrait {
                    connection: cache.report_connection.clone(),
                    enabled,
                },
            )
            .await
            .map_err(|_| LinuxViewerError::new("Couldn't reshape"))?;
        registry
            .apply_shape_response(&selected, cache, response, &encoded)
            .await
            .map_err(|_| LinuxViewerError::new("Couldn't reshape"))
    }
}

pub(super) struct ShapeControls(pub UpstreamRegistry);
impl ShapeControls {
    fn id(cache: &ClientShape) -> String {
        format!(
            "live-shape:{}:{}:portrait",
            cache.launch, cache.local_connection
        )
    }
}
impl LinuxStreamViewer for ShapeControls {
    fn change_signal(&self) -> Option<Arc<tokio::sync::Notify>> {
        self.0.viewer.change_signal()
    }
    fn snapshot<'a>(
        &'a self,
        launch: &'a str,
    ) -> BoxFuture<'a, Result<Option<ViewerSnapshot>, LinuxViewerError>> {
        Box::pin(async move {
            let mut snapshot = self.0.viewer.snapshot(launch).await?;
            if let (Some(snapshot), Some(cache)) = (
                &mut snapshot,
                self.0.shape_cache().filter(|c| c.launch == launch),
            ) {
                if let Some(response) = cache.response.as_ref().filter(|r| {
                    r.owner
                        && snapshot
                            .shape
                            .as_ref()
                            .is_some_and(|s| s.connection == cache.local_connection)
                }) {
                    snapshot
                        .controls
                        .push(korri_surface_treaty::SurfaceGameplayControl {
                            id: Self::id(&cache),
                            label: "Portrait".into(),
                            description: None,
                            enabled: !cache.busy
                                && snapshot.shape.as_ref().is_some_and(|s| s.ready),
                            disabled_reason: if cache.busy {
                                Some("Reshaping".into())
                            } else if !snapshot.shape.as_ref().is_some_and(|s| s.ready) {
                                Some("Live resizing unavailable".into())
                            } else {
                                None
                            },
                            destructive: false,
                            dismiss_on_success: false,
                            interaction:
                                korri_surface_treaty::SurfaceGameplayControlInteraction::Toggle {
                                    value: response.portrait,
                                    true_label: "On".into(),
                                    false_label: "Off".into(),
                                },
                        });
                }
            }
            Ok(snapshot)
        })
    }
    fn apply<'a>(
        &'a self,
        launch: &'a str,
        control: &'a str,
        value: Option<korri_surface_treaty::SurfaceGameplayControlValue>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        Box::pin(async move {
            if control.starts_with("live-shape:") {
                let cache = self
                    .0
                    .shape_cache()
                    .filter(|c| c.launch == launch && Self::id(c) == control)
                    .ok_or_else(|| LinuxViewerError::new("Stale shape control"))?;
                let Some(korri_surface_treaty::SurfaceGameplayControlValue::Toggle { value }) =
                    value
                else {
                    return Err(LinuxViewerError::new("Portrait requires a toggle"));
                };
                self.0
                    .set_shape_portrait(launch, &cache.local_connection, value)
                    .await
            } else {
                self.0.viewer.apply(launch, control, value).await
            }
        })
    }
    fn display(&self) -> Option<PluginDisplay> {
        self.0.viewer.display()
    }
    fn resize<'a>(
        &'a self,
        l: &'a str,
        c: &'a str,
        d: PluginDisplay,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        self.0.viewer.resize(l, c, d)
    }
    fn start<'a>(
        &'a self,
        l: &'a str,
        a: &'a str,
        d: Option<PluginDisplay>,
    ) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        self.0.viewer.start(l, a, d)
    }
    fn status<'a>(
        &'a self,
        l: &'a str,
    ) -> BoxFuture<'a, Result<LinuxViewerStatus, LinuxViewerError>> {
        self.0.viewer.status(l)
    }
    fn background<'a>(&'a self, l: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        self.0.viewer.background(l)
    }
    fn resume_and_focus<'a>(&'a self, l: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        self.0.viewer.resume_and_focus(l)
    }
    fn stop<'a>(&'a self, l: &'a str) -> BoxFuture<'a, Result<(), LinuxViewerError>> {
        self.0.viewer.stop(l)
    }
}

// Allow the same eight-pixel rounding used by native display shaping. This
// compares aspect, not quality: different pixel counts can have the same shape.
fn same_aspect(a: &PluginDisplay, b: &PluginDisplay) -> bool {
    if a.width == 0 || a.height == 0 || b.width == 0 || b.height == 0 {
        return false;
    }
    (u64::from(a.width) * u64::from(b.height)).abs_diff(u64::from(b.width) * u64::from(a.height))
        <= 8 * (u64::from(a.height) + u64::from(b.height))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quality_reduction_is_not_a_shape_change() {
        assert!(same_aspect(
            &PluginDisplay {
                width: 2448,
                height: 1848
            },
            &PluginDisplay {
                width: 1224,
                height: 924
            }
        ));
        assert!(same_aspect(
            &PluginDisplay {
                width: 2448,
                height: 1848
            },
            &PluginDisplay {
                width: 1280,
                height: 968
            }
        ));
        assert!(!same_aspect(
            &PluginDisplay {
                width: 2448,
                height: 1848
            },
            &PluginDisplay {
                width: 1968,
                height: 1248
            }
        ));
        assert!(!same_aspect(
            &PluginDisplay {
                width: 2448,
                height: 1848
            },
            &PluginDisplay {
                width: 1848,
                height: 2448
            }
        ));
    }
}
