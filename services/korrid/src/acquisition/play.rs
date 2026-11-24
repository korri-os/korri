//! ADR 0092 (#556): installs yield to play.
//!
//! Stream play, netplay and a game that runs on a device come first. The
//! rule, from the signals korrid has today:
//!
//! Taking (the installing device: pieces from devices, provider and artifact
//! downloads):
//! - a netplay session runs here, or a game runs here (also one this device
//!   streams to others): installs pause;
//! - this device watches a stream: installs slow to `SLOW_BYTES_PER_SECOND`,
//!   and pause while the viewer's last sample (ADR 0088: about one second)
//!   shows a dropped frame or a round trip of `STREAM_RTT_PAUSE_MS` or more.
//!   The delivered bitrate has no target in korrid to compare with yet
//!   (ADR 0088's controller is not built), so it does not count.
//!
//! Serving (a device that holds a release and gives pieces to others):
//! - on battery (the battery does not charge): serve at
//!   `SLOW_BYTES_PER_SECOND`; below `LOW_BATTERY_PERCENT`, refuse every
//!   piece with "busy", which the receiver takes as "try another source";
//! - while it plays (any of the cases above): serve at
//!   `SLOW_BYTES_PER_SECOND`. A holder that streams a game to the device
//!   that installs it still serves, so a play-here install goes on behind
//!   its stream (ADR 0092).
//!
//! A paused install looks again every `PAUSE_CHECK`, so it resumes by itself
//! when play ends. The signals are read when an install needs them, at most
//! once per `PLAY_FRESH`; no timer runs while nothing installs.
use crate::RpcFailure;
use futures::future::BoxFuture;
use korri_surface_treaty::SurfaceBattery;
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use typeshare::typeshare;

/// A serving device on battery refuses pieces below this charge (ADR 0092).
pub(crate) const LOW_BATTERY_PERCENT: f64 = 20.0;
/// The pace of a slowed install, and of a device that serves slowly.
pub(crate) const SLOW_BYTES_PER_SECOND: u64 = 2 * 1024 * 1024;
/// A viewer round trip at or above this pauses installs on its device.
pub(crate) const STREAM_RTT_PAUSE_MS: f64 = 40.0;
/// How long one reading of the play signals is used.
const PLAY_FRESH: Duration = if cfg!(test) {
    Duration::from_millis(20)
} else {
    Duration::from_secs(1)
};
/// How often a paused install looks again.
pub(crate) const PAUSE_CHECK: Duration = if cfg!(test) {
    Duration::from_millis(20)
} else {
    Duration::from_secs(1)
};
/// How long a receiver leaves a source alone after it answered busy.
pub(crate) const BUSY_REST: Duration = if cfg!(test) {
    Duration::from_millis(50)
} else {
    Duration::from_secs(5)
};

/// How an install yields: it goes slower, or it waits.
#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum YieldPace {
    Slowed,
    Paused,
}

/// Why an install yields.
#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum YieldReason {
    /// A game runs on this device, also one that it streams to others.
    Game,
    /// A netplay session runs on this device.
    Netplay,
    /// This device watches a stream.
    Stream,
    /// Every source refused for now: it plays, or its battery is low.
    SourcesBusy,
}

/// An install that yields to play, and why (ADR 0092, #556).
#[typeshare]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InstallYield {
    pub pace: YieldPace,
    pub reason: YieldReason,
}

/// What this device does now that installs yield to.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Play {
    /// A game runs on this device (not frozen).
    pub game: bool,
    /// A netplay session runs on this device.
    pub netplay: bool,
    /// This device watches a stream: the viewer's last measurements.
    pub stream: Option<StreamHealth>,
    /// This device's battery (ADR 0094); none without one.
    pub battery: Option<SurfaceBattery>,
}

/// The ADR 0088 viewer measurements that installs back off for.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StreamHealth {
    /// Frames the display dropped in the last sample.
    pub frames_dropped: Option<f64>,
    pub rtt_ms: Option<f64>,
}

