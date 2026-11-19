# #544: the viewer notices a sender that stops

The #524 resolution spike found a Rust viewer that showed a frozen picture
for 36 s after Sunshine aborted, with no log line and no exit
(`docs/research/2026-11-19-live-resolution-games-spike.md` on branch
`spike/524-live-resolution-games`, section 3). This record proves the fix
on aka, with no person.

## What the viewer does

`src/link_watch.rs`. The stream loop reads nightjar-core's counts each
100 ms: the video datagrams that reached the video socket, and the sound
packets that the audio receiver gave out. When the video and the sound have
both been silent for 1.5 s (the video alone in a stream with no sound), the
viewer:

1. writes `link: no video for N ms and no sound for M ms: the sender or the
   link to it is gone`;
2. sends the treaty's `LinkLost` event to its controller, with the
   CLOCK_MONOTONIC times of the last video packet, the last sound packet and
   the detection;
3. ends the stream with an error: `the stream ended: link: …`. With a
   lifecycle, `Status(Stopped)` follows the event.

The C core (`-networkcore c`) is not watched: it counts nothing for the
watch.

## Why 1.5 s

| What a running stream sends | Longest gap | Source |
| --- | --- | --- |
| Video, static picture, Sunshine's own floor | `1000 / (fps / 2)` ms: 33 ms at 60 fps, 200 ms at 10 fps | Sunshine `src/video.cpp`, `encode_run`: `minimum_fps_target` is `framerate / 2` unless configured, and the encoder waits at most `max_frametime` for a new image before it encodes the last one again (read in source; inferred for aka's product capture) |
| Video, static picture, this fixture | 17.6 ms and 38.2 ms; 59.2 and 58.6 frames/s | runs 1 and 2 below (verified) |
| Sound | 5 ms packets, silence included: 2000 packets in 10 s | Sunshine `rtsp.cpp` sets `x-nv-aqos.packetDuration` to 5; nightjar-core's "Audio arrival over 10 s: 2000 packets" in run 2's `viewer.log` (verified) |
| Video while a loaded host makes a new encoder | 0.1 to 3 s, the sound on | #524's resolution spike, finding 6 (verified there) |

The watch needs both silent, so a video pause with sound is never a loss.
1.5 s is 7.5 times the longest gap of a 10 fps static picture. With the
loop's 100 ms reads, the report comes at most 1.6 s after the last packet:
inside #544's 2 s.

## Runs on aka, 2026-11-19

```sh
# On aka, as root, in a checkout of this branch. The viewer was built there.
nix run .#viewer-dead-sender-proof -- --build-store auto \
  --viewer /nix/store/pgxc0hiam0bri1sgayanqj1x1kmq5im4-korri-viewer-nightjar-0.1.0 \
  --sunshine /nix/store/ap898nhjj347ysnaf3jpp4ynzndh88a6-sunshine-korri-2026.914.233613-korri \
  [--moving 15 --static 30]
```

`run.py`'s docstring has the steps: a moving picture, then the fixture's
picture source stops (a static output), then the fixture's Sunshine gets
SIGSTOP. The viewer is `korri-viewer-nightjar` at commit `f119b5615`,
1280x720 at 60 fps, the SDL platform on a private Xvfb, with the tone.

| | Run 1 | Run 2 |
| --- | --- | --- |
| Moving, then static | 8 s, 10 s | 15 s, 30 s |
| Link loss while the sender sent | none | none |
| Longest video gap, moving / static (frame record) | 17.9 / 17.6 ms | 19.1 / 38.2 ms |
| Longest silence, the viewer's reads (to 100 ms) | video 41 ms, sound 100 ms | video 45 ms, sound 45 ms |
| SIGSTOP to the viewer's report | 1.596 s | 1.505 s |
| Last video packet to the report | 1.606 s | 1.511 s |
| The viewer's line | no video for 1603 ms and no sound for 1503 ms | no video for 1502 ms and no sound for 1502 ms |
| SIGSTOP to exit | 3.926 s | 3.820 s |
| Exit status | 1, `the stream ended: link: …` | 1, the same |
| nightjar-core at the stop | `connection_status: Some(Okay)`, `input_contact_lost: false` | the same |
| Fixture restore check | pass | pass |

All 10 checks passed in both runs (`result.json`). nightjar-core's own state
still said `Okay` at the stop: the same state as in the spike, so only the
watch saw the loss.

Records: `aka-2026-11-19/run-1/` and `run-2/` (`result.json`, the sender's
facts, the pairing's log, and the viewer's `viewer.log` and `times.csv`).

Not proven here: a sender that crashes and restarts (SIGSTOP holds the
process; the spike's crash and systemd's restart were not reproduced), the
lifecycle path to korrid on a device (korrid's part has its own tests:
`linux_viewer/socket_tests.rs` and `upstreams.rs`), the Android app, and a
loaded host. The exit took about 2.3 s after the report: that is the core's
stop toward a silent host, not measured further.
