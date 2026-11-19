# Back after typing opens the overlay (#491): the play proof on aka

In D10 a hardware letter key in the stream started an input method session
for a text editor (`../android-app-d10/logcat-app.txt`, line 191:
`LatinIme.onActivate`, `inputType=0x1`). The next Back did not open the
gameplay overlay, and the proof worked around it: it opened the overlay
before it sent letters. Inferred, not traced: the editor was Slint's text
view, which had the focus, and the input method took the Back.

The keyboard fix for #514 (`47164b2bd`) changed the focus: while a stream
plays, NativeActivity's own view holds it
(`korri_viewer::soft_keyboard::take_focus`). That view is no text editor
(`inputType=0x0`), and after a letter key the input method shows nothing.
This run proves that Back then opens the overlay, with a new step of the
play proof straight after the letter keys and the input method's dump. No
code of the app changed.

Run on 2026-11-19 at 14:37 to 14:42 UTC, on aka as root, from a copy of this
branch (`origin/main` `7565d2aea` with the #511 fix) and `proof/play.py` as
it is committed here but for one comment, on nightjar-core. The fixture APK was built on aka from
the same Rust sources:
`/nix/store/5gwfnd5k2l3qhy1pc34lc7akp69j9f77-korri-android-app-fixture-apk-0.1.0`.

```sh
# in that copy, with HOME, XDG_STATE_HOME and XDG_CACHE_HOME under /var/tmp/fix-511b/android-run:
nix run .#android-app-play-proof -- --build-store auto \
  --sunshine /nix/store/bvz43ydfridi8smy1gyqvzxk5v5xd3rg-sunshine-korri-2026.914.233613-korri \
  --port 5674 --adb-port 5174 --out /var/tmp/fix-511b/android-run/proof \
  --apk /nix/store/5gwfnd5k2l3qhy1pc34lc7akp69j9f77-korri-android-app-fixture-apk-0.1.0/korri-android-app-unsigned.apk
```

No person took part. The run held `/var/tmp/korri-turns.lock`, as a file
that names it and with `flock`, and removed it at the end. At the start aka
read a load of 0.24, k10temp 57 °C, and no Nix build, compiler, emulator,
stream or game; at the end 87 °C and no emulator (`host.txt`). The emulator
was Android 12 (API 31), google_apis, x86_64, headless; it stopped with no
process left.

**Result: all 153 checks passed** (`result.json`): the 151 of
`../android-app-overlay-mouse/` and these two.

| Step | What happened |
| --- | --- |
| The letter keys | `input keyevent` M, K, O, R and I: the sender's Sunshine received each, down and up (`sunshine-input.json`). The input method then served NativeActivity's view and showed nothing: `mInputShown=false` (`input-method-after-keys.txt`) |
| Back | 14:40:26.606, `overlay: open on Back; the stream's input is held`. korrid heard the leave, and the shell drew the overlay 0.33 s later (`logcat-app.txt`, lines 323 to 342) |
| Back again | 14:40:29.496, `overlay: closed on korrid resumed the stream after 2.9 s; it took 1 keys and 0 motion events`, and `viewer resume for korrid: … the stream is in front, its input goes on` |

The rest of the proof ran after this step and passed, Toggle keyboard among
it.

Not proven: a phone (the Fold 8, at D12), which #491 also asks for; another
input method than the emulator's Gboard; Back while Toggle keyboard shows
the soft keyboard, which Android gives to the input method to hide it.
