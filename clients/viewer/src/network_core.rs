// SPDX-License-Identifier: GPL-3.0-or-later
//! The seam between the session part and its network core (#391 step E):
//! the connection's start and stop, the platform's video and audio
//! callbacks, the input, the core's clock and readings, and the control
//! events. Each core implements `NetworkCore`.
//!
//! One core runs a stream. `select` chooses it before the connection
//! starts, and every input packet and reading goes to that core
//! (`active`): the picture, sound and input code of a platform never names
//! a core. `nightjar.rs` is nightjar-core, built with the `nightjar`
//! feature, on Linux and Android, and then the default (ADR 0062: it
//! measured equal); Moonlight's C core (`moonlight_common_c.rs`) is the
//! default in a build without it and the fallback by option.
//!
//! The platform's video and audio code stays behind the C core's callback
//! tables (`DECODER_RENDERER_CALLBACKS`, `AUDIO_RENDERER_CALLBACKS`) and its
//! control events behind `CONNECTION_LISTENER_CALLBACKS`: Linux's carried C
//! code and Android's Rust code both implement those tables today, and a
//! core that is not the C core calls them as the C core does.
mod moonlight_common_c;
#[cfg(feature = "nightjar")]
mod nightjar;
// The carried C picture and input code's calls into the core.
#[cfg(not(target_os = "android"))]
mod c_calls;

use crate::network as ffi;
use anyhow::{Result, bail};
use std::{ffi::c_void, sync::RwLock};

/// Which network core streams. `-networkcore` names it; the default when
/// nothing does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Moonlight's `moonlight-common-c`, through Korri's binding.
    MoonlightCommonC,
    /// nightjar-core, vendored in `vendor/nightjar`.
    Nightjar,
}

impl Default for Kind {
    /// nightjar-core where the build has it (#391 step E: it measured equal
    /// on Linux, ADR 0062, and Android's app passed its emulator proof with
    /// it); the C core in a build without it.
    fn default() -> Self {
        if cfg!(feature = "nightjar") {
            Self::Nightjar
        } else {
            Self::MoonlightCommonC
        }
    }
}

impl Kind {
    /// `-networkcore`'s value: `c` or `nightjar`.
    pub fn parse(name: &str) -> Result<Self> {
        match name {
            "c" => Ok(Self::MoonlightCommonC),
            #[cfg(feature = "nightjar")]
            "nightjar" => Ok(Self::Nightjar),
            #[cfg(not(feature = "nightjar"))]
            "nightjar" => bail!("this build has no nightjar network core (cargo feature nightjar)"),
            _ => bail!("unknown network core {name}: c or nightjar"),
        }
    }
}

/// The state of one controller as the stream carries it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ControllerState {
    pub number: u16,
    pub active_mask: u16,
    pub buttons: u32,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_stick_x: i16,
    pub left_stick_y: i16,
    pub right_stick_x: i16,
    pub right_stick_y: i16,
}

/// One input packet: one `LiSend*` call of the C core. A `LiSendKeyboardEvent`
/// is `LiSendKeyboardEvent2` with no flags, and a `LiSendScrollEvent` of N
/// clicks a `LiSendHighResScrollEvent` of N × 120, as the C core makes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Packet {
    /// `LiSendKeyboardEvent2`.
    Keyboard {
        code: i16,
        down: bool,
        modifiers: u8,
        flags: u8,
    },
    /// `LiSendMultiControllerEvent`.
    Controller(ControllerState),
    /// `LiSendControllerArrivalEvent`.
    Arrival {
        number: u8,
        active_mask: u16,
        kind: u8,
        buttons: u32,
        capabilities: u16,
    },
    /// `LiSendMouseMoveEvent`.
    MouseMove { dx: i16, dy: i16 },
    /// `LiSendMousePositionEvent`.
    MousePosition {
        x: i16,
        y: i16,
        width: i16,
        height: i16,
    },
    /// `LiSendMouseButtonEvent`.
    MouseButton { pressed: bool, button: u8 },
    /// `LiSendHighResScrollEvent`.
    Scroll { amount: i16 },
    /// `LiSendHighResHScrollEvent`.
    HScroll { amount: i16 },
    /// `LiSendTouchEvent`.
    Touch {
        kind: u8,
        pointer: u32,
        x: f32,
        y: f32,
        pressure: f32,
        contact_major: f32,
        contact_minor: f32,
        rotation: u16,
    },
    /// `LiSendControllerTouchEvent`: a controller's own touchpad (Linux's
    /// SDL controllers).
    ControllerTouch {
        number: u8,
        kind: u8,
        pointer: u32,
        x: f32,
        y: f32,
        pressure: f32,
    },
    /// `LiSendControllerMotionEvent` (Linux's SDL controllers).
    ControllerMotion {
        number: u8,
        kind: u8,
        x: f32,
        y: f32,
        z: f32,
    },
    /// `LiSendControllerBatteryEvent`, from Android's feedback thread
    /// (`feedback.rs`).
    Battery {
        number: u8,
        state: u8,
        percentage: u8,
    },
}

