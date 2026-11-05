//! Frames that draw nothing while an animation keeps asking for frames (Pico's focus
//! cycle between its 200 ms steps) are paced by the vendored winit backend
//! (`frame_throttle.rs`, `redraw_delay`): an unpresented frame gets no Wayland frame
//! callback, so the next redraw waits for the next frame interval instead of coming at
//! once. On the RG353M the loop ran 5600-8700 frames a second without it. The interval
//! counts from when the last frame started: counted from its end, frames that took 4 ms
//! to draw ran at about 48 a second (Pico on the RG353M at low clocks).

use i_slint_backend_winit::redraw_delay;
use std::time::Duration;

#[test]
fn quiet_animated_frames_keep_the_display_cadence() {
    for refresh in [None, Some(60000), Some(59940), Some(120000)] {
        let per_second = refresh.map_or(60, |r: u32| r.div_ceil(1000));
        for draw in [
            Duration::ZERO,
            Duration::from_millis(4),
            Duration::from_millis(7),
        ] {
            let mut now = Duration::ZERO;
            let mut frames = 0u32;
            while now < Duration::from_secs(1) {
                frames += 1;
                let started = now;
                now += draw;
                // The frame drew nothing, so it was not presented.
                let delay =
                    redraw_delay(false, refresh, now - started).expect("a quiet frame waits");
                now += delay;
            }
            assert!(
                frames <= per_second + 1 && frames + 2 >= per_second,
                "{frames} quiet frames in a second at {refresh:?}, drawing {draw:?} each"
            );
        }
    }
}

#[test]
fn a_presented_frame_is_paced_by_the_frame_callback() {
    assert_eq!(redraw_delay(true, Some(60000), Duration::ZERO), None);
}