/// Where the play signals come from: the device's state in production, a
/// fixture in tests.
pub(crate) trait PlaySource: Send + Sync {
    fn play(&self) -> BoxFuture<'_, Play>;
}

/// The pace of installs on a device that does `play`.
pub(crate) fn taking(play: &Play) -> Option<InstallYield> {
    let paused = |reason| {
        Some(InstallYield {
            pace: YieldPace::Paused,
            reason,
        })
    };
    if play.netplay {
        return paused(YieldReason::Netplay);
    }
    if play.game {
        return paused(YieldReason::Game);
    }
    let stream = play.stream.as_ref()?;
    let struggles = stream.frames_dropped.is_some_and(|dropped| dropped > 0.0)
        || stream.rtt_ms.is_some_and(|rtt| rtt >= STREAM_RTT_PAUSE_MS);
    Some(InstallYield {
        pace: if struggles {
            YieldPace::Paused
        } else {
            YieldPace::Slowed
        },
        reason: YieldReason::Stream,
    })
}

/// How a device that does `play` serves pieces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Serve {
    Free,
    Slowed,
    Refuse,
}

pub(crate) fn serving(play: &Play) -> Serve {
    if let Some(battery) = play.battery.as_ref().filter(|battery| !battery.charging) {
        // ADR 0094: the percent is known only where the device verified it.
        if battery
            .percent
            .is_some_and(|percent| percent < LOW_BATTERY_PERCENT)
        {
            return Serve::Refuse;
        }
        return Serve::Slowed;
    }
    if play.game || play.netplay || play.stream.is_some() {
        return Serve::Slowed;
    }
    Serve::Free
}

/// The device's answer to "may installs go on now", shared by every
/// install of the device and by the pieces it serves.
#[derive(Clone, Default)]
pub(crate) struct Gate(Arc<Inner>);

#[derive(Default)]
struct Inner {
    source: Mutex<Option<Arc<dyn PlaySource>>>,
    reading: tokio::sync::Mutex<Option<(Instant, Play)>>,
    /// The last pace of installs and when it was read.
    last: Mutex<Option<(Instant, Option<InstallYield>)>>,
    /// The slowed budgets: when the next byte may go.
    taking: Mutex<Option<Instant>>,
    serving: Mutex<Option<Instant>>,
}

impl Gate {
    /// Read the play signals from `source` from now on.
    pub(crate) fn yield_to(&self, source: Arc<dyn PlaySource>) {
        *self.0.source.lock().expect("play source mutex poisoned") = Some(source);
    }

    async fn play(&self) -> Play {
        let source = self
            .0
            .source
            .lock()
            .expect("play source mutex poisoned")
            .clone();
        let Some(source) = source else {
            return Play::default();
        };
        let mut reading = self.0.reading.lock().await;
        if let Some((at, play)) = reading.as_ref() {
            if at.elapsed() < PLAY_FRESH {
                return play.clone();
            }
        }
        let play = source.play().await;
        *reading = Some((Instant::now(), play.clone()));
        play
    }

    /// The pace of installs on this device now.
    pub(crate) async fn taking(&self) -> Option<InstallYield> {
        let pace = taking(&self.play().await);
        *self.0.last.lock().expect("play mutex poisoned") = Some((Instant::now(), pace));
        pace
    }

    /// The pace that a running install last met. None while no install
    /// asked lately: no install yields then.
    #[cfg_attr(not(test), allow(dead_code))] // #553: Velvet's list of installs.
    pub(crate) fn taking_now(&self) -> Option<InstallYield> {
        self.0
            .last
            .lock()
            .expect("play mutex poisoned")
            .filter(|(at, _)| at.elapsed() < 3 * PAUSE_CHECK.max(PLAY_FRESH))
            .and_then(|(_, pace)| pace)
    }

