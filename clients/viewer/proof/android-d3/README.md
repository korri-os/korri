# Slice D3 — the sound, on the Android emulator

Run on 2026-11-14 at 13:40 to 13:42 (zao local time) from commit
`a381dd29fd3c7ce9379fa551c72edf4723a53df9`, with a clean tree:

```sh
nix run .#viewer-android-proof -- --out /tmp/korri-d3/proof-3
```

No person took part. The tool built everything on zao. The emulator was
Android 12 (API 31), google_apis, x86_64, headless, 2 cores, `nice` 10, on
zao, with its sound card on: QEMU's `wav` driver wrote everything that
Android played to a file on zao, so nothing reached zao's speakers. The
sender was stream-cadence's private Sunshine fixture on aka, at aka's
Tailscale address `100.117.97.45`, port 37353, streaming 1280x720 at 60 fps
and stereo sound. The fixture's tone played 1000 Hz into its private
PulseAudio. A game was live on aka (`live_games` in `result.json`); the
fixture runs beside the product and does not touch it, and no product unit
changed. The emulator was stopped at the end; no `qemu-system` process was
left.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/dxb4dmrbcza9j8w30q8mq5i0k0db54sb-korri-viewer-android-apk-0.1.0` |
| Refusal probe (unsigned, test only) | `/nix/store/00fgx992pnszmr7i0vkmvw9ldjssngkk-korri-viewer-refusal-probe-apk-0.1.0` |
| Linux viewer, for pairing | `/nix/store/lm5bw283pqs2v1hhdhhq2lf80xwn9apy-korri-viewer-0.1.0` |
| Android tests (x86_64) | `/nix/store/3hhjapincwd2d7h5k5nzrhll7h04qbyd-korri-viewer-android-check` |

## Checks

All 35 passed (`result.json`), and the fixture's restore check passed.
The checks of D1 and D2 are the same as in `../android-d2/`. The new ones:

| Check | Evidence |
| --- | --- |
| The sender plays the tone | `sender-facts.json`: `tone` 1000 Hz on `korri-tone`. `sunshine.log`: Sunshine cannot make its virtual sinks (`Couldn't create virtual sink … Module initialization failed`), as the tone's server wants, and has no `Unable to initialize audio capture` |
| The Android tests pass, the sound's included | `android-test-lib.txt`: 51 tests, 15 of them `audio::` and `sound::`. `sound::tests::real_opus_plays_through_aaudio` plays real Opus packets of a 440 Hz tone through real AAudio |
| The sound's output | `logcat.txt`: `sound: output try 0 asked low-latency granted none sharing shared rate 48000 channels 2 burst 960 capacity 2132 buffer 1920 (asked 1920) packet 240 frames`. `logcat-sound.txt`: `AUDIO_OUTPUT_FLAG_FAST denied by server` |
| Android mixes the app's track | `audioflinger-while-streaming.txt`: one active track, client 2929 (the app's PID), 48000 Hz, stereo, PCM 16-bit, usage `e` (14, game). The output runs at 44100 Hz with `No FastMixer` |
| The emulator played the stream's tone | `result.json` `emulator_sound`: 21.3 s at 1000 Hz of 24.8 s of sound, median RMS 260.9 (the music stream's volume is -33 dB). `emulator-sound-1000hz.wav` is one second of it |
| The emulator played the test's tone | 2.0 s at 440 Hz |
| The sound played | `sound: ended: 2483 callbacks, 1054831 frames played, …`: 22.0 s |
| The sound went on after Android's audio server restarted | `result.json` `audio_server_restart`: PID 394, then 3952. `logcat-sound.txt`: the app's AudioTrack lost its track at 61.312 (`restoreTrack_l(17): dead IAudioTrack`) and had a new one at 64.434; AAudio raised no error, and the app opened no new stream. The app played 22.0 s of sound; the stream had run 15.2 s when the server died |
| The sound ended before Stopped | `logcat.txt`: `sound: stream closed` and `sound: ended:` at 77.784, the Stopped event at 77.871 |
| The picture still moves | `streaming-1.png` to `streaming-4.png`: frame IDs 3229, 3361, 3499, 3662. `times.csv`: 1835 rows, 1595 handed off, with rising IDs 2935 to 4785 |

## What the sound did

From `sound: ended:` in `logcat.txt`:

| | |
| --- | --- |
| Packets | 5985, none concealed, no Opus error |
| Dropped over 40 ms pending | 1584 (26.5 %) |
| Frames played | 1054831 (22.0 s) |
| Frames of silence | 200849 (4.2 s), in 389 short callbacks after the first sound |
| AAudio xruns | 6 |
| Streams opened again | 0 |

While Android's audio server was down, 61.3 s to 64.4 s, AAudio took no
sound, and every packet was dropped (about 200 a second): the 40 ms limit
held, and no backlog came after. Outside that time, the emulator's sound
card took about 43,900 frames a second of the stream's 48,000 (44,200 in
the run before this one, which did not stop the server); the rest is
dropped at the limit. Why the emulator takes less than real time is not
known: its mixer runs at 44.1 kHz and resamples the stream, and the tone
reads 1000 Hz in the file, so the pitch is right. These numbers say
nothing about a phone (ADR 0063).

## The runs before this one

Two earlier runs of the same proof passed all their checks: one from an
earlier version of these commits (only its log wording differed), and one
from `afcd6c980` (its checks without the audio server's restart: 33 of 33,
24.8 s at 1000 Hz, 921 of 5757 packets dropped). Before the restart step
joined the proof, a probe stopped the audio server while the 2 s Android
test played: AAudio raised no error, and `AAudioStream_getXRunCount`
waited about 4 s, until the server was back. That wait is why the README
says that a teardown in that moment waits for the server too. None of
these records is kept.

## Not proven here

- The low-latency path. The emulator refused it, so AAudio's standard path
  ran, with 20 ms bursts. A phone's low latency, MMAP, Bluetooth, its
  underruns and its latency: step H, on the Galaxy Z Fold3.
- AAudio's error callback and the new stream that follows it: the tests
  prove both over the scripted output; the emulator raised no AAudio
  error, also when its audio server died.
- Surround: the stream is stereo. The tests decode 5.1 and 7.1 over the
  scripted output only.
- The arm64 library on a phone.
- Input: slice D4.
