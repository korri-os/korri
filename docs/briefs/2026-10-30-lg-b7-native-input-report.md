# LG B7 native Slint + korrid: input session report (2026-10-30)

Status when work paused: the app is installed but does not start. The TV runs, Lightkeeper is in front, and our app is not running. Do not relaunch the app until the open problem below is understood.

## Current state

| Item | Value |
|---|---|
| Experiment worktree | `.worktrees/c-1c512a91…`, HEAD `39dee17a1622ef0024b85291f0990723a05a240c` (clean) |
| Main | untouched (`6c0e5c702`) |
| Installed app ELF | `729ac4b912d8fbc797ba83bf103093cee42e0ba4a4d83af2bb18714387fab254` (source `39dee17a1`) |
| `prepare-owned-upgrade.sh` prior | `729ac4b9…` (matches the installed bytes) |
| TV | power state `Active`; foreground `io.github.chiotas.lightkeeper`; our app stopped |
| Last check run | `04f8c404-937e-47a4-bd89-605580cae2a1`: launch accepted, process alive 45 s, 2 threads, RSS 4.8 MB, never foreground |

## What works (verified on the TV)

- Native Slint renders through the system SDL on the B7. The compositor screenshot showed all four real korrid RPCs answered in 10 ms (`NATIVE-CANDIDATE-eb9ae461c.md`).
- Startup panic fixed: getrandom `linux_raw` backend (`eb9ae461c`). The jail has no `/dev/random`, and glibc 2.19 has no `getrandom` wrapper.
- With the physical remote on the system SDL builds (`33ee8debc` and later):
  - D-pad navigation works.
  - Back closes the app (`SDL_WEBOS_ACCESS_POLICY_KEYS_BACK`, scancode 482).
  - OK confirms the focused button when the cursor is hidden: Return arrives as scancode 40, keycode 13.
  - Magic Remote pointer clicks work.

## Findings

1. **Synthetic input does not reach native SDL apps.** Luna `networkinput/test/sendKeyCode` and a temporary `/dev/uinput` keyboard both left the app unchanged. Physical remote keys do arrive. Use the physical remote for input tests.
2. **The B7's system SDL is LG `libsdl2-webos` 2.0.4.** Its keyboard event has an extra `inputSource` field after `windowID`, so key fields sit 4 bytes later than upstream. This matches `SDL_WEBOS_BROKEN_ABI` in webosbrew SDL-webOS. Fixed in `33ee8debc`; the probe recorded Right as scancode 79.
3. **LG remote codes:**

   | Code | Meaning |
   |---|---|
   | 79 / 80 / 81 / 82 | Arrows |
   | 40 | OK |
   | 482 | Back |
   | 484 | Cursor shown |
   | 485 | Cursor hidden |

4. **Cursor hiding on D-pad press.** The LG menu and Moonlight do this. The app cannot do it through the system SDL on webOS 3.9:
   - `SDL_ShowCursor(0)` hides the cursor, and shaking the remote no longer brings it back.
   - `wl_webos_input_manager` on webOS 3.9 has no `set_cursor_visibility` request. The probe recorded `cursor:no_method`.
   - Changing `wl_starfish_pointer.set_mrcu_standby_timer` on SDL's connection had no visible effect.
5. **Moonlight bundles its own SDL.** Moonlight 1.6.36 loads `lib/backports/libSDL2-2.0.so.0`, which is webosbrew SDL-webOS `release-2.30.12`. It calls `SDL_webOSCursorVisibility(false)` on arrow keys, and the user confirmed the cursor hides on this TV.
6. **Bundling that SDL froze the TV.** Commit `6dcd436dc` bundled the same release (`release-2.30.12-webos.5`, non-ABI variant). The archive SHA-256 is `4ad56645…` and the library SHA-256 is `f564f7da…`. The probe showed the cursor hiding (485 after arrows). But then:
   - The TV got stuck in the screensaver.
   - The user had to unplug it.
   - A later launch from the LG menu froze the TV.
   - Probable factor (unconfirmed): SDL-webOS registers the app (`registerNativeApp`), answers screensaver requests (`tvpower registerScreenSaverRequest`) and subscribes to power state. The system SDL build did none of this.
7. **After the freeze, the reverted system-SDL build hangs early.** Unknown whether the cause is TV state left over after the unplug or AppManager registration left by the bundled build.

## Open problem: next steps, in order

1. Ask the user whether the TV UI responds normally, and what the screen showed during the last check run.
2. Read-only checks:
   - `getAppInfo` for `org.korri.slint-spike`, looking for any lifecycle or registration state.
   - Process state of the hung app on the next run, captured with the exit-only trace tooling, not by guesswork.
3. If the TV needs it, ask the user to do a full power cycle, then run one bounded check of the reverted build.
4. Only after the hang is explained, retry the bundled SDL with `SDL_WEBOS_REGISTER_APP=false` set. Keep the screensaver and power subscriptions in mind. Compare with Moonlight's `src/app/app.c` hints. Retry only with the user watching and ready to unplug.

## Commits on the experiment branch (this session)

| Commit | Change |
|---|---|
| `df023c55d` | Input probe |
| `dbc5b1868` | APPID experiment (no-op; AppManager already sets it) |
| `ec7a706c7` | Back access policy and key numbers in the probe |
| `33ee8debc` | LG keyboard event offsets |
| `de6731d7a` | OK as confirm |
| `ad77ceb96` | D-pad/pointer handover by motion (rejected by the user) |
| `c266c2d4d` | Handover follows LG cursor events 484/485 |
| `eae083b13` | `SDL_ShowCursor` attempt (cursor could not come back) |
| `be0209db8`, `c3138a4a1` | `set_cursor_visibility` attempts (request absent on 3.9) |
| `b94dc1c3a` | starfish standby timer attempt (no effect) |
| `6dcd436dc` | Bundled SDL-webOS (froze the TV) |
| `39dee17a1` | Revert of `6dcd436dc` (installed now) |

## Key evidence files

These live outside the repository, on the workstation, under
`~/.cache/research/korri-lg-b7-prep-20261029T175543Z/readiness/`.

- `AUTONOMOUS-STARTUP-STATUS.md`: running log up to the focus probe.
- `NATIVE-CANDIDATE-eb9ae461c.md`, plus `native-capture-eb9ae461c-combined/`: rendering and RPC proof.
- `sdl-webos-2.30.12-webos.5/`: pinned SDL-webOS download.
- Probe and inspection scripts:
  - `read-own-input-probe.*`
  - `probe-moonlight-*.sh`
  - `probe-lg-cursor-mechanism.sh`
  - `probe-webos-wayland-protocol-strings.sh`
  - `tv-power-state-and-wake.sh`
  - `close-own-app.sh`
  - `launch-own-app.sh`
- Managed process logs: `~/.local/share/forgerie/managed-processes/01a188a6-…/<id>/output.log`. The key IDs appear above.

## Notes

- Wake-on-LAN (`wake-owned-b7.py`) recovers the TV from network standby.
- The installer refuses to replace the app while it runs. Close it first with `close-own-app.sh`.
- Input-probe files accumulate in `/tmp` on the TV. The reader limit is now 64.
- Unresolved, unchanged: Slint embedded licensing; the native surface contract.
