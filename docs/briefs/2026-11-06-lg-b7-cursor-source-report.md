# LG B7 cursor source and binary investigation (2026-11-06)

**The Moonlight cursor API cannot send its request on the B7.** There is no
webOS 3.x fallback in the pinned SDL source. The observation that Moonlight
hides the cursor is not proof that this API succeeds. Do not load another SDL,
invent an opcode, or enable more power/lifecycle effects to bridge this gap.

After this finding, the parent requested a low-risk candidate using the system
SDL's already-proven hide, paired with explicit restoration. That candidate is
implemented below. Its TV behaviour is not yet verified. This child did not
contact the TV.

## Sources checked

| Source | Pinned revision |
|---|---|
| webosbrew/SDL-webOS, `release-2.30.12-webos.5` | `9f30a1f01e4d36aa5d3e4e04df5921b7a9d093ee` |
| mariotaku/moonlight-tv, `v1.6.36` | `89221f66b79e06178a6465503cc5a306d2794786` |
| Parent's copies from the B7 | `~/.cache/research/korri-webos-m1/evidence/` |
| Previous downloaded SDL release | `~/.cache/research/korri-lg-b7-prep-20261029T175543Z/readiness/sdl-webos-2.30.12-webos.5/` |

Source citations below are relative to these pinned upstream checkouts.
The workstation research checkout is `/tmp/korri-cursor-research/`.

## Actual mechanism

1. SDL `src/core/webos/SDL_webos_cursor.c:6-16` calls the mouse backend's
   `WebOSSetCursorVisibility` function.
2. `src/video/wayland/SDL_waylandmouse.c:756-775` returns `SDL_FALSE` if
   `WL_WEBOS_INPUT_MANAGER_SET_CURSOR_VISIBILITY == -1`. Otherwise it sends
   `wl_webos_input_manager.set_cursor_visibility(visible)` and returns true.
   This path does not call Luna, change a hint, use a surface, change SDL's
   `cursor_shown`, or install a motion callback.
3. `SDL_waylandvideo.c:962-966` binds `wl_webos_input_manager` at **version 1**
   with the interface from the installed `libwayland-webos-client.so.1`.
   `SDL_waylandwebos_abifix.c:35-43, 54-60, 72-107` maps request names from
   that library's method table. Missing requests remain `-1`. There is no
   alternate mechanism for an older compositor.
4. `wayland-protocols/webos-input-manager.xml:3-28` describes the newer v1
   interface: the visibility request takes one `uint`. The separate
   `cursor_visibility` **event** takes `uint, nullable wl_webos_seat`.
   The version number alone does not establish the request ABI.
5. Moonlight `src/app/lvgl/input/lv_drv_sdl_key.c:246-255` calls this API for
   arrows when the screen keyboard is not shown. It **ignores the return
   value**. `src/app/app.c:64-83` sets Back/Exit, 5000 ms cursor sleep, 60 Hz
   cursor frequency, calibration-disable, and Bluetooth hints before video
   starts. It does not add a fallback around the cursor API.

### B7's real interface, not a string-search proxy

Parsed the ELF32 `wl_webos_input_manager_interface` at virtual address `0xa594`
from the parent's `tv-libwayland-webos-client.so.1.0.0`:

| Table | Opcode | Name | Signature |
|---|---:|---|---|
| Request (count 1) | 0 | `get_webos_seat` | `no` |
| Event (count 1) | 0 | `cursor_visibility` | `u?o` |

Interface version: 1. Library SHA-256:
`869733a104dd714bfcc43627de54ea9d9a770e9c920956c3ebf13f1beae3a74c`.
There is no visibility request to reimplement through SDL's Wayland display.
Getting a surface from `SDL_GetWindowWMInfo` does not supply the missing method.

### Moonlight's different file hash is not a different cursor implementation

| File | SHA-256 |
|---|---|
| Actual Moonlight SDL copied by parent | `036f924b54f53359b8dc85c2a67c203a79479beb9b9241fa28c7e70e5afcd630` |
| Previously bundled non-ABI SDL release | `f564f7da624cec47168300d017f3ec822f41b5096f0ce1c96873eecd8614d446` |
| Downloaded ABI release for comparison | `5156b14097845d3508afc761cd5b56b4a37e6fb660f29e745b5f66e11dca386d` |

