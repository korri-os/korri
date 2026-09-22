# Connect the gameplay overlay to the live session

Status: resolved
Blocked by: None

## What to build

Make Home open a real gameplay overlay for the one live session. Let the player leave, return, end, and run only the actions that korrid reports for that exact launch.

## Acceptance criteria

- [ ] One Home tap opens the gameplay overlay and freezes the exact live-session launch. The frozen game uses no CPU, draws nothing new, and receives no input.
- [ ] Korri owns all input while the overlay or full portal is in front. The full portal opens from the overlay, not directly from Home.
- [ ] A second Home tap or the overlay return control thaws the same launch and raises its window.
- [ ] Portal end asks once before it stops the exact launch. The kill chord stops it immediately without confirmation.
- [ ] Every freeze, thaw, stop, and plugin action carries the exact launch id. A stale launch is refused and no command retargets to a newer launch.
- [ ] `FocusFailed` leaves the game running, keeps return available, and shows a short failure notice. Korri does not refreeze or end the session.
- [ ] The overlay shows return, end, open Korri, and only the actions that korrid lists for the exact session. Unsupported actions are not greyed out or invented.
- [ ] A refused or unanswered action shows a short notice that names the reason, leaves the session untouched, and keeps the overlay open.
- [ ] The portal overlay uses live korrid state instead of the in-memory fixture, and inputd's Home/system-panel path reaches it.
- [ ] korrid state-machine tests cover freeze, thaw, stop, stale identity, focus failure, and recovery after a korrid restart.
- [ ] Queued-input discard is not implemented here; ticket 12 owns it.