/// What `LiStartConnection` takes: the host, the stream and the
/// platform's callbacks. Every pointer stays valid until the core's `stop`
/// returns. A null decoder or audio table gives the core's own empty one;
/// the C core fills a table's missing functions in place.
pub(crate) struct Connection<'a> {
    pub server: &'a mut ffi::SERVER_INFORMATION,
    pub stream: &'a mut ffi::STREAM_CONFIGURATION,
    pub listener: &'a mut ffi::CONNECTION_LISTENER_CALLBACKS,
    pub video: *mut ffi::DECODER_RENDERER_CALLBACKS,
    pub audio: *mut ffi::AUDIO_RENDERER_CALLBACKS,
    /// The decoder's flags (`drFlags`): fullscreen and rotation.
    pub video_flags: i32,
    /// The audio callbacks' context: Linux's audio device name.
    pub audio_context: *mut c_void,
}

/// A network core. One connection at a time: `stream.rs` holds its lock
/// from `start` to `stop`.
pub(crate) trait NetworkCore: Sync {
    /// The core's own launch query parameters (`LiGetLaunchUrlQueryParameters`),
    /// `&key=value` pairs.
    fn launch_parameters(&self) -> Result<&'static str>;
    /// `LiStartConnection`: blocks for the whole handshake. 0 when the
    /// stream runs, else the failed stage's error, which the listener's
    /// `stageFailed` has already had.
    fn start(&self, connection: Connection<'_>) -> i32;
    /// `LiInterruptConnection`, from another thread: a `start` under way
    /// ends at its next point of interruption. A start may clear it as it
    /// begins, so the caller repeats it until `start` returns.
    fn interrupt(&self);
    /// `LiStopConnection`: ends the stream and joins the core's threads; no
    /// callback runs after it. Only after a `start` that returned 0.
    fn stop(&self);
    /// One input packet. 0 when queued, else the C core's codes: -2 with
    /// no stream, `LI_ERR_UNSUPPORTED` (-5501) when the host takes no such
    /// packet.
    fn send(&self, packet: Packet) -> i32;
    /// The core's clock in microseconds (`LiGetMicroseconds`): the clock of
    /// each decode unit's receive and enqueue times.
    fn now_us(&self) -> u64;
    /// The estimated round trip in milliseconds, when the core has one
    /// (`LiGetEstimatedRttInfo`).
    #[cfg_attr(
        not(target_os = "android"),
        allow(dead_code, reason = "Android's stream health reads it")
    )]
    fn rtt_ms(&self) -> Option<u32>;
    /// Milliseconds of sound waiting for the audio callbacks
    /// (`LiGetPendingAudioDuration`).
    #[cfg_attr(
        not(target_os = "android"),
        allow(dead_code, reason = "Android's sound reads it")
    )]
    fn pending_audio_ms(&self) -> i32;
}

static MOONLIGHT_COMMON_C: moonlight_common_c::MoonlightCommonC =
    moonlight_common_c::MoonlightCommonC;
#[cfg(feature = "nightjar")]
static NIGHTJAR: nightjar::Nightjar = nightjar::Nightjar::new();

static ACTIVE: RwLock<&'static dyn NetworkCore> = RwLock::new(&MOONLIGHT_COMMON_C);