Moonlight's revision string is
`SDL-release-2.30.12-webos.5-0-g9f30a1f (webOS Backport)`.
Its `.text` matches the non-ABI release **byte for byte** (SHA-256
`d17c7cacd20e3228f03cbb6f98bc6169d7ef64495b267b2148b80df6a5729258`).
Every section of Moonlight's file except `.shstrtab` matches the stripped
non-ABI release; the ELF section-table metadata differs. The ABI release's
`.text` differs. Thus a difference in the full-file hash does not explain
Moonlight's cursor behaviour. No copied SDL was loaded or executed.

## Registration hypothesis and TV-wide effects

**Do not assume that the system SDL does no registration.** Its actual ARM
code checks the literal hint name `SDL_HINT_WEBOS_REGISTER_APP` (not modern
SDL-webOS's `SDL_WEBOS_REGISTER_APP`). In the copied system SDL, Thumb virtual
addresses `0xcc5e-0xcc70` get that hint and parse it with `strtol`:

| Value in LG SDL 2.0.4 | Branch |
|---|---|
| Unset, or numeric zero | Calls `registerNativeApp` at `0xccb0-0xccb6` |
| Numeric nonzero | Skips that call, branches to `0xccb8` |

This is inverted relative to the modern hint's boolean meaning. Merely setting
`SDL_WEBOS_REGISTER_APP=true` is not a system-SDL experiment.
`SDL_GetHint`'s implementation at `0xdb60-0xdb92` reads the environment too.
The parent checked the running shell and spike on the TV: this hint is
**absent** in both. AppManager supplies `SDL_MRCU_TIMER=300000` and
`SDL_VIDEODRIVER=wayland`. Neither copied SDL contains an `SDL_MRCU_TIMER`
string, and the pinned modern SDL source does not read that variable.
The system SDL already takes its registration branch. Registration is not an
untried cursor mechanism. Drop the proposed registration experiment.

The system SDL also calls `getPowerState` (`0xcabe-0xcac8`); that helper has a
10 ms sleep/retry loop waiting for completion (`0xcad2-0xcae0`) with no visible
timeout. Its video code contains `registerScreenSaverRequest`,
`responseScreenSaverRequest`, and `turnOnScreen`; its video-init path calls the
registration helper with 1 at `0x82776-0x82778`. Their presence must not be
mistaken for proof that the current process executes every path.

Modern SDL-webOS's registration bundles these effects in
`src/core/webos/SDL_webos_init.c`:

| Call or effect | Lines | Scope/risk |
|---|---|---|
| `getAppInfo`, reads lifecycle version | 135-166 | AppManager read; synchronous service wait |
| `registerNativeApp` or `registerApp` | 169-190 | **TV-wide service**, this app's lifecycle subscription |
| Relaunch raises window; v2 close queues quit | 193-239 | This app's lifecycle and focus |
| `registerScreenSaverRequest` | 242-261 | **TV-wide power service**, subscribes as `<appid>.wakelock` |
| `responseScreenSaverRequest` | 264-302 | **TV-wide screensaver decision**; ack is `!suspend_screensaver` |
| `getPowerState`, subscribed | 305-335 | **TV-wide power service**; updates local cached power state |
| `turnOnScreen` | 338-345 | **TV-wide screen wake**; separate helper, not called by the cursor API or registration itself |
| Unregisters three service callbacks | 96-118 | Subscription cleanup on SDL shutdown |

`SDL.c:233-243` initiates modern registration during subsystem initialization.
Changing registration is not an app-local cursor operation. No registration,
screensaver, power, or lifecycle call was added. An outside watchdog cannot
guarantee recovery from a compositor/TV hang; the TV may need power removal.
This child ran no registration experiment.

## Appinfo comparison

The LG TV appinfo reference at
`https://webostv.developer.lge.com/develop/references/appinfo-json`
(section `supportTouchMode`, checked 2026-11-06) says this key applies **only
to StanbyME**. `full` means the application handles touch input. It does not
document a Magic Remote cursor policy. Moonlight added it in commit
`74c85d273e2c558ca4d934415d8f551a4fd56abb` (2024-02-27), titled
“trying supportTouchMode”.

Moonlight added `useAllMouseButtons` in `89221f66b79e06178a6465503cc5a306d2794786`
(2025-10-18), titled “declared useAllMouseButtons in appinfo”. That change only
adds the metadata key. No B7 implementation of this key was available to this
investigation. It is not evidence of a D-pad cursor policy. `iconColor`,
`largeIcon` and `splashBackground` describe launch presentation; `dialAppName`
is launch/discovery metadata, not a documented cursor control.

Current public webosose SAM and LSM source checkouts (SAM
`96161d3e50670fb21daccfb12a75c35fe406ce0e`, LSM
`b2fef6f73232244f6f13102aa66d2d3f5942835e`) contain neither
`supportTouchMode` nor `useAllMouseButtons`. They are **not** the B7's closed
webOS 3.9 source, so absence there cannot prove that the B7 ignores a key.
An appinfo-only A/B is lower risk than a new SDL or registration path, but its
relevance to this requirement remains unproven. Do not ship these keys as a
cursor fix on this evidence.

## Remaining differences and implementation limits

- Modern SDL sets `cloudgame_active=true` by default and can set
  `restore_cursor_position` and `cursor_fps` on the **app's shell surface**
  (`SDL_waylandwebos.c:135-179`). These are concrete differences from the
  system SDL's string set, not a proven explanation for D-pad hiding on 3.9.
  Do not translate them into appinfo fields without an observed producer.
- The old Korri `webos_cursor.rs` changed the starfish standby timer. The
  earlier device report says that had no visible effect. It was not the
  mechanism of `SDL_webOSCursorVisibility`. This candidate removes it and the
  blocking `wl_display_roundtrip` at startup.
- The old `SdlPlatform::key_event` returned through `host_keys` before its
  cursor and 484/485 logic. The real shell uses that path. This candidate
  performs cursor handover before delivering host keys.
- Further trace: SDL `SDL_waylandevents.c:1336-1379` forwards keyboard input;
  `SDL_keyboard.c:816-959` updates key state and queues it, without cursor
  control. `SDL_waylandvideo.c:861-865` ignores the compositor visibility
  event. The only starfish timer set is at `SDL_waylandvideo.c:967-970`, on
  initial binding, not on an arrow. Moonlight's `root.c:234-240` only stores
  its UI input mode. None supplies an older-OS cursor API fallback.

## App-owned candidate implemented at the parent's request

The existing `webos` feature selects `window-sdl`; the cursor adapter runs only
on its TV path (no X11 display). It loads **only system SDL**.

- Startup queries `SDL_ShowCursor(-1)` but does not change visibility.
- A D-pad press (79-82) calls `SDL_ShowCursor(0)` once. Release, repeat and LG
  hidden notification 485 leave it hidden.
- The next SDL mouse-motion event calls `SDL_ShowCursor(1)` if the adapter
  owns a D-pad hide, and puts the view in pointer mode. LG shown notification
  484 also calls `SDL_ShowCursor(1)`.
- LG idle-hide notification 485 never calls `SDL_ShowCursor(0)`. Its idle
  policy remains independent of SDL's app-owned flag.
- OK can still arrive as a left click after an app-local hide. In key mode
  the shell's host callback now receives Confirm for its press and release;
  the inspector keeps its existing Slint Return route.
- Negative SDL returns are logged and do not claim a successful state change.
- No direct Wayland, cursor timer, Luna, registration, power or screensaver
  operation is added. The existing screensaver-allow setup stays unchanged.

Startup stderr on the TV:

```text
sdl: webOS cursor active: system SDL_ShowCursor; hide on D-pad, restore on motion/484; no native visibility request on B7
```

A query failure instead reports `webOS cursor unavailable` and SDL's error;
X11 workstation runs report that the TV mechanism is unavailable on X11.
Tests cover the direction/release/notification policy, and exercise the actual
host-key route with real workstation SDL using its dummy video driver.

This is **not** a claim that Moonlight uses the same pair. Parent testing must
establish whether the system SDL delivers motion or 484 while its app-owned
cursor flag is off. If it delivers neither, restoration remains blocked.
Small or synthetic motion after a D-pad press can also restore the cursor;
no ungrounded distance threshold was invented. Ordinary SDL calls still enter
LG's old backend, so this is lower risk than replacing SDL, not a guarantee
against a compositor defect. The parent owns a bounded on-TV test.

## Workstation checks

- `shell-check` ran once. Both format checks and desktop/webOS Clippy passed
  before the implementation change. It passed 81 tests, then failed the
  desktop-only `zone_previews_are_byte_identical_cached_and_direct` test:
  cached/direct Retro page PNGs differed at 1280x720, 1920x1080, 480x800 and
  640x480. No cursor adapter is selected by that desktop test build.
- After implementation, all **15 window-sdl tests passed**, including the
  real-SDL host-key, click-confirm, motion and 484/485 path. The workstation
  SDL was `sdl2-compat` 2.32.64 with its dummy video driver. It reported denied
  access to workstation input nodes; the test uses constructed SDL events,
  not hardware input. This does not establish B7 motion delivery.
- Adapter all-target Clippy and the shell's webOS Clippy both passed with
  `-D warnings` after implementation. Both format checks and `git diff --check`
  passed too. Build/package results are recorded with the delivered artifact.
