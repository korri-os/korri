---
status: accepted
---

# The gameplay overlay never freezes the game; Android goes first

**Owner decisions, 2026-11-08.** Korri needs an overlay over a running game,
as Steam and Xbox have; without it Korri "doesn't feel like a product". On
Linux, Home froze the game with the cgroup freezer, dimmed its window and
raised the shell (verified on the RG353M, 2026-11-04). On Android in Korri
mode, Home froze the game on the holder and returned to the Korri screen.
Moonlight's own see-through overlay (`overlay.html`) opened only outside Korri
mode, had fixed buttons and no data from korrid.

## Decision

- Leave no longer freezes the game. The game keeps running, its audio keeps
  playing, and Korri holds all of its input until the person resumes.
- Korri performs no pause of its own. When the game's plugin supports a pause,
  the gameplay overlay shows it as a plugin control.
- The active surface package (Pico, Zone) draws the gameplay overlay on every
  platform, from korrid's `GameplayOverlay` model. On Android it is drawn
  see-through over the live stream, in a second Slint view inside the Game
  activity. Spike #375 showed that a see-through shell window on top of Game
  also works, but it pauses Game, makes the audio burst each time the overlay
  opens or closes, and takes about 100 ms to resume, against about 20 ms.
  `overlay.html` is not reworked into a second overlay UI.
- The overlay offers Resume, End, Disconnect, Open Korri and the plugin
  controls. The device that started a game on another device may End it. After that
  device disconnects, any federation device that can stream the game may End
  or change it, so a server-only device such as aka never holds a game that
  nobody can end.
- A status strip shows only while the overlay is open: time, battery, stream
  health and game name.
- Stream controls that Moonlight had (screen fit, keyboard, mouse mode and the
  rest) become product features. Each stream viewer declares the controls it
  supports, and the overlay shows them. A change lasts for that stream only
  until the settings cascade supports it. Their model is extracted from the
  Android code, not invented.
- Android goes first, in this order: a spike of see-through Slint over the
  Game activity; Home opens the surface overlay over the running stream; plugin
  controls with pause; the status strip; stream controls. Linux keeps its
  freeze until Android lands, then follows the same rules.

## Consequences

Until Linux follows, Linux and Android behave differently on Home. Freeze-based
session code (`session_freezer_resolved`, `SessionLeave` mapping to
`session_freeze`) must stop running for Leave on Android; it stays for Linux.

Later, Android will launch games as other apps through intents, and the
gameplay overlay must work over those too (owner, 2026-11-08). A view inside
the Game activity cannot cover another app. So the Rust side draws the surface
overlay into any Android `Surface` it is given, and does not depend on Game.
Today Game hosts that Surface. For intent launches, a Korri-owned window over
other apps will host it. Only the host of the Surface and the way Home is
caught differ between the two.

Guide opens the overlay only while the app in front is an active game that
Korri launched. In any other app, Guide brings up Korri instead (owner,
2026-11-10, #382). RetroArch's own menu is reached only through the overlay's
plugin control; no in-game controller chord opens it (owner, 2026-11-10,
#402).
