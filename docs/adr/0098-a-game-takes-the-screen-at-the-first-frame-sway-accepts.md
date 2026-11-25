# A game takes the screen at the first frame Sway accepts

**Status:** accepted. Owner decisions, 2026-11-19 to 2026-11-24, issue #510.

When a game started on aka, Korri revealed the stream as soon as the game's
unit started. The person then saw 3 to 4 s of aka's empty black desktop before
the game had a window (launch `ee96cbbe…`, #510). Locally, korrid noticed the
game's window by checking about once a second.

## Decision

- A game **takes the screen** at its first frame: the first picture that the
  compositor accepts from any window of that launch, a launcher included, once
  per launch (`CONTEXT.md`). Korri reports that the game drew, not what it drew.
- korrid on the device that runs the game learns that moment from Sway's
  window events, through an IPC subscription, not from polling and not from
  inside the game's unit. The gamescope plugin always nests gamescope on Sway
  (`--backend wayland`), and nested gamescope maps its Sway window at its first
  composite. So one event covers every runner. korrid makes the window full
  screen the moment Sway reports it, and a rule in Korri's Sway config keeps a
  game window off screen until korrid places it, so it never shows beside the
  shell.
- The session status records the moment in `initial_handoff`: `Observed` now
  means the first frame. There is no second field and no old meaning.
- Until then, Korri's starting screen stays in front and the launch can be
  cancelled. On the device that runs the game, korrid mutes the game's sound and
  drops its controller presses, the device's own and the stream's alike. One
  place covers local play and every stream. Neither is replayed later.
- A stream's viewer connects at once and stays hidden. It appears when both are
  true: the device that runs the game says the game took the screen, and the
  viewer has shown its first frame. The Rust viewer reports its first frame to
  korrid through its lifecycle treaty. The target is about 100 ms from the
  first frame to the reveal.
- The device that runs the game sends session status changes to the viewer's
  korrid as they happen, on a session-events stream over the iroh connection
  they already share (ADR 0095). The viewer stops polling for that session.
- If no first frame comes within 30 s, Korri shows the game anyway. If the
  game's unit exits first, Korri shows an error at once.
- This applies to Linux devices. The Android app keeps its current reveal at
  the first decoded frame; Android work is parked.

## Considered options

- **Pixels:** sampling the screen for a non-black frame. The owner rejected it
  as too weak to be the product signal.
- **gamescope's own property:** `GAMESCOPE_FOCUSED_WINDOW` moves only to a
  window with a done commit. It matched Sway's `new` event within 18 ms in all
  28 pairs measured. Reading it needs root to enter the unit's mount namespace
  and the unit's uid, which weakens the sandbox for no gain in timing.
- **A ready report from each runner:** more accurate for games that draw black
  while they load, but every runner needs its own work. A runner that draws
  black for long, such as Mario Kart World in one crashed launch, is fixed in
  that runner's own project.

## Evidence

Spike on aka, 2026-11-19, 40 launches: the signal never came after the first
non-black frame, worst case 3 ms behind the 33–60 ms capture step. It also
fires on Steam dialogs, Proton error dialogs and black fade-ins. The slowest
signal came after 17.4 s. Raw data on zao at
`/home/simonwjackson/korri-510-spike/`; summary in #510.

## Linux stream implementation (slice 2)

The viewer keeps mapping and presenting its X11 window. The existing Sway
`for_window [all] move scratchpad` and `no_focus [all]` rules keep it off
screen. korrid moves that exact unit's window out of scratchpad and makes it
full screen only at reveal. Leaving presentation running avoids making the
first-frame signal depend on mapping or visibility, and covers the viewer's
picture paths without a second window-control protocol. The viewer device
must have slice 1's Sway rules before this build runs.

`app.session.events` subscribes to one launch on `/peer-session-events`.
Each newline carries a signed, encrypted `SessionStatus` response bound to
that subscription's request. It uses a stream of the launch's existing iroh
connection. The holder subscribes to local session changes before reading
the initial snapshot. It checks the same stream scope as session status,
checks current authority again before each record, and never follows a
replacement launch. A slow reader holds one pending record, not a queue.

The Linux lifecycle treaty adds `FirstFrame`. The presenter sends it from
the same successful presentation that records `start: first frame shown`.
`Start` no longer focuses a viewer. The viewer device keeps the surface's
starting state until both facts hold and compositor focus is confirmed.
At 30 s it waives both picture conditions, but still requires the exact live
launch and current user intent. Holder observations publish before taking
focus authority, so a status reader cannot block the producer while waiting
for its first observation. Android keeps its existing reveal path.
