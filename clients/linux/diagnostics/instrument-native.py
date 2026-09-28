"""Build-time-only counters at existing production selection/receive/read sites.

No adapter replacement, input injection, WebSocket wrapper, or credential access.
The Nix variant applies this to its private source copy, never the working tree.
"""
from pathlib import Path
import sys


def replace(path, old, new):
    text = path.read_text()
    if text.count(old) != 1:
        raise SystemExit(f"counter instrumentation source drift: {path.name}")
    path.write_text(text.replace(old, new))


def instrument(root):
    source = root / "clients/portal/src"
    (source / "input/native-probe-counts.ts").write_text('''// TEST ONLY: never included by the production portal derivation.
const counts = {
  nativeStarts: 0, nativeSamples: 0, nativeInitializations: 0,
  gamepadStarts: 0, gamepadReads: 0,
}
Object.defineProperty(window, "__korriNativeProbeCounts", {
  get: () => ({ ...counts }), configurable: false,
})
export function countNativeProbe(key: keyof typeof counts): void {
  counts[key] = Math.min(1_000_000, counts[key] + 1)
}
''')
    for name in ("native-adapter.ts", "gamepad-adapter.ts"):
        path = source / "input" / name
        path.write_text('import { countNativeProbe } from "./native-probe-counts"\n' + path.read_text())
    native = source / "input/native-adapter.ts"
    replace(native, "    start(emit) {", '    start(emit) {\n      countNativeProbe("nativeStarts")')
    replace(native, "onInitialized: () => { if (active())", 'onInitialized: () => { countNativeProbe("nativeInitializations"); if (active())')
    replace(native, "        onEvent: event => {", '        onEvent: event => {\n          if (event.kind === "input") countNativeProbe("nativeSamples")')
    gamepad = source / "input/gamepad-adapter.ts"
    replace(gamepad, "    start(emit) {", '    start(emit) {\n      countNativeProbe("gamepadStarts")')
    replace(gamepad, "(() => navigator.getGamepads())", '(() => { countNativeProbe("gamepadReads"); return navigator.getGamepads() })')
    # If another read site appears, the claimed counter coverage needs review.
    sites = [p for p in source.rglob("*.ts*") if not p.name.endswith((".test.ts", ".test.tsx")) and "navigator.getGamepads()" in p.read_text()]
    if sites != [gamepad] or gamepad.read_text().count("navigator.getGamepads()") != 1:
        raise SystemExit("Gamepad API read coverage changed")


if __name__ == "__main__":
    instrument(Path(sys.argv[1]))
