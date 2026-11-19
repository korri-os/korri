---
status: accepted
---

# Stream quality adapts in the viewing device's korrid, inside the person's limits

**Owner decision, 2026-11-19 (issue #524).** Today a stream keeps the
quality it starts with. The Android app fixes 60 fps and 10 Mbps. The PC
viewer takes korrid's display size. On a weak or changing link the stream
stutters or breaks, and the person can do nothing about it.

The design starts from the July 2026 controller plan on `legacy`
(`work/items/active/01KSXN94148T4616TA79KHQD9T-adaptive-stream-controller/plan.md`)
and from three spikes on 2026-11-19:

- `docs/research/2026-11-19-live-bitrate-fps-spike.md` (branch
  `spike/524-live-bitrate-fps`): live bitrate and frame rate on x86 and ARM
  Sunshine;
- `docs/research/2026-11-19-live-resolution-games-spike.md` (branch
  `spike/524-live-resolution-games`): live resolution with 4 real games.

## Decision

1. **The person states intent, not settings.** One slider sets what matters
   most. Optional fine-tuning sets limits on bitrate, frame rate and
   resolution, and limits on the result (latency, delivered frame rate).
   How the slider looks is not decided. Named presets are not in the first
   version.
2. **A limit is kept.** When the link cannot meet a limit, the controller
   keeps the limit and the overlay says that it cannot be met.
3. **Live levers.** The controller changes bitrate and frame rate. It
   changes resolution only rarely, as a last resort, and only the encode
   size. It never reshapes the game for quality. Reshaping stays for fold,
   unfold and portrait (ADR 0058). It does not change the size while the
   host is overloaded.
4. **Each viewer steers its own stream.** The loop runs in korrid on the
   viewing device. That korrid collects the viewer's stats and the host's
   facts, and decides. The host's korrid applies each change through
   Sunshine (the route of #476, patch 0046) or refuses it. The game's render
   shape still follows the shape owner (ADR 0058).
5. **What the person sees.** A short health mark in the gameplay overlay's
   status line. The full host and viewer stats in the overlay on demand.
6. **How long a choice lasts.** A change during play lasts for that session.
   Each viewing device has its own default. The data model for both is not
   decided; it is extracted from the first real producer and consumer.
7. **A dead link is survived.** The viewer holds the last frame, the game
   keeps running, the viewer reconnects, and quality ramps up again with
   care. This is in the first version.
8. **PC hosts first.** ARM hosts keep their fixed launch settings until
   #541 is solved and an iris gen2 device (Thor or Odin 2 Portal) is
   measured. RKMPP (RG353M) is measured before it joins.

## Why

- **Bitrate and frame rate change live on x86 (verified).** 50 of 52
  requests applied, with no reconnect and no decoder reopen. Each change
  sends one IDR. On VAAPI no frame is lost. On Vulkan, aka's encoder, each
  change stalls about 70 ms (#540).
- **An encode-only resize is short; a reshape is not (verified).** Across 24
  changes in 4 games, an encode-only change paused the picture 66 to 90 ms
  and lost no frame. A reshape paused it 193 to 234 ms, crashed Sunshine
  once under load (#543), and is refused for Steam games (#545).
- **ARM applies a change in place, but iris gen1 misses the target
  (verified).** On the Mini V2 every request applied with no IDR and no
  break. The stream stayed at 5 to 9 Mbps in CBR whatever the target was
  (#541).
- **The stats already meet on the viewing device.** The viewer measures the
  link and the decoder (nightjar). Each viewer needs its own stream, and the
  device that must reconnect after a dead link is the viewing device.

## Considered options

- **A preset ladder** (legacy task-067, before July). Rejected in July: a
  fixed ladder oscillates and does not use the link that is there.
- **The loop on the host's korrid.** It sees every viewer and the encoder,
  but every viewer's stats must travel to it, and it cannot act for a viewer
  whose link is dead.
- **The loop inside the viewer app.** It reacts faster, but it ties the logic
  to one viewer, and korrid must still carry the request.
- **Reshape the game for quality.** A re-rendered picture is sharper than a
  scaled one. Rejected for now because of its cost and faults (above).
- **Wait for ARM hosts.** Rejected: #541 may not be solvable, and handhelds
  as viewers still gain when the PC adapts.

## Consequences

- #544 (the viewer does not notice a dead sender) and #540 (the Vulkan stall)
  are fixed before the controller is built.
- The legacy private VAAPI bitrate path does not come back. The spikes
  showed that it is not needed.
- Sunshine carries a live-settings patch beside 0044 and 0046 (spike patch
  0047 on x86, 0048 and an FFmpeg patch on ARM). Each release builds it for
  x86 and ARM.
- Not yet checked: that Sunshine keeps one encoder per session, which
  per-viewer encode sizes need.
