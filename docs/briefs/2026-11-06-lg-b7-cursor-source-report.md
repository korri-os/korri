# LG B7 cursor source and binary investigation (2026-11-06)

**Blocked: the Moonlight cursor API cannot send its request on the B7.**
There is no webOS 3.x fallback in the pinned SDL source. The observation that
Moonlight hides the cursor is not proof that this API succeeds. Do not load
another SDL, invent an opcode, or enable more power/lifecycle effects to bridge
this gap. This investigation did not contact the TV or change runtime code.

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
The current Korri launcher does not set either registration hint; the actual
AppManager-supplied value in the running process remains unknown.

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
Changing registration is not an app-local cursor operation. A parent-run
experiment must first inspect only the old hint's value. If it is unset or
zero, the binary already takes the registration branch; forcing zero adds no
new test. If it is nonzero, overriding **`SDL_HINT_WEBOS_REGISTER_APP=0`** is a
possible existing-build experiment, not a proven fix. Use one bounded launch,
with the user ready, preserve the screensaver-allow setting, and close the app
after the cursor test. The outside watchdog cannot guarantee recovery from a
compositor/TV hang; the TV may need power removal. Do not run it merely to
confirm a branch already taken. This child ran no such experiment.

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
- Existing Korri `webos_cursor.rs` changes the starfish standby timer. The
  earlier device report says that had no visible effect. It is not the
  mechanism of `SDL_webOSCursorVisibility`.
- Existing `SdlPlatform::key_event` returns through `host_keys` before its
  cursor and 484/485 logic. The real shell uses that host-key path. Any later
  cursor implementation must run its input handover before that return.
- Existing cursor startup uses a blocking `wl_display_roundtrip`. A parent
  watchdog does not make a synchronous Wayland request inherently safe.

No runtime change, new unit test, startup diagnostic, ARM build or IPK is
claimed. The requested source path stops at an interface absent on this TV.
A new package would not meet the requirement. The parent still needs evidence
of which other Moonlight/compositor behaviour causes the observed hiding.