/// The core of `kind`, which every input packet and reading reaches from
/// now on. Only between streams: under `stream.rs`'s connection lock,
/// before `start`.
pub(crate) fn select(kind: Kind) -> &'static dyn NetworkCore {
    let core: &'static dyn NetworkCore = match kind {
        Kind::MoonlightCommonC => &MOONLIGHT_COMMON_C,
        #[cfg(feature = "nightjar")]
        Kind::Nightjar => &NIGHTJAR,
        #[cfg(not(feature = "nightjar"))]
        Kind::Nightjar => unreachable!("Kind::parse refuses nightjar in this build"),
    };
    install(core);
    core
}

fn install(core: &'static dyn NetworkCore) {
    *ACTIVE.write().unwrap_or_else(|p| p.into_inner()) = core;
}

/// The core that `select` chose last; the C core before any choice.
pub(crate) fn active() -> &'static dyn NetworkCore {
    *ACTIVE.read().unwrap_or_else(|p| p.into_inner())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// The tests that change the active core take turns.
    static TURN: Mutex<()> = Mutex::new(());

    /// A core that records what reaches it and answers `answer`.
    pub(crate) struct Recording {
        pub packets: Mutex<Vec<Packet>>,
        pub answer: i32,
        pub clock_us: u64,
    }

    impl NetworkCore for Recording {
        fn launch_parameters(&self) -> Result<&'static str> {
            Ok("&corever=1")
        }
        fn start(&self, _: Connection<'_>) -> i32 {
            -1
        }
        fn interrupt(&self) {}
        fn stop(&self) {}
        fn send(&self, packet: Packet) -> i32 {
            self.packets.lock().unwrap().push(packet);
            self.answer
        }
        fn now_us(&self) -> u64 {
            self.clock_us
        }
        fn rtt_ms(&self) -> Option<u32> {
            Some(7)
        }
        fn pending_audio_ms(&self) -> i32 {
            40
        }
    }

    /// Makes `core` the active one until the guard goes; then the C core
    /// is active again.
    pub(crate) struct Installed(#[allow(dead_code)] MutexGuard<'static, ()>);

    impl Drop for Installed {
        fn drop(&mut self) {
            install(&MOONLIGHT_COMMON_C);
        }
    }

    pub(crate) fn installed(core: &'static dyn NetworkCore) -> Installed {
        let turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        install(core);
        Installed(turn)
    }

    // ADR 0062: nightjar-core measured equal (#391 step E), so a build with
    // it streams with it; `-networkcore c` keeps the C core.
    #[test]
    fn nightjar_streams_where_built_unless_c_is_named() {
        let built = cfg!(feature = "nightjar");
        assert_eq!(
            Kind::default(),
            if built {
                Kind::Nightjar
            } else {
                Kind::MoonlightCommonC
            }
        );
        assert_eq!(Kind::parse("c").unwrap(), Kind::MoonlightCommonC);
        assert!(Kind::parse("moonlight-common-rust").is_err());
        assert!(Kind::parse("").is_err());
    }

    #[cfg(not(feature = "nightjar"))]
    #[test]
    fn a_build_without_nightjar_refuses_it_by_name() {
        let error = Kind::parse("nightjar").unwrap_err().to_string();
        assert!(error.contains("feature nightjar"), "{error}");
    }

    #[cfg(feature = "nightjar")]
    #[test]
    fn a_nightjar_build_takes_it_by_name() {
        assert_eq!(Kind::parse("nightjar").unwrap(), Kind::Nightjar);
    }

    #[test]
    fn input_and_readings_reach_the_selected_core() {
        static CORE: std::sync::LazyLock<Recording> = std::sync::LazyLock::new(|| Recording {
            packets: Mutex::new(Vec::new()),
            answer: -5501,
            clock_us: 1234,
        });
        let _installed = installed(&*CORE);
        let packet = Packet::MouseMove { dx: 3, dy: -4 };
        assert_eq!(active().send(packet), -5501);
        assert_eq!(*CORE.packets.lock().unwrap(), [packet]);
        assert_eq!(active().now_us(), 1234);
        assert_eq!(active().rtt_ms(), Some(7));
        assert_eq!(active().pending_audio_ms(), 40);
    }
}