    /// Before an install takes `bytes`: wait while it is paused, and spend
    /// the slowed budget. `may_pause` false turns a pause into a slow pace,
    /// for a download that cannot resume. `report` hears each pace. Returns
    /// whether it waited for a pause.
    pub(crate) async fn take(
        &self,
        bytes: u64,
        may_pause: bool,
        report: &mut impl FnMut(Option<InstallYield>),
    ) -> bool {
        let mut waited = false;
        loop {
            let mut pace = self.taking().await;
            if !may_pause {
                if let Some(pace) = pace.as_mut() {
                    pace.pace = YieldPace::Slowed;
                }
            }
            report(pace);
            match pace.map(|pace| pace.pace) {
                None => return waited,
                Some(YieldPace::Slowed) => {
                    spend(&self.0.taking, bytes).await;
                    return waited;
                }
                Some(YieldPace::Paused) => {
                    waited = true;
                    tokio::time::sleep(PAUSE_CHECK).await;
                }
            }
        }
    }

    /// Before this device serves `bytes`: false when it refuses, else true
    /// once the slowed budget allows it.
    pub(crate) async fn serve(&self, bytes: u64) -> bool {
        match serving(&self.play().await) {
            Serve::Refuse => false,
            Serve::Slowed => {
                spend(&self.0.serving, bytes).await;
                true
            }
            Serve::Free => true,
        }
    }
}

/// A leaky bucket at `SLOW_BYTES_PER_SECOND`: wait for the turn of `bytes`.
async fn spend(next: &Mutex<Option<Instant>>, bytes: u64) {
    let start = {
        let mut next = next.lock().expect("pace mutex poisoned");
        let now = Instant::now();
        let start = next.filter(|next| *next > now).unwrap_or(now);
        *next = Some(start + Duration::from_secs_f64(bytes as f64 / SLOW_BYTES_PER_SECOND as f64));
        start
    };
    tokio::time::sleep_until(start.into()).await;
}

/// The answer of a source that serves no piece for now.
pub(crate) fn busy() -> RpcFailure {
    RpcFailure {
        code: "InstallSourceBusy".into(),
        message: "The device that holds this release serves no pieces now.".into(),
    }
}

/// The play signals of this device: its game session, its netplay, the
/// stream it watches and its battery.
pub(crate) fn source(state: &crate::AppState) -> Arc<dyn PlaySource> {
    Arc::new(DeviceSignals {
        host: match &state.mode {
            crate::ServerMode::Host(host) => Some(host.clone()),
            _ => None,
        },
        receiver: state.netplay_receiver.clone(),
        holder: state.netplay_holder.clone(),
        remote: state.remote.clone(),
    })
}

struct DeviceSignals {
    host: Option<crate::host::HostRuntime>,
    receiver: crate::netplay::receiver::Receiver,
    holder: Arc<crate::netplay::holder::Holder>,
    remote: Option<crate::upstreams::UpstreamRegistry>,
}

/// How long one viewer reading may take before it counts as none.
const VIEWER_READ: Duration = Duration::from_millis(500);

impl PlaySource for DeviceSignals {
    fn play(&self) -> BoxFuture<'_, Play> {
        Box::pin(async move {
            use crate::host::control::HostSessionStatus as S;
            let running = match &self.host {
                Some(host) => match host.session_status().await {
                    Ok(S::Running { launch_id, .. } | S::FocusFailed { launch_id, .. }) => {
                        Some(launch_id)
                    }
                    _ => None,
                },
                None => None,
            };
            let netplay = self.receiver.playing()
                || match &running {
                    Some(launch) => self.holder.hosts(launch).await,
                    None => false,
                };
            let stream = match &self.remote {
                Some(remote) => match remote.selected_launch_id() {
                    Some(launch) => {
                        let snapshot = tokio::time::timeout(
                            VIEWER_READ,
                            remote.local_viewer().snapshot(&launch),
                        )
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .flatten();
                        let health = snapshot.and_then(|snapshot| snapshot.stream_health);
                        Some(StreamHealth {
                            frames_dropped: health.as_ref().and_then(|h| h.frames_dropped),
                            rtt_ms: health.as_ref().and_then(|h| h.rtt_ms),
                        })
                    }
                    None => None,
                },
                None => None,
            };
            Play {
                game: running.is_some(),
                netplay,
                stream,
                battery: battery(),
            }
        })
    }
}

