//! Frames that draw nothing while an animation keeps asking for frames (Pico's focus
//! cycle between its 200 ms steps) are paced by the vendored winit backend
//! (`frame_throttle.rs`, `redraw_delay`): an unpresented frame gets no Wayland frame
//! callback, so the next redraw waits one frame interval instead of coming at once. On
//! the RG353M the loop ran 5600-8700 frames a second without it.

use i_slint_backend_winit::redraw_delay;
use std::time::Duration;

#[test]
fn quiet_animated_frames_run_at_most_once_a_frame_interval() {
    for refresh in [None, Some(60000), Some(59940), Some(120000)] {
        let mut now = Duration::ZERO;
        let mut frames = 0u32;
        while now < Duration::from_secs(1) {
            frames += 1;
            // The frame drew nothing, so it was not presented.
            now += redraw_delay(false, refresh).expect("a quiet frame waits");
        }
        let per_second = refresh.map_or(60, |r| r.div_ceil(1000));
        assert!(
            frames <= per_second + 1,
            "{frames} quiet frames in a second at {refresh:?}"
        );
        assert!(
            frames + 2 >= per_second,
            "{frames} quiet frames in a second at {refresh:?}"
        );
    }
}

#[test]
fn a_presented_frame_is_paced_by_the_frame_callback() {
    assert_eq!(redraw_delay(true, Some(60000)), None);
}
