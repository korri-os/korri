/**
 * Zone's looks: where the device keeps the choice, and the LOOK row that
 * makes it.
 *
 * Storage mirrors Pico's face (surfaces/pico/src/pico-font-preference.ts):
 * one key, `zone.look`, one id; missing, unreadable, blocked or unknown is
 * Blend; a failed write is swallowed. The LOOK row is OPTIONS › SYSTEM's,
 * a horizontal control the portal steps with `korri-semantic-direction`;
 * the look takes effect at once, the cursor stays on the row, the device
 * keeps it, and Korri is never told.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, render, within } from "@testing-library/react"
import type { SurfaceHost, SurfaceInputAction, SurfaceModel } from "@contracts/surface/korri-surface"
import { ZoneSurface } from "../src/index"
import { isZoneLook, ZONE_DEFAULT_LOOK, ZONE_LOOKS, zoneLookDrawing, zoneLookStep } from "../src/plan/look"
import { optionsPlan, ZONE_OPTIONS_SYSTEM, ZONE_SYSTEM_LOOK } from "../src/plan/options"
import { updateLook } from "../src/state/look"
import { update, zoneSurfaceStart } from "../src/state/surface"
import { readZoneLook, rememberZoneLook, ZONE_LOOK_KEY } from "../src/zone-look-preference"
import { performZoneRequest } from "../src/zone-host"
import { byWords } from "./words"

afterEach(() => {
  cleanup()
  localStorage.removeItem(ZONE_LOOK_KEY)
})

/** Run `body` with the browser's storage replaced by `storage` (a getter that may throw). */
function withStorage(storage: () => Storage, body: () => void): void {
  const own = Object.getOwnPropertyDescriptor(window, "localStorage")
  Object.defineProperty(window, "localStorage", { configurable: true, get: storage })
  try {
    body()
  } finally {
    if (own === undefined) Reflect.deleteProperty(window, "localStorage")
    else Object.defineProperty(window, "localStorage", own)
  }
}

const failing = (method: "getItem" | "setItem"): Storage => {
  const real = window.localStorage
  return new Proxy(real, {
    get(target, key) {
      if (key === method) return () => { throw new Error(`${method} refused`) }
      const value = Reflect.get(target, key) as unknown
      return typeof value === "function" ? value.bind(target) : value
    },
  })
}

describe("zone.look: the device keeps one look id", () => {
  test("the key is zone.look, as Pico's is pico.font", () => {
    expect(ZONE_LOOK_KEY).toBe("zone.look")
  })

  test("missing is Blend", () => {
    localStorage.removeItem(ZONE_LOOK_KEY)
    expect(readZoneLook()).toBe("blend")
    expect(ZONE_DEFAULT_LOOK).toBe("blend")
  })

  test("an unknown id is Blend", () => {
    for (const stored of ["MODERN", "pixel", "", " retro", "{\"look\":\"retro\"}"]) {
      localStorage.setItem(ZONE_LOOK_KEY, stored)
      expect(readZoneLook()).toBe("blend")
    }
  })

  test("blocked storage (reading localStorage throws) is Blend, and a write is swallowed", () => {
    withStorage(() => { throw new Error("SecurityError: storage is blocked") }, () => {
      expect(readZoneLook()).toBe("blend")
      expect(() => rememberZoneLook("modern")).not.toThrow()
    })
  })

  test("an unreadable value (getItem throws) is Blend", () => {
    localStorage.setItem(ZONE_LOOK_KEY, "retro")
    withStorage(() => failing("getItem"), () => {
      expect(readZoneLook()).toBe("blend")
    })
  })

  test("a failed write (full storage) is swallowed and keeps what was there", () => {
    localStorage.setItem(ZONE_LOOK_KEY, "retro")
    withStorage(() => failing("setItem"), () => {
      expect(() => rememberZoneLook("modern")).not.toThrow()
    })
    expect(readZoneLook()).toBe("retro")
  })

  test("each look, set, reads back as itself and is stored as its id alone", () => {
    for (const look of ZONE_LOOKS) {
      rememberZoneLook(look)
      expect(localStorage.getItem(ZONE_LOOK_KEY)).toBe(look)
      expect(readZoneLook()).toBe(look)
    }
  })

  test("the host door keeps it on the device and calls no host method", () => {
    const touched: string[] = []
    const host = new Proxy({}, { get: (_target, key) => { touched.push(String(key)); return () => undefined } }) as SurfaceHost
    performZoneRequest(host, { _tag: "RememberLook", look: "modern" })
    expect(touched).toEqual([])
    expect(localStorage.getItem(ZONE_LOOK_KEY)).toBe("modern")
  })
})

