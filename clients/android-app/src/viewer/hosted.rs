// SPDX-License-Identifier: GPL-3.0-or-later
//! The shell's gameplay overlay over one stream (#434 slice D5b): when the
//! overlay layer's Surface goes to the shell and comes back, when korrid
//! hears that the person left, and when the stream's input goes on. Pure:
//! each call returns its steps, and the loop thread performs them in their
//! order (`viewer.rs`). The library builds for Android only, so the build
//! machine runs these tests through `tests/host.rs`.
//!
//! Artemis's Korri mode does the same with a view of its own:
//!
//! | Artemis (`Game`, `KorriSurfaceHost`, `KorriStreamViewer`) | Here |
//! | --- | --- |
//! | `showSurfaceOverlay`: `setInputHeld(true)`, a new `SurfaceView` | the stream's input held (`Play`), then [`Hosted::open`]: the layer's Surface is offered to the shell |
//! | `surfaceCreated`: `KorriSurfaceHost.attach`, then `KorriStreamViewer.leave` | the shell takes the Surface ([`Hosted::take`]), then korrid's `session.leave` |
//! | The new view shows what the shell draws | The layer shows once the shell has drawn into it ([`Hosted::drawn`]): until then it keeps the last frame of an earlier opening |
//! | korrid's Resume: `hideSurfaceOverlay`; `KorriSurfaceHost.detach` waits until the shell has let the Surface go; the view goes; `setInputHeld(false)` | korrid's `resume_and_focus`: [`Hosted::close`] hides the layer, the shell lets the Surface go at its next tick ([`Hosted::released`]), then the stream's input goes on and the resume hears so. Nothing waits on Android's main thread |
//! | A Leave that fails, a Surface that the shell cannot use: `hideSurfaceOverlay` and a toast | The same close; the log says why. No toast |

/// One step for the loop thread, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Show the overlay layer: the shell has drawn this opening into it.
    Show,
    /// Hide the overlay layer.
    Hide,
    /// Tell korrid that the person left the stream (`session.leave`); its
    /// answer comes back to [`Hosted::left`] with this opening's number.
    Leave(u64),
    /// The overlay is closed and the shell has no Surface: the stream's
    /// input goes on, and each resume that waits hears so. Why, for the log.
    Closed(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Closed,
    /// The stream's input is held; the shell takes the Surface at its next
    /// tick.
    Offered,
    /// The shell has the Surface. `shown`: it has drawn into it, and the
    /// layer shows.
    Lent {
        shown: bool,
    },
    /// The shell lets the Surface go at its next tick.
    Returning,
}

#[derive(Debug)]
pub(crate) struct Hosted {
    state: State,
    /// The number of the latest opening.
    opening: u64,
    /// Why the overlay closes, once it does.
    why: &'static str,
}

impl Default for Hosted {
    fn default() -> Self {
        Self {
            state: State::Closed,
            opening: 0,
            why: "",
        }
    }
}

impl Hosted {
    /// The overlay opened and the stream's input is held: the shell may
    /// take the Surface. Nothing changes while an opening still runs.
    pub(crate) fn open(&mut self) {
        if self.state == State::Closed {
            self.state = State::Offered;
            self.opening += 1;
        }
    }

    /// The overlay is open, or still closing.
    pub(crate) fn is_open(&self) -> bool {
        self.state != State::Closed
    }

    /// The Surface waits for the shell to take it.
    pub(crate) fn offered(&self) -> bool {
        self.state == State::Offered
    }

    /// The shell holds the Surface: the layers must not go yet.
    pub(crate) fn lends(&self) -> bool {
        matches!(self.state, State::Lent { .. } | State::Returning)
    }

    /// The shell asks for a Surface (`Overlay::take_surface`). It gets this
    /// opening's once; then korrid hears that the person left, so that
    /// korrid's overlay finds a Surface to draw into.
    pub(crate) fn take(&mut self) -> Option<Step> {
        (self.state == State::Offered).then(|| {
            self.state = State::Lent { shown: false };
            Step::Leave(self.opening)
        })
    }

    /// The shell drew into the Surface: the layer shows, once an opening.
    pub(crate) fn drawn(&mut self) -> Option<Step> {
        (self.state == State::Lent { shown: false }).then(|| {
            self.state = State::Lent { shown: true };
            Step::Show
        })
    }

    /// korrid answered the Leave of `opening`. A refusal closes that
    /// opening; an answer for an earlier one changes nothing.
    pub(crate) fn left(&mut self, opening: u64, ok: bool) -> Vec<Step> {
        if ok || opening != self.opening {
            return Vec::new();
        }
        self.close("korrid refused the leave")
    }

