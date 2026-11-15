# Slice D8 — korrid and the shell in one library, on the Android emulator

Run on 2026-11-14 at 21:24 to 21:26 (zao local time) from commit
`77e4d08c53619f0d7fe49b2fc83827892d0cc858`, with a clean tree.

```sh
nix run .#android-app-proof -- --out /tmp/korri-d8/proof-3
```

No person took part. The tool built the APK on zao
(`/nix/store/0aps2pvb9byj8nw55crhmab6jbqir8zi-korri-android-app-apk-0.1.0`,
71,426,180 bytes signed) and signed it with the user's debug key. The
emulator was Android 12 (API 31), google_apis, x86_64, headless, 2 cores,
`nice` 10, console port 5612, its data wiped at the start: a new device. It
was stopped at the end; no emulator process was left. All 20 checks passed
(`result.json`).

| Step | What happened |
| --- | --- |
| The APK | No dex (`apk-contents.txt`); `hasCode=false` (`apk-manifest.txt`); `android.app.NativeActivity`, package `com.simonwjackson.korri.app` (`apk-badging.txt`); `libkorri_android_app.so` for arm64-v8a (103,123,768 bytes) and x86_64 (103,763,800 bytes) |
| First start | korrid served on `127.0.0.1:38205` 1.7 s after `am start` (0.7 s after the process started) and answered the shell's `system.health`. korrid observed 3 addresses on `wlan0` through its Rust network reader. The window flags were applied on Android's main thread. The shell drew Velvet's library page, `empty` with no games, 9.4 s after `am start` (`home.png`) |
| korrid from outside | Through `adb forward`, korrid's port answered 401 to a call without the capability and to one with a wrong capability |
| Back, twice | The activity stayed resumed, in the same process (the screenshot was the same as `home.png`) |
| Home, then a start | The launcher resumed; the start showed Velvet again in the same process (`relaunch.png`); korrid still served |
| `wm density 360` | Android ended the activity (`the activity ended`) and made a new one in the same process; its `android_main` found korrid on the same port, 38205, and the shell drew Velvet (`recreated.png`). `wm density reset` did the same once more |
| `am force-stop`, then a start | A new process, 3792: korrid started on its saved state on port 43537, answered, and the shell drew Velvet (`cold.png`) |
| Logcat | No fatal signal, panic or ANR (`logcat-app.txt`: the app's lines and the system's about it) |

Every screenshot shows Velvet's "Connect your identity": the device has no
owner, and this app has no signer route yet. The status bar is still drawn
over Velvet, although the window manager reports it hidden (README, known
gaps).

The host was loaded by other work during the run; the timings mean nothing
(ADR 0063).