describe("the looks, as data", () => {
  test("three, from modern to retro; a stored value is one of them or not", () => {
    expect(ZONE_LOOKS).toEqual(["modern", "blend", "retro"])
    expect(ZONE_LOOKS.every(isZoneLook)).toBe(true)
    expect(isZoneLook("Modern")).toBe(false)
  })

  test("a step moves one along the axis, round the ends, both ways", () => {
    expect(zoneLookStep("blend", 1)).toBe("retro")
    expect(zoneLookStep("retro", 1)).toBe("modern")
    expect(zoneLookStep("modern", 1)).toBe("blend")
    expect(zoneLookStep("blend", -1)).toBe("modern")
    expect(zoneLookStep("modern", -1)).toBe("retro")
    expect(zoneLookStep("retro", -1)).toBe("blend")
  })

  test("Blend and Retro draw the same in TypeScript; Modern draws smooth and free", () => {
    expect(zoneLookDrawing("retro")).toEqual({ edges: "stairs", rings: "pixel", motion: "art" })
    expect(zoneLookDrawing("blend")).toEqual(zoneLookDrawing("retro"))
    expect(zoneLookDrawing("modern")).toEqual({ edges: "smooth", rings: "smooth", motion: "free" })
  })

  test("a step asks to keep the new look; Korri hears nothing", () => {
    expect(updateLook("blend", { _tag: "SteppedLook", direction: 1 }))
      .toEqual({ model: "retro", requests: [{ _tag: "RememberLook", look: "retro" }] })
  })
})

// ---- The LOOK row ----

const MODEL: SurfaceModel = {
  presentation: { kind: "catalog" },
  catalog: { _tag: "Ready", games: [{ id: "celeste", title: "Celeste Classic", subtitle: "PICO-8" }] },
  status: { _tag: "Browsing" },
  clockLabel: "21:47",
  buildLabel: "korri 0.4.1 · 9a1fb22",
  actions: [{ id: "pair", label: "Pair controller", enabled: true }],
  settings: [],
  settingsStatus: { _tag: "Idle" },
}

interface Recorded {
  readonly host: SurfaceHost
  readonly calls: string[]
  readonly press: (action: SurfaceInputAction) => void
}

function recordingHost(): Recorded {
  const calls: string[] = []
  const listeners = new Map<SurfaceInputAction, Set<() => void>>()
  const host = new Proxy({} as Record<string, unknown>, {
    get(_target, key) {
      if (key === "input") {
        return {
          on: (action: SurfaceInputAction, listener: () => void) => {
            const set = listeners.get(action) ?? new Set()
            set.add(listener)
            listeners.set(action, set)
            return () => set.delete(listener)
          },
        }
      }
      if (key === "gameActions") return () => []
      return (...args: unknown[]) => {
        calls.push(`${String(key)}(${args.map(arg => JSON.stringify(arg)).join(", ")})`)
      }
    },
  }) as unknown as SurfaceHost
  const press = (action: SurfaceInputAction) => act(() => {
    for (const listener of listeners.get(action) ?? []) listener()
  })
  return { host, calls, press }
}

const root = () => document.querySelector<HTMLElement>(".zone-screen")!
const lookRow = () => {
  const scroll = document.querySelector<HTMLElement>(".zone-options-scroll")!
  const button = within(scroll).getAllByText(byWords("Look")).map(node => node.closest("button")).find(node => node !== null)
  if (button === undefined || button === null) throw new Error("no LOOK row")
  return button
}
const sideways = (direction: "left" | "right") => {
  const focused = document.activeElement
  if (!(focused instanceof HTMLElement) || !focused.hasAttribute("data-korri-horizontal-control")) {
    throw new Error(`${direction} on a control that does not step: the portal would move focus`)
  }
  act(() => {
    focused.dispatchEvent(new CustomEvent("korri-semantic-direction", { detail: { direction, repeat: false } }))
  })
}

