# #494 — the overlay layer keeps out of the camera cutout

Run on 2026-11-16 at 06:47 to 06:51 (aka's clock, UTC), on aka as root, from
a copy of this branch with `proof/play.py` as `d56c73a31` has it. The fixture
APK was built on aka from the fix's tree (`8617541a9`, the same Rust sources):
`/nix/store/dqhxv18jvxigik21ypawsxchl5j9dmr9-korri-android-app-fixture-apk-0.1.0`.

```sh
# in that copy, with HOME, XDG_STATE_HOME and XDG_CACHE_HOME under /var/tmp/fix-494:
nix run .#android-app-play-proof -- \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5694 --adb-port 5194 --out /var/tmp/fix-494/proof-2 \
  --apk /nix/store/dqhxv18jvxigik21ypawsxchl5j9dmr9-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

`--cutout` was left at its default, `pixel_5`. No person took part. A poller
started the run when aka's k10temp read 58 °C, with no emulator, no proof
fixture and no live stream of the product's korri-sunshine. It then read
korri-sunshine every 5 s, 44 times (`korri-sunshine-watch.log`): no TCP
session on 47989, 47990 or 48010, and no UDP flow on 47998 to 48000 or 48002
to 48010 in conntrack. An idle HTTPS socket from haku to 47984 had been open
before the run (not a stream); none was open during it. k10temp read 95 °C at
its highest and 87 °C at the end. The emulator was Android 12 (API 31),
google_apis, x86_64, headless, 2 cores, console port 5694, its own adb server
on 5194, its data wiped. The emulator and its adb server were stopped at the
end, with no process left. All 96 checks passed (`result.json`): D10's, D11's,
D5b's, #495's, and #494's.

## The cutout

The proof enabled the image's emulated Pixel 5 before it installed the app:
`com.android.internal.emulation.pixel_5` and
`com.android.systemui.emulation.pixel_5`. In portrait the window manager
reported a hole in the top left corner: safe insets top 136, bounding
rectangle (0, 0)-(136, 136) (`display-cutout-enabled.txt`). SystemUI's crash
buffer held nothing 10 s later.

## The overlay layer in both turns

The stream turned the activity to landscape (90°, `SENSOR_LANDSCAPE`). Guide
opened the overlay, and the shell drew it. Then the emulator's accelerometer
turned the display by 180° to 270° (-9.81 m/s² on x; the rotation answered in
0.6 s), and back to 90° (9.81 m/s²; 1.2 s). The window stayed 2400x1080 all
the time. In each turn the window manager's cutout (`display-overlay-*.txt`)
and SurfaceFlinger's tree (`surfaceflinger-layers.txt`) read so:

| Turn | Cutout (safe insets; rectangle) | Window less the insets | `korri-viewer-overlay` | Its buffers (`bbq-wrapper`) | The app's log |
| --- | --- | --- | --- | --- | --- |
| 90°, first | left 136; (0, 944)-(136, 1080) | (136, 0)-(2400, 1080) | pos (136, 0), crop [0, 0, 2264, 1080], shown | visible in [136, 0, 2400, 1080]; size 2264x1080; buffer 2264x1080 | (136, 0)-(2400, 1080), left 136 |
| 270°, turned by 180° | right 136; (2264, 0)-(2400, 136) | (0, 0)-(2264, 1080) | pos (0, 0), crop [0, 0, 2264, 1080], shown | visible in [0, 0, 2264, 1080]; size 2264x1080; buffer 2264x1080 | (0, 0)-(2264, 1080), right 136, at 06:49:50.748 |
| 90°, back | left 136; (0, 944)-(136, 1080) | (136, 0)-(2400, 1080) | pos (136, 0), crop [0, 0, 2264, 1080], shown | visible in [136, 0, 2400, 1080]; size 2264x1080; buffer 2264x1080 | (136, 0)-(2400, 1080), left 136, at 06:49:55.195 |

So in each turn the layer stands exactly in the window less the cutout's
insets, as Artemis's overlay view stands with the insets as its margins
(`KorriStreamCutout.keepOutOfCutout`). It meets none of the cutout's
rectangles. The turn by 180° kept the window's size and took the cutout to
the other side; the layer followed it, and followed it back.

The app placed the layer 5 times (`logcat-app.txt`, `overlay: layer placed
at`): (0, 136)-(1080, 2400) in the portrait window 1080x2400 when it made the
layers, with the cutout's top inset 136; (136, 0)-(2400, 1080) after the turn
to landscape; the two half turns above; and (136, 0)-(2400, 1080) when the
stream started again after Home.

The video and the backdrop did not move. In all three dumps the video layer
shows the whole 1280x720 stream at (240, 0)-(2160, 1080), and the backdrop's
buffer layer is visible exactly in the bars, [0, 0, 240, 1080] and
[2160, 0, 2400, 1080], the cutout's side included. The window's own buffer,
the shell's library, is visible nowhere.

The bars beside the open overlay (`overlay-cutout-back.png`,
`overlay-cutout-turned.png`, below the top 100 rows that the emulator's
status bar draws over, 2 px from each edge): at 90° the columns left of the
layer are black (every pixel 0), and the bar under the panel, from the
layer's edge to the picture, is the panel over black, (13, 13, 17). At 270°
the left bar is the panel over black, the right bar is black, and so are the
columns right of the layer.

## Earlier runs

- 05:36 to 05:41, the same APK with the `corner` cutout alone
  (`com.android.internal.display.cutout.emulation.corner`: top inset 126, a
  rectangle in the top right corner). SystemUI crashed 814 times in a loop,
  from the app's start on: `ScreenDecorations.setupDecorations` gives
  `PrivacyDotViewController` a null bottom-left dot, because a cutout on one
  edge with no rounded corners makes no bottom overlay
  (`corner-run-systemui-crash.txt`). The loop took the app's window away
  before its stream started, so no overlay was checked. The proof now uses a
  phone's overlays, which also give rounded corners, and stops at once when
  SystemUI crashes after the cutout is enabled.
- Between the two, an emulator session with no app read each emulated
  cutout and the accelerometer: 9.81 m/s² on x turns the display to 90°,
  -9.81 to 270°. Its other readings were not consistent and are not used.

## Not proven here

- A real phone: the Galaxy Z Fold 8 at D12, its folding, and its hardware
  composer.
- The touch offset on a device. Crop to fill was chosen with a tap at
  (285, 879) in the window, 149 px into the layer, and it took effect. Its
  row is wider than the layer's 136 px offset, so the tap would also have
  hit it without the offset. The offset itself is proven by the host test
  `a_touch_reaches_the_overlay_from_the_layers_corner`.
- Which main event made the app follow the turn by 180°. The window kept
  its size, so `Layers::follow` did not move the layer; one of the events
  that `layer::may_move_the_cutout` names did, and the app does not log
  which. By Android's rotation path it is `RedrawNeeded` (inferred, not
  checked).
- The layer's place in portrait in SurfaceFlinger: a stream turns the
  activity to its own shape, so the portrait place is only in the log.
- The viewer's own app, whose D5 stand-in gets the same rule: not run.