    /// Close the overlay, for `why`. A Surface that the shell holds comes
    /// back first ([`Hosted::closing`]); one that it never took ends the
    /// overlay at once.
    pub(crate) fn close(&mut self, why: &'static str) -> Vec<Step> {
        match self.state {
            State::Closed | State::Returning => Vec::new(),
            State::Offered => {
                self.state = State::Closed;
                vec![Step::Closed(why)]
            }
            State::Lent { shown } => {
                self.state = State::Returning;
                self.why = why;
                if shown {
                    vec![Step::Hide]
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// The shell must let the Surface go now (`Overlay::closing`).
    pub(crate) fn closing(&self) -> bool {
        self.state == State::Returning
    }

    /// The shell let the Surface go (`Overlay::released`): because it was
    /// asked to, or because it could not draw (`failed`) or ends.
    pub(crate) fn released(&mut self, failed: bool) -> Vec<Step> {
        let state = std::mem::replace(&mut self.state, State::Closed);
        match state {
            State::Closed => Vec::new(),
            State::Returning => vec![Step::Closed(self.why)],
            State::Offered => vec![Step::Closed("the shell did not take the overlay's Surface")],
            State::Lent { shown } => {
                let why = if failed {
                    "the shell could not draw the overlay"
                } else {
                    "the shell let the overlay's Surface go"
                };
                let mut steps = if shown { vec![Step::Hide] } else { Vec::new() };
                steps.push(Step::Closed(why));
                steps
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESUMED: &str = "korrid resumed the stream";
    const NONE: [Step; 0] = [];

    /// An overlay that the shell has taken and drawn: the layer shows.
    fn shown() -> Hosted {
        let mut hosted = Hosted::default();
        hosted.open();
        assert_eq!(hosted.take(), Some(Step::Leave(1)));
        assert_eq!(hosted.drawn(), Some(Step::Show));
        hosted
    }

    #[test]
    fn the_shell_takes_the_surface_once_and_only_then_korrid_hears_the_leave() {
        let mut hosted = Hosted::default();
        assert_eq!(hosted.take(), None, "no Surface while closed");
        hosted.open();
        assert!(hosted.is_open() && hosted.offered() && !hosted.lends());
        assert_eq!(hosted.take(), Some(Step::Leave(1)));
        assert!(hosted.lends() && !hosted.offered());
        assert_eq!(hosted.take(), None, "one Surface an opening");
        hosted.open();
        assert_eq!(hosted.take(), None, "an open overlay does not open again");
    }

    #[test]
    fn the_layer_shows_only_once_the_shell_has_drawn_into_it() {
        let mut hosted = Hosted::default();
        hosted.open();
        assert_eq!(
            hosted.drawn(),
            None,
            "nothing drawn before the shell has the Surface"
        );
        hosted.take();
        assert_eq!(hosted.drawn(), Some(Step::Show));
        assert_eq!(hosted.drawn(), None, "shown once");
    }

    #[test]
    fn a_resume_hides_the_layer_and_the_input_goes_on_after_the_shell_let_the_surface_go() {
        let mut hosted = shown();
        assert_eq!(hosted.close(RESUMED), [Step::Hide]);
        assert!(
            hosted.is_open(),
            "the input stays held until the Surface is back"
        );
        assert!(hosted.closing() && hosted.lends());
        assert_eq!(hosted.close("again"), NONE, "one close at a time");
        assert_eq!(hosted.released(false), [Step::Closed(RESUMED)]);
        assert!(!hosted.is_open() && !hosted.lends() && !hosted.closing());
        assert_eq!(hosted.released(true), NONE, "nothing more to let go");
    }

    #[test]
    fn a_close_before_the_shell_took_the_surface_ends_the_overlay_at_once() {
        let mut hosted = Hosted::default();
        hosted.open();
        assert_eq!(hosted.close(RESUMED), [Step::Closed(RESUMED)]);
        assert!(!hosted.is_open());
        assert_eq!(hosted.take(), None, "and korrid hears no leave");
    }

    #[test]
    fn a_layer_that_never_showed_is_not_hidden() {
        let mut hosted = Hosted::default();
        hosted.open();
        hosted.take();
        assert_eq!(hosted.close(RESUMED), NONE);
        assert_eq!(hosted.released(false), [Step::Closed(RESUMED)]);
    }

    #[test]
    fn a_refused_leave_closes_its_own_opening_and_a_late_answer_changes_nothing() {
        let mut hosted = shown();
        assert_eq!(hosted.left(1, true), NONE);
        assert_eq!(hosted.left(1, false), [Step::Hide]);
        assert!(hosted.closing());
        assert_eq!(
            hosted.released(false),
            [Step::Closed("korrid refused the leave")]
        );

        hosted.open();
        assert_eq!(hosted.take(), Some(Step::Leave(2)));
        assert_eq!(
            hosted.left(1, false),
            NONE,
            "the first opening's answer comes late"
        );
        assert!(hosted.lends() && !hosted.closing());
    }

    #[test]
    fn a_shell_that_cannot_draw_ends_the_overlay() {
        let mut hosted = shown();
        assert_eq!(
            hosted.released(true),
            [
                Step::Hide,
                Step::Closed("the shell could not draw the overlay")
            ]
        );
        assert!(!hosted.is_open());
        let mut hosted = Hosted::default();
        hosted.open();
        hosted.take();
        assert_eq!(
            hosted.released(true),
            [Step::Closed("the shell could not draw the overlay")]
        );
    }

    #[test]
    fn a_shell_that_ends_before_it_took_the_surface_ends_the_overlay() {
        let mut hosted = Hosted::default();
        assert_eq!(hosted.released(true), NONE, "nothing was offered");
        hosted.open();
        assert_eq!(
            hosted.released(true),
            [Step::Closed("the shell did not take the overlay's Surface")]
        );
        assert!(!hosted.is_open());
    }
}