/// ADR 0094's battery source: `/sys/class/power_supply`, read on kernel
/// events. Tests read no battery of the machine that runs them.
fn battery() -> Option<SurfaceBattery> {
    #[cfg(all(target_os = "linux", not(test)))]
    {
        crate::battery::device().borrow().clone()
    }
    #[cfg(any(not(target_os = "linux"), test))]
    {
        None
    }
}

/// A play source that a test sets by hand.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestPlay(pub Mutex<Play>);

#[cfg(test)]
impl TestPlay {
    pub(crate) fn set(&self, change: impl FnOnce(&mut Play)) {
        change(&mut self.0.lock().unwrap());
    }
}

#[cfg(test)]
impl PlaySource for TestPlay {
    fn play(&self) -> BoxFuture<'_, Play> {
        let play = self.0.lock().unwrap().clone();
        Box::pin(async move { play })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn battery(percent: Option<f64>, charging: bool) -> Option<SurfaceBattery> {
        Some(SurfaceBattery { percent, charging })
    }

    #[test]
    fn installs_pause_for_a_game_and_netplay_and_slow_for_a_healthy_stream() {
        let pace = |play: Play| taking(&play).map(|y| (y.pace, y.reason));
        assert_eq!(pace(Play::default()), None);
        assert_eq!(
            pace(Play {
                game: true,
                ..Default::default()
            }),
            Some((YieldPace::Paused, YieldReason::Game))
        );
        assert_eq!(
            pace(Play {
                game: true,
                netplay: true,
                ..Default::default()
            }),
            Some((YieldPace::Paused, YieldReason::Netplay))
        );
        let stream = |frames_dropped, rtt_ms| Play {
            stream: Some(StreamHealth {
                frames_dropped,
                rtt_ms,
            }),
            ..Default::default()
        };
        assert_eq!(
            pace(stream(None, None)),
            Some((YieldPace::Slowed, YieldReason::Stream))
        );
        assert_eq!(
            pace(stream(Some(0.0), Some(STREAM_RTT_PAUSE_MS - 1.0))),
            Some((YieldPace::Slowed, YieldReason::Stream))
        );
        assert_eq!(
            pace(stream(Some(1.0), Some(5.0))),
            Some((YieldPace::Paused, YieldReason::Stream))
        );
        assert_eq!(
            pace(stream(Some(0.0), Some(STREAM_RTT_PAUSE_MS))),
            Some((YieldPace::Paused, YieldReason::Stream))
        );
        // The battery does not change the pace of taking.
        assert_eq!(
            pace(Play {
                battery: battery(Some(5.0), false),
                ..Default::default()
            }),
            None
        );
    }

    #[test]
    fn a_device_on_battery_serves_slowly_and_stops_below_the_threshold() {
        let serve = |battery, game| {
            serving(&Play {
                battery,
                game,
                ..Default::default()
            })
        };
        assert_eq!(serve(None, false), Serve::Free);
        assert_eq!(serve(battery(Some(100.0), true), false), Serve::Free);
        assert_eq!(serve(battery(Some(10.0), true), false), Serve::Free);
        assert_eq!(serve(battery(Some(80.0), false), false), Serve::Slowed);
        // A device that has not verified its percent (ADR 0094) slows only.
        assert_eq!(serve(battery(None, false), false), Serve::Slowed);
        assert_eq!(
            serve(battery(Some(LOW_BATTERY_PERCENT), false), false),
            Serve::Slowed
        );
        assert_eq!(
            serve(battery(Some(LOW_BATTERY_PERCENT - 1.0), false), false),
            Serve::Refuse
        );
        assert_eq!(serve(None, true), Serve::Slowed);
    }
}