/** From home, `system` and the d-pad to SYSTEM's LOOK row. */
function onLookRow(): Recorded {
  const recorded = recordingHost()
  render(<ZoneSurface host={recorded.host} model={MODEL} />)
  recorded.press("system")
  act(() => lookRow().focus())
  expect(document.activeElement).toBe(lookRow())
  return recorded
}

describe("OPTIONS › SYSTEM › LOOK", () => {
  test("SYSTEM holds LOOK after Korri's actions and before BUILD, its value the look in use, its choices the three", () => {
    const plan = optionsPlan({ ...MODEL, look: "retro" })
    const system = plan.groups.find(group => group.key === ZONE_OPTIONS_SYSTEM)!
    expect(system.rows.map(row => row.item.label)).toEqual(["Pair controller", "Look", "Build"])
    const look = system.rows.find(row => row.id === ZONE_SYSTEM_LOOK)!
    expect(look.item.value).toBe("RETRO")
    expect(look.item.interaction).toEqual({
      kind: "choice",
      choices: [{ value: "modern", label: "MODERN" }, { value: "blend", label: "BLEND" }, { value: "retro", label: "RETRO" }],
    })
    // No look given (a part with no Zone behind it): no row.
    expect(optionsPlan(MODEL).rows.some(row => row.id === ZONE_SYSTEM_LOOK)).toBe(false)
  })

  test("SYSTEM stands for LOOK alone when Korri publishes no action and no build", () => {
    const plan = optionsPlan({ ...MODEL, actions: [], buildLabel: undefined, look: "blend" })
    expect(plan.groups.map(group => group.key)).toEqual([ZONE_OPTIONS_SYSTEM])
  })

  test("the row is a horizontal control the portal steps; Zone starts in the kept look", () => {
    localStorage.setItem(ZONE_LOOK_KEY, "modern")
    onLookRow()
    expect(root().dataset.look).toBe("modern")
    expect(lookRow().getAttribute("data-korri-horizontal-control")).toBe("choice")
    expect(lookRow().textContent).toContain("MODERN")
  })

  test("right steps toward retro and round, at once, the cursor staying on the row", () => {
    const { calls } = onLookRow()
    expect(root().dataset.look).toBe("blend")
    sideways("right")
    expect(root().dataset.look).toBe("retro")
    expect(lookRow().textContent).toContain("RETRO")
    expect(document.activeElement).toBe(lookRow())
    sideways("right")
    expect(root().dataset.look).toBe("modern")
    expect(lookRow().textContent).toContain("MODERN")
    expect(document.activeElement).toBe(lookRow())
    expect(calls).toEqual([])
  })

  test("left steps toward modern and round, at once, the cursor staying on the row", () => {
    const { calls } = onLookRow()
    sideways("left")
    expect(root().dataset.look).toBe("modern")
    expect(document.activeElement).toBe(lookRow())
    sideways("left")
    expect(root().dataset.look).toBe("retro")
    expect(lookRow().textContent).toContain("RETRO")
    sideways("left")
    expect(root().dataset.look).toBe("blend")
    expect(document.activeElement).toBe(lookRow())
    expect(calls).toEqual([])
  })

  test("A steps it forward, as on every choice", () => {
    const { calls } = onLookRow()
    act(() => lookRow().click())
    expect(root().dataset.look).toBe("retro")
    expect(document.activeElement).toBe(lookRow())
    expect(calls).toEqual([])
  })

  test("the device keeps every step, and a new mount opens in it", () => {
    onLookRow()
    sideways("left")
    expect(localStorage.getItem(ZONE_LOOK_KEY)).toBe("modern")
    sideways("left")
    expect(localStorage.getItem(ZONE_LOOK_KEY)).toBe("retro")
    cleanup()
    render(<ZoneSurface host={recordingHost().host} model={MODEL} />)
    expect(root().dataset.look).toBe("retro")
  })

  test("the surface's update routes the row to the look, never to a setting", () => {
    const start = zoneSurfaceStart(MODEL, "blend")
    const step = update(start, { _tag: "Options", message: { _tag: "SteppedRow", rowId: ZONE_SYSTEM_LOOK, direction: -1 } }, MODEL)
    expect(step.model.look).toBe("modern")
    expect(step.requests).toEqual([{ _tag: "RememberLook", look: "modern" }])
  })
})
