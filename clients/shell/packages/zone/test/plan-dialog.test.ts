/**
 * A dialog's buttons stay inside the window (the reachability invariant):
 * src/plan/screen.ts dialogButtons, which ZoneDialog draws and the focus
 * graph walks, at 240x480, 320x240 and the five probe sizes, in every look,
 * for Korri's real long labels and every dialog the scenarios open.
 */
import { describe, expect, test } from "bun:test"
import { dialogButtons, zoneScreen } from "../src/plan/screen"
import { ZONE_LOOKS } from "../src/plan/look"
import { scenarioNames, SIZES, Walk } from "./walk"

const WINDOWS = [[240, 480], [320, 240], ...SIZES] as const

/** Korri's longest real button words: owner binding, saves, shared play. */
const LABELS: readonly (readonly string[])[] = [
  ["Check for signer"],
  ["No", "Check for signer"],
  ["Cancel", "Check for signer"],
  ["zao · 19 Oct 2026 · 11h 31m", "mini · 18 Oct 2026 · 2h 5m", "Cancel"],
  ["Keep both", "Use this device's save", "Use the other save"],
  ["Retry", "Dismiss"],
]

/** Every button box of a dialog, as the window places it (ZoneDialog: the row at x = width / 8). */
function inside(labels: readonly string[], width: number, height: number, look: (typeof ZONE_LOOKS)[number]): string[] {
  const screen = zoneScreen(width, height, look)
  const planned = dialogButtons(labels, screen)
  return planned.boxes.flatMap((box, index) => {
    const x = width / 8 + box.x
    return x >= 0 && x + box.w <= width ? [] : [`${look} ${width}x${height} "${labels[index]}" at ${x.toFixed(1)}..${(x + box.w).toFixed(1)} (cap ${planned.cap})`]
  })
}

describe("every dialog button stands inside the window", () => {
  test("Korri's long labels", () => {
    const out: string[] = []
    for (const labels of LABELS) for (const look of ZONE_LOOKS) for (const [w, h] of WINDOWS) out.push(...inside(labels, w, h, look))
    expect(out).toEqual([])
  })

  test("the dialogs the scenarios open", () => {
    const out: string[] = []
    for (const name of scenarioNames()) {
      for (const [w, h] of WINDOWS) {
        const dialog = new Walk(name, w, h).view().dialog as { shown: boolean; buttons: { label: string }[] } | undefined
        if (dialog?.shown !== true) continue
        for (const look of ZONE_LOOKS) out.push(...inside(dialog.buttons.map(button => button.label), w, h, look))
      }
    }
    expect(out).toEqual([])
  })

  test("caps-2 while it fits, then caps-1, then smaller only for words caps-1 cannot hold", () => {
    const wide = zoneScreen(1920, 1080, "blend")
    expect(dialogButtons(["No", "Yes"], wide).cap).toBe(2 * 10 * wide.a)
    const small = zoneScreen(240, 480, "blend")
    expect(dialogButtons(["Check for signer"], small).cap).toBe(10 * small.a)
    expect(dialogButtons(["Check for signer, then keep this device as the owner"], small).cap).toBeLessThan(10 * small.a)
  })

  test("the focus graph walks the same boxes ZoneDialog draws", () => {
    const screen = zoneScreen(240, 480, "blend")
    const planned = dialogButtons(["Check for signer"], screen)
    expect(planned.boxes[0]!.w).toBeLessThanOrEqual((240 * 3) / 4 - 22 * screen.a)
  })
})
