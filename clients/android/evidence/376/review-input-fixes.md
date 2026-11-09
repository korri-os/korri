# Review input fixes (2026-11-09)

Reviewed base: `028a33ed0f350c0057a166817a204b1789e32c5a`, rebased onto main `e69978db0`.

## P1: retained gyro rate

Reproduced with a recording shadow at the real `MoonBridge.sendControllerMotionEvent` JNI-call boundary: a nonzero gyro sample followed by `setInputHeld(true)` produced no zero call. Two tests failed before the fix.

`NvConnection` now tracks the byte controller IDs that sent gyro samples, independently of button reports. Entering the hold sends `(0,0,0)` gyro to each through MoonBridge before blocking later samples. The unchanged native `InputStream.c:571–576` gives these zero-gyro packets reliable delivery. The cache then clears; Resume accepts fresh samples without restoring an old rate. Acceleration is not neutralized, preserving attitude, as the existing sensor shutdown does.

The native-call spy tests cover nonzero → hold → zero, multiple gyro controllers, blocked later gyro/acceleration, repeated hold, Resume/fresh rates, and acceleration-only input.

## P2: swallowed system keys

Reproduced with the real Game key callbacks: volume-down/up/mute and power were consumed by the hosted overlay. The new regression failed before the fix.

The hosted bridge and legacy overlay now share the legacy exclusion list. Game's pre-IME/Activity key callbacks return these keys to Android before any gameplay processing. Activity dispatch also skips overlay forwarding for them. The regression checks both edges, an empty overlay key queue, gameplay input remaining held, and normal D-pad forwarding.

## Checks

- Before fixes: 3 new tests failed on the reported behavior (2 gyro, 1 system-key test).
- Focused JVM checks: **59 passed**, zero failures/errors. Scope: NvConnection policy/lifecycle/hold, overlay key/motion/lifecycle, viewer boundaries/state, controller heartbeat/hold, and Moonlight parity inventory.
- `nix run .#android-apk`: passed, including both surface verifiers and arm64 native builds. APK SHA256: `d5169446df5d624e410044d09943525a2acbea1c30bcbfa02432d8dbd221790f`.
- Android lint: still fails with **20 errors and 281 warnings**, the same counts as before; first failure remains `StreamSettings.java:134`, `MissingSuperCall`. The complete issue sets were not independently compared.
- `git diff --check`: passed. Kept NvConnection's CRLF line endings; no whole-file rewrite.
- Secret pre-commit hooks run normally.

No phone installation, device input, landing or push performed. Native-call tests do not prove behavior on a physical gyro controller; no device verification was requested for this round.
