# Slice D5 — the gameplay overlay, on the Android emulator

Run on 2026-11-14 at 16:49 to 16:52 (zao local time) with the tree of
commit `6a1d97e21` (D5 on D4's tip `c695948e1`), except one comment in
`run.py` that was corrected after the run. The README was being edited;
it is no input of the run. The APK and the tests that the commit builds
are the store paths below.

```sh
nix run .#viewer-android-proof -- --out /tmp/korri-d5/proof-9-3
```

No person took part. The tool built everything on zao. The emulator was
Android 12 (API 31), google_apis, x86_64, headless, 2 cores, `nice` 10, on
zao, with its sound to a WAV file on zao. The sender was stream-cadence's
private Sunshine fixture on aka, at aka's Tailscale address
`100.117.97.45`, port 51825, streaming 1280x720 at 60 fps with its
1000 Hz tone, logging each input packet that it received (`log_input`)
and passing none on. No game was live on aka at the end
(`live_games` empty); a `longshot` process of the `korri` user ran there
during the attempts and kept aka near the fixture's 85 °C limit. The
emulator was stopped at the end; no `qemu-system` process was left.

| Store path | |
| --- | --- |
| APK (unsigned) | `/nix/store/xlid54rqdvv8vkj52aravpfr7qxbaxld-korri-viewer-android-apk-0.1.0` |
| Refusal probe (unsigned, test only) | `/nix/store/00fgx992pnszmr7i0vkmvw9ldjssngkk-korri-viewer-refusal-probe-apk-0.1.0` |
| Linux viewer, for pairing | `/nix/store/k25v9fclvjqja6kn3651r56a26g5nmn9-korri-viewer-0.1.0` |
| Android tests (x86_64) | `/nix/store/6fndpjzmq972adhlfywfdf7wz6jqfyxj-korri-viewer-android-check` |

## What the proof did for D5

After D4's input and Android's audio-server restart (`result.json`,
`overlay.steps` and `overlay.pad_steps`):

1. `input keyevent KEYCODE_M`: the marker. The sender's packets after it
   are this step's.
2. A second "Korri test gamepad" through `hid -` (stdin). A held, then
   Guide (its 13th button, `BUTTON_MODE`) pressed and released with A
   still held.
3. With the overlay open: two screenshots a second apart
   (`overlay-open-1.png`, `-2`), Android's mixer twice
   (`audioflinger-overlay-open-1.txt`, `-2`), SurfaceFlinger's layers
   (`surfaceflinger-layers-overlay.txt`, `surfaceflinger-overlay-open.txt`);
   then A released, B pressed, the left stick fully right,
   `input keyevent KEYCODE_H KEYCODE_E KEYCODE_L KEYCODE_D`,
   `input tap 1680 270` (the picture at 0.75, 0.25) and
   `input mouse tap 1680 810`.
4. Guide again: the overlay closed. Two screenshots (`overlay-closed-1.png`,
   `-2`). Then X held 0.3 s, `input keyevent KEYCODE_Z` and
   `input tap 720 270` (the picture at 0.25, 0.25).
5. `input keyevent KEYCODE_BACK`: the overlay opened; a screenshot
   (`overlay-back.png`); Back again closed it. The pad went.
6. Home: the app left the screen, which ended the stream.

## Checks

67 of 68 passed. The one that failed is D4's, and D5 does not touch it
(below). The D5 checks:

| Check | Evidence |
| --- | --- |
| The overlay layer exists | `surfaceflinger-layers-overlay.txt`: `korri-viewer-overlay#0` |
| Guide opens it, and the input is held | `logcat.txt`: `overlay: open on Guide; the stream's input is held`, then from the main thread `overlay: layer shown: posted 0.5 ms before, applied in 1.03 ms` |
| The video moves under it | `overlay-open-1.png`, `-2`: frame IDs 6308, 6438, after 4768 to 4985 in `after-input-*.png` |
| The stand-in shows | The 99th percentile of the gray of the picture's part of the window's lower half: 63 in both open shots and in `overlay-back.png`, at most 90 allowed |
| It is gone after Guide closes it | 254 in `overlay-closed-1.png`, `-2`, at least 150 wanted |
| The video moves after it closes | IDs 6835, 6965 |
| Back opens it over the moving video | `overlay-back.png`: ID 7190; `overlay: open on Back`, `overlay: closed on Back after 3.0 s` |
| The sound plays while it is open | The app's track in Android's mixer advanced 345,995 frames in 7.6 s (45,500 a second of 48,000); its last power row, 0.5 s before the second look, was the tone at -41.7 to -48.6 dB |
| The overlay took the input | `overlay: closed on Guide after 8.6 s; it took 12 keys and 6 motion events` |
| The held keys never arrived | `sunshine-input.json` after the marker: the keyboard packets are Z down and Z up only; no H, E, L or D |
| The held pad input never arrived | No controller packet with B (00002000) or the left stick at 32766 after the marker |
| The host saw A let go | The controller packets after the marker: 00001000 (A, with the heartbeat and Android's repeats) 14 times, then 00000000 (the hold's let-go) before 00004000 (X, after the close) |
| The pad was silent while it was open | The longest gap between controller packets, 8,822 ms, follows that let-go: the overlay was open 8.6 s and the heartbeat sent nothing. The Back opening shows a 3,050 ms gap for its 3.0 s |
| The held touch and mouse never arrived | No touch at (0.75, 0.25), no absolute mouse packet after the marker |
| The touch after it closed arrived | A touch down (01) and up (02) at (0.25, 0.25) |
| The app finished after Home | `activities-after-home.txt` holds no `com.simonwjackson.korri.viewer` |

The touches after the marker also show the hold's cancels: a cancel of
every touch (07) when the overlay opened on Guide, because D4's touches
had gone before, and again when it opened on Back, after the tap. The
app's report: `input: ended: 14 keys, 97 controller states, 2 arrivals,
2 mouse, 123 touches sent; 44 held; 0 unsupported, 0 outside the stream,
0 failed`.

The D1 to D4 checks held with the overlay: the tone played (63.9 s at
1000 Hz in the emulator's WAV), the sound went on after the audio server
restarted (67.2 s played, 36.4 s before the server died), the frame
record was written (4,521 rows, 3,772 handed off, IDs rising with no
repeat in this run), and the input gate closed before the core stopped.

**The check that failed:** `sender_pad_heartbeat_repeats_every_250_ms`
(D4's). The D4 pad's release after it went and its four repeats arrived
67, 232, 251 and 250 ms apart; the check wants 250 ± 50 ms for each. The
first gap is not the heartbeat's: the app notices a removal on its
twice-a-second device check, and the heartbeat's next repeat follows its
own 250 ms clock, so the first gap can be anything up to 250 ms. In the
D5 runs it was 265, 254, 59 and 67 ms; D4's record had 252. This part of
the run is D4's, before D5's first step.

## The overlay's cost on the emulator

The decoder asked for an IDR frame 0.43 times a second while the overlay
was open (5 in 11.5 s) and 0.03 times a second while it was not (2 in
68.4 s). SurfaceFlinger's count of the composer's missed frames rose by
142 in the 8.6 s that the overlay was open, and by 55 in the 5.5 s after.
The composer took all three layers as `DEVICE` (the window, the video's
`bbq-wrapper`, the overlay's `bbq-wrapper`). This emulator composes on
the host in software (`-gpu swiftshader_indirect`), so a second
full-window layer costs CPU on every frame there; what a phone's composer
does is the spike's open risk (step H).

## The runs before this one

Not kept: their records are on zao in `/tmp/korri-d5/`.

| Run | Code | Result |
| --- | --- | --- |
| `proof-1` | D5 on D4's first tip | The overlay opened, held the input and closed. Then the sender's stream stopped 14 s into the overlay, and Sunshine ended the session 9.4 s later; the proof kept no Sunshine log then, so the cause is not known. The proof now keeps it on a failure |
| `proof-2` | the same | Every D5 check passed. Three D2 to D4 checks failed under load (zao's load average was 24): the sound after the restart, the frame record's IDs, D4's heartbeat |
| `proof-4`, `proof-8-1` | rebased on `c695948e1` | Every D5 check passed; the same three failed again. In `proof-4` the decoder's IDR requests were 1.55 a second with the overlay open against 0.26 with it closed, and the frame record's three repeated IDs all came while it was open, 0.2 to 0.4 s after an IDR request: the sender encoded one picture twice. The proof now lets an ID repeat but not go back, and opens the overlay after the audio-server restart, so that the sound check measures the restart and the overlay check the overlay |
| `proof-3`, `-5`, `-6`, `-7`, `-9-1`, `-9-2` | | Did not start: another emulator ran on the same AVD (`proof-3`), or aka was hot (85 °C or more) |

## The shell's renderer in the overlay layer

`slint-surfacewindow.png` and `slint-surfacewindow-logcat.txt` come from a
throwaway check on the same emulator image, not kept in the repository:
a NativeActivity that made the overlay layer as `android/src/layer.rs`
makes it, over its own window filled blue, and gave the layer's Surface
to the shell's vendored `SurfaceWindow::from_surface` (Skia over GL). A
`.slint` component (a dim, a rounded panel, a line of text) rendered
through `slint-interpreter` under a `slint::platform::Platform` that
makes no window of its own. `SurfaceWindow::from_surface` took the
2400x1080 Surface, the first frame came 698 ms after `show`, the layer
showed it see-through over the blue, and `release()` returned.

## Not proven here

- A real phone, the arm64 library, and a phone's composer with two
  layers (step H).
- korrid's overlay: no korrid runs in this app (`../../README.md`).
- A fold or turn while the overlay is open: the window had its size when
  the layers were made, so the overlay buffer's resize never ran.
- The Start quick menu and the hold of a mouse button, a key with
  modifiers and an absolute-mouse touch: the host tests only.
