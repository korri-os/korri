# #495 — black bars beside the picture

Run on 2026-11-16 at 04:17 to 04:21 (aka's clock, UTC), on aka as root, from
a copy of this branch at `4f37bf05d` (the fix) with its `proof/play.py`. The
fixture APK was built on aka from that tree:
`/nix/store/kgq0ydgk15k2ij5r04cds7wpfa0qw20w-korri-android-app-fixture-apk-0.1.0`.

```sh
# in that copy, with HOME and XDG_STATE_HOME under /var/tmp/fix-495:
nix run .#android-app-play-proof -- \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5690 --adb-port 5191 --out /var/tmp/fix-495/proof \
  --apk /nix/store/kgq0ydgk15k2ij5r04cds7wpfa0qw20w-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

No person took part. A poller started the run when aka's k10temp read
58 °C, with no emulator, no fixture and no outside client of the product's
korri-sunshine; k10temp read 87 °C at the end (96 °C at its highest). It
read korri-sunshine's TCP ports (47984, 47989, 47990, 48010) every 5 s, 43
times: no outside client connected. The emulator was Android 12 (API 31),
google_apis, x86_64, headless, 2 cores, console port 5690, its own adb
server on 5191, its data wiped; it and its adb server were stopped at the
end, with no process left. All 73 checks passed (`result.json`), D10's, D11's
and D5b's included; the sender's restore check passed.

## The cause

The video layer is the picture's size: `(240, 0)-(2160, 1080)` in the
emulator's 2400x1080 window for a 1280x720 stream. Until this fix it lay
straight above the window's own buffer, and SurfaceFlinger's tree held
nothing else under the window but the overlay (D5b's
`surfaceflinger-layers.txt`: `korri-viewer-video` at z 0,
`korri-viewer-overlay` at z 1). The shell draws its library into that
window and goes on drawing it during the stream (D5b's log: `shell rendered
… page=home` under the overlay, `page=detail` after it). So the bars showed
the library, and so did the overlay's panel, which is 94 % opaque
(Velvet's `velvet-94`). The viewer's own app fills its window black
(`layer::fill_black`); the Korri app cannot, because the shell's renderer
owns its window.

Read with this proof's checks, D5b's screenshots fail: the left bar of
`stream-1.png` is black for 71 % of its pixels (the library's title, Play
and Ready), the right bar for 17 % (the library's gradient, up to 36 of
255), and under the panel 78 % to 79 % of its background is the panel's
colour over black.

## The bars

Each screenshot below the status bar (the top 100 rows, which the emulator
keeps drawn), outside the picture and 2 pixels from its edge:

| Screenshots | Picture | Overlay | Outside the picture |
| --- | --- | --- | --- |
| `stream-1.png` to `-4` | fitted, after the window turned from 1080x2400 | closed | both bars: every pixel 0 |
| `overlay-1.png` to `-3`, `overlay-controls.png`, `overlay-fit.png` | fitted | open | right bar: every pixel 0. Left bar, under the panel: 99.8 % to 99.9 % of what the panel does not draw on is (13, 13, 17), the panel over black |
| `overlay-fill.png` | Crop to fill | open | nothing |
| `overlay-fit-again.png` | fitted again | open | as `overlay-fit.png` |
| `after-overlay-1.png` to `-3` | fitted | closed by Back | both bars: every pixel 0 |
| `controls-fit.png`, `controls-fit-again.png` | fitted, before and after korrid's Crop to fill | closed | both bars: every pixel 0 |
| `controls-fill.png` | Crop to fill | closed | nothing |
| `again-1.png` to `-3` | fitted, the stream started again after Home (a second turn) | closed | both bars: every pixel 0 |

The record keeps `stream-1.png`, `overlay-controls.png`, `overlay-fill.png`,
`overlay-fit-again.png`, `after-overlay-3.png`, `controls-fill.png`,
`controls-fit-again.png` and `again-3.png`.

The backdrop showed with the first picture both times: the decoder gave
1280x720 at 04:19:42.025, and the main thread logged `picture: the black
backdrop shows under the video, with its first picture` at 04:19:42.046
(`logcat-app.txt`); again at 04:20:55.865 after the return.

SurfaceFlinger's tree (`surfaceflinger-layers.txt`): `korri-viewer-backdrop`
at z 0, `korri-viewer-video` at z 1, `korri-viewer-overlay` at z 2. The layer
that holds the backdrop's buffer (`bbq-wrapper`, flags 0x102, `isOpaque=1`)
is visible in exactly `[0, 0, 240, 1080]` and `[2160, 0, 2400, 1080]`, the
video's in `[240, 0, 2160, 1080]`, and the window's own buffer nowhere: the
compositor leaves the shell's library out. With Crop to fill the video's is
visible in the whole window and the backdrop's nowhere.

## Earlier runs

- 03:51 to 03:55, the APK of an earlier version of the fix, which made the
  backdrop opaque with `Transaction.setOpaque` after building it: 72 of 73
  checks passed, every bar black. The SurfaceFlinger check failed: on API 31
  the layer that holds a layer's buffers takes its opacity from the build, so
  the backdrop's `bbq-wrapper` was not opaque (flags 0x100, `isOpaque=0`) and
  the window's own buffer stayed visible in the bars under it
  (`surfaceflinger-first-run.txt`). The fix now builds the layer opaque
  (`SurfaceControl.Builder.setOpaque`), and the check reads the
  `bbq-wrapper`; its first version read the parent layer's `isOpaque`, which
  is 0 for a layer without a buffer.
- 04:09, the APK of this record: it started at 62 °C, and the sender
  fixture's heat guard refused to start at 91.5 °C, 7 s after the emulator
  booted. Nothing was checked. The poller then waited for 58 °C.

Not proven here: a real phone, its camera cutout, a fold or any other size
change in the middle of the stream beyond the first turn to landscape;
whether a phone's hardware composer composes the stretched backdrop itself
(on the emulator its buffer of 1080x2400 is stretched to 2400x1080).
