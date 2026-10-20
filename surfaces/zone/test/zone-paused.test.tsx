/**
 * PAUSED (C24): the pure overlay model, then the page on the real model
 * with a fixture host recording every control Korri was asked to invoke.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceGameplayOverlayPresentation } from "@contracts/surface/korri-surface"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { useSyncExternalStore } from "react"
import {
  controlFill,
  controlFilter,
  controlReset,
  controlRestart,
  controlResume,
  controlSave,
  controlVolume,
  overlayDefault,
  overlayGroups,
  overlayMany,
  overlayNoTitle,
} from "../src/fixtures/fixture-layers"
import { sendFixtureDirection } from "../src/fixtures/fixture-directions"
import { createOverlayProgram } from "../src/fixtures/layer-program"
import { ZonePaused } from "../src/pages/ZonePaused"
import {
  rangeStep,
  shownRangeValue,
  updateOverlay,
  ZONE_OVERLAY_START,
  type ZoneOverlayMessage,
  type ZoneOverlayModel,
} from "../src/state/overlay"
import { byWords } from "./words"

afterEach(cleanup)

const every: SurfaceGameplayOverlayPresentation = {
  kind: "gameplay-overlay",
  title: "Paused",
  controls: [controlResume, controlRestart],
  groups: [
    { id: "display", label: "Display", controls: [controlFill, controlFilter] },
    { id: "sound", label: "Sound", controls: [controlVolume, controlSave, controlReset] },
  ],
}

const open = (overlay = every): ZoneOverlayModel =>
  updateOverlay(ZONE_OVERLAY_START, { _tag: "KorriPublished" }, overlay).model
const send = (model: ZoneOverlayModel, message: ZoneOverlayMessage, overlay = every) => updateOverlay(model, message, overlay)

describe("updateOverlay", () => {
  test("KorriPublished: the cursor opens on the first control; a vanished one goes back to it", () => {
    expect(open().focusedId).toBe("resume")
    const gone = { ...open(), focusedId: "nope" }
    expect(send(gone, { _tag: "KorriPublished" }).model.focusedId).toBe("resume")
    expect(open({ kind: "gameplay-overlay", controls: [], groups: [] }).focusedId).toBeUndefined()
  })

  test("KorriPublished: a range stops moving once Korri shows its value, and a press waits no longer", () => {
    const moving = { ...open(), moving: { volume: 65 }, leaving: true }
    expect(send(moving, { _tag: "KorriPublished" }).model).toMatchObject({ moving: { volume: 65 }, leaving: false })
    const caughtUp: SurfaceGameplayOverlayPresentation = {
      ...every,
      groups: [{ id: "sound", label: "Sound", controls: [{ ...controlVolume, interaction: { kind: "range", value: 65, min: 0, max: 100, step: 5 } }] }],
    }
    expect(send(moving, { _tag: "KorriPublished" }, caughtUp).model.moving).toEqual({})
  })

  test("KorriPublished: a question about a control Korri disabled is withdrawn", () => {
    const asking = { ...open(), asking: { controlId: "restart" }, answer: 1 as const }
    const disabled = { ...every, controls: [controlResume, { ...controlRestart, enabled: false }] }
    expect(send(asking, { _tag: "KorriPublished" }, disabled).model).toMatchObject({ asking: undefined, answer: 0 })
  })

  test("PressedBack withdraws a question, else asks to return to the game", () => {
    const asking = { ...open(), asking: { controlId: "restart" } }
    expect(send(asking, { _tag: "PressedBack" })).toEqual({ model: { ...asking, asking: undefined, answer: 0 }, requests: [] })
    expect(send(open(), { _tag: "PressedBack" }).requests).toEqual([{ _tag: "DismissGameplayOverlay" }])
  })

  test("FocusedControl moves the cursor; leaving a moving range hands it back to Korri", () => {
    const onVolume = { ...open(), focusedId: "volume", moving: { volume: 75 } }
    expect(send(onVolume, { _tag: "FocusedControl", controlId: "fill-screen" }).model).toMatchObject({
      focusedId: "fill-screen",
      moving: {},
    })
    const asking = { ...open(), asking: { controlId: "restart" } }
    expect(send(asking, { _tag: "FocusedControl", controlId: "fill-screen" }).model.focusedId).toBe("resume")
  })

  test("PressedControl: a command is invoked with no value", () => {
    const step = send(open(), { _tag: "PressedControl", controlId: "resume" })
    expect(step.requests).toEqual([{ _tag: "InvokeGameplayControl", controlId: "resume" }])
  })

  test("PressedControl: dismissOnSuccess is the host's; Zone waits and sends no dismiss of its own", () => {
    const step = send(open(), { _tag: "PressedControl", controlId: "resume" })
    expect(step.model.leaving).toBe(true)
    expect(step.requests.some(request => request._tag === "DismissGameplayOverlay")).toBe(false)
    expect(send(step.model, { _tag: "PressedControl", controlId: "resume" }).requests).toEqual([])
    expect(send(step.model, { _tag: "SteppedControl", controlId: "fill-screen", direction: 1 }).requests).toEqual([])
  })

  test("PressedControl: A flips a toggle and steps a choice forward; a range ignores it", () => {
    expect(send(open(), { _tag: "PressedControl", controlId: "fill-screen" }).requests)
      .toEqual([{ _tag: "InvokeGameplayControl", controlId: "fill-screen", value: { kind: "toggle", value: true } }])
    expect(send(open(), { _tag: "PressedControl", controlId: "screen-filter" }).requests)
      .toEqual([{ _tag: "InvokeGameplayControl", controlId: "screen-filter", value: { kind: "choice", value: "scanlines" } }])
    const range = send(open(), { _tag: "PressedControl", controlId: "volume" })
    expect(range.requests).toEqual([])
    expect(range.model.focusedId).toBe("volume")
  })

  test("PressedControl: a disabled control takes the cursor and nothing else", () => {
    const step = send(open(), { _tag: "PressedControl", controlId: "save-state" })
    expect(step.requests).toEqual([])
    expect(step.model.focusedId).toBe("save-state")
  })

  test("PressedControl: a destructive control asks first, NO lit", () => {
    const step = send(open(), { _tag: "PressedControl", controlId: "restart" })
    expect(step.requests).toEqual([])
    expect(step.model).toMatchObject({ asking: { controlId: "restart" }, answer: 0 })
    expect(send(step.model, { _tag: "PressedControl", controlId: "resume" }).requests).toEqual([])
  })

  test("SteppedControl: a toggle flips either way", () => {
    expect(send(open(), { _tag: "SteppedControl", controlId: "fill-screen", direction: -1 }).requests)
      .toEqual([{ _tag: "InvokeGameplayControl", controlId: "fill-screen", value: { kind: "toggle", value: true } }])
  })

  test("SteppedControl: a choice steps to the next option and wraps", () => {
    expect(send(open(), { _tag: "SteppedControl", controlId: "screen-filter", direction: -1 }).requests)
      .toEqual([{ _tag: "InvokeGameplayControl", controlId: "screen-filter", value: { kind: "choice", value: "none" } }])
    const last = { ...every, groups: [{ id: "d", label: "D", controls: [{ ...controlFilter, interaction: { ...controlFilter.interaction, value: "scanlines" } as typeof controlFilter.interaction }] }] }
    expect(send(open(last), { _tag: "SteppedControl", controlId: "screen-filter", direction: 1 }, last).requests)
      .toEqual([{ _tag: "InvokeGameplayControl", controlId: "screen-filter", value: { kind: "choice", value: "none" } }])
  })

  test("SteppedControl: a range moves by its step, shows the value at once, and is invoked on each step", () => {
    const first = send(open(), { _tag: "SteppedControl", controlId: "volume", direction: 1 })
    expect(first.requests).toEqual([{ _tag: "InvokeGameplayControl", controlId: "volume", value: { kind: "range", value: 65 } }])
    expect(shownRangeValue(first.model, controlVolume)).toBe(65)
    const second = send(first.model, { _tag: "SteppedControl", controlId: "volume", direction: 1 })
    expect(second.requests).toEqual([{ _tag: "InvokeGameplayControl", controlId: "volume", value: { kind: "range", value: 70 } }])
  })

  test("SteppedControl: a range at its end, a command and a disabled control change nothing", () => {
    const top = { ...open(), moving: { volume: 100 } }
    expect(send(top, { _tag: "SteppedControl", controlId: "volume", direction: 1 }).requests).toEqual([])
    expect(send(open(), { _tag: "SteppedControl", controlId: "resume", direction: 1 }).requests).toEqual([])
    expect(send(open(), { _tag: "SteppedControl", controlId: "save-state", direction: 1 }).requests).toEqual([])
  })

  test("SteppedControl: a destructive toggle asks with the value it would send", () => {
    const step = send(open(), { _tag: "SteppedControl", controlId: "hard-mode", direction: 1 })
    expect(step.requests).toEqual([])
    expect(step.model.asking).toEqual({ controlId: "hard-mode", value: { kind: "toggle", value: true } })
  })

  test("FocusedAnswer lights NO or YES, only while asking", () => {
    const asking = { ...open(), asking: { controlId: "restart" } }
    expect(send(asking, { _tag: "FocusedAnswer", index: 1 }).model.answer).toBe(1)
    expect(send(open(), { _tag: "FocusedAnswer", index: 1 }).model.answer).toBe(0)
  })

  test("ChoseAnswer: YES invokes what was asked, NO only closes", () => {
    const asking = { ...open(), asking: { controlId: "hard-mode", value: { kind: "toggle", value: true } as const } }
    expect(send(asking, { _tag: "ChoseAnswer", yes: true })).toMatchObject({
      model: { asking: undefined },
      requests: [{ _tag: "InvokeGameplayControl", controlId: "hard-mode", value: { kind: "toggle", value: true } }],
    })
    expect(send(asking, { _tag: "ChoseAnswer", yes: false })).toMatchObject({ model: { asking: undefined }, requests: [] })
    expect(send(open(), { _tag: "ChoseAnswer", yes: true }).requests).toEqual([])
  })

  test("rangeStep keeps the step's precision and the bounds", () => {
    expect(rangeStep(0.1, 0, 1, 0.1, 1)).toBe(0.2)
    expect(rangeStep(0.2, 0, 1, 0.1, 1)).toBe(0.3)
    expect(rangeStep(0.95, 0, 1, 0.1, 1)).toBe(1)
    expect(rangeStep(0, 0, 1, 0.1, -1)).toBe(0)
  })
})

/** The page on the real model, Korri applying each value it is sent. */
function Harness({ program }: { readonly program: ReturnType<typeof createOverlayProgram> }) {
  const { model, context } = useSyncExternalStore(program.subscribe, program.get)
  return (
    <ZonePaused
      answer={model.answer}
      asking={model.asking}
      focusedId={model.focusedId}
      moving={model.moving}
      onAnswer={(yes) => program.dispatch({ _tag: "ChoseAnswer", yes })}
      onFocusAnswer={(index) => program.dispatch({ _tag: "FocusedAnswer", index })}
      onFocusControl={(controlId) => program.dispatch({ _tag: "FocusedControl", controlId })}
      onPressControl={(controlId) => program.dispatch({ _tag: "PressedControl", controlId })}
      onStepControl={(controlId, direction) => program.dispatch({ _tag: "SteppedControl", controlId, direction })}
      overlay={context}
    />
  )
}

const mount = (overlay: SurfaceGameplayOverlayPresentation) => {
  const program = createOverlayProgram(overlay)
  render(<Harness program={program} />)
  return program
}

const row = (label: string) => screen.getByRole("button", { name: new RegExp(`^${label}`) })

/** The portal's left and right, as spatial-focus.ts delivers them. */
const portalStep = (element: HTMLElement, direction: "left" | "right") =>
  act(() => {
    element.dispatchEvent(new CustomEvent("korri-semantic-direction", { detail: { direction } }))
  })

describe("ZonePaused", () => {
  test("C24: Korri's title in the band, or PAUSED when it gives none", () => {
    mount({ ...overlayDefault, title: "Celeste Classic" })
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Celeste Classic")
    cleanup()
    mount(overlayNoTitle)
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Paused")
  })

  test("C24: flat controls first, then each group under its label", () => {
    mount(overlayGroups)
    const labels = screen.getAllByRole("button").map(button => button.textContent)
    expect(labels).toEqual(["Continue", "Exit", "Fill screenOff", "Screen filterSharp", "Volume60"])
    const display = screen.getByRole("region", { name: "Display" })
    expect(within(display).getByText(byWords("Fill screen"))).toBeDefined()
    expect(within(screen.getByRole("region", { name: "Sound" })).getByText("Volume")).toBeDefined()
  })

  test("C24: the first control is lit and holds focus; B returns to the game", () => {
    const program = mount(overlayDefault)
    const resume = row("Continue")
    expect(resume.closest("[data-focused]")).not.toBeNull()
    expect(document.activeElement).toBe(resume)
    act(() => program.dispatch({ _tag: "PressedBack" }))
    expect(program.host.calls).toEqual(["dismissGameplayOverlay"])
  })

  test("C24: a command invokes; dismissOnSuccess sends no dismiss of its own", () => {
    const program = mount(overlayDefault)
    fireEvent.click(row("Continue"))
    expect(program.host.calls).toEqual(["control:resume"])
  })

  test("C24: a toggle shows Korri's labels and flips on left, right and A", () => {
    const program = mount(every)
    const fill = row("Fill screen")
    expect(within(fill).getByText("Off")).toBeDefined()
    portalStep(fill, "right")
    expect(program.host.calls).toEqual([`control:fill-screen:${JSON.stringify({ kind: "toggle", value: true })}`])
    expect(within(row("Fill screen")).getByText("On")).toBeDefined()
    fireEvent.click(row("Fill screen"))
    expect(program.host.calls.at(-1)).toBe(`control:fill-screen:${JSON.stringify({ kind: "toggle", value: false })}`)
  })

  test("C24: a choice steps on the portal's left and right and shows the option's label", () => {
    const program = mount(every)
    const filter = row("Screen filter")
    act(() => {
      fireEvent.focus(filter)
    })
    act(() => sendFixtureDirection(filter, "right"))
    expect(program.host.calls).toEqual([`control:screen-filter:${JSON.stringify({ kind: "choice", value: "scanlines" })}`])
    expect(within(row("Screen filter")).getByText("Scanlines")).toBeDefined()
  })

  test("C24: a range steps by its step within min and max, its value shown as it moves", () => {
    const program = mount(every)
    const volume = row("Volume")
    expect(within(volume).getByText("60")).toBeDefined()
    expect(volume.getAttribute("data-korri-horizontal-control")).toBe("range")
    portalStep(volume, "left")
    expect(program.host.calls).toEqual([`control:volume:${JSON.stringify({ kind: "range", value: 55 })}`])
    expect(within(row("Volume")).getByText("55")).toBeDefined()
  })

  test("C24: a disabled control stays reachable, dims, says why and does nothing", () => {
    const program = mount(every)
    const save = row("Save state")
    expect(save.getAttribute("aria-disabled")).toBe("true")
    expect(save.hasAttribute("data-korri-horizontal-control")).toBe(false)
    expect(screen.getByText(byWords("This game does not support save states"))).toBeDefined()
    fireEvent.click(save)
    expect(program.host.calls).toEqual([])
  })

  test("C24: a destructive control asks NO / YES with NO lit; NO sends nothing, YES invokes", () => {
    const program = mount(every)
    fireEvent.click(row("Restart"))
    const dialog = screen.getByRole("dialog", { name: "Restart" })
    const [no, yes] = within(dialog).getAllByRole("button")
    expect([no?.textContent, yes?.textContent]).toEqual(["No", "Yes"])
    expect(no?.getAttribute("data-focused")).toBe("true")
    expect(document.activeElement).toBe(no ?? null)
    fireEvent.click(no!)
    expect(program.host.calls).toEqual([])
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(document.activeElement?.textContent).toBe("Restart")
    fireEvent.click(row("Restart"))
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Yes" }))
    expect(program.host.calls).toEqual(["control:restart"])
  })

  test("C24: a destructive toggle's question names the value it would take", () => {
    mount(every)
    portalStep(row("Hard mode"), "right")
    const dialog = screen.getByRole("dialog", { name: "Hard mode" })
    expect(within(dialog).getByText(byWords("Restarts the level"))).toBeDefined()
    expect(within(dialog).getByText("On")).toBeDefined()
  })

  test("C24: many controls all render, the long label whole", () => {
    mount(overlayMany)
    expect(screen.getByText(byWords("Cheat codes for the second quest and the linked game"))).toBeDefined()
    expect(screen.getByText(byWords("The Legend of Zelda: Oracle of Seasons"))).toBeDefined()
    expect(screen.getAllByRole("button").length).toBe(11)
  })
})

describe("the layers' lettering reaches contrast", () => {
  const source = readFileSync(join(import.meta.dir, "..", "src", "zone-tokens.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "")
  const tokens = new Map([...source.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)].map(([, name, value]) => [name ?? "", (value ?? "").trim()]))
  const resolve = (value: string): string => {
    const reference = value.match(/^var\((--[a-z0-9-]+)\)$/)
    return reference === null ? value : resolve(tokens.get(reference[1] ?? "") ?? "")
  }
  const luminance = (hex: string) => {
    const [r = 0, g = 0, b = 0] = [1, 3, 5].map(at => Number.parseInt(hex.slice(at, at + 2), 16) / 255)
      .map(c => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b
  }
  const contrast = (ink: string, ground: string) => {
    const [hi = 0, lo = 0] = [luminance(resolve(`var(${ink})`)), luminance(resolve(`var(${ground})`))].sort((a, b) => b - a)
    return (hi + 0.05) / (lo + 0.05)
  }

  test.each([
    ["--zone-paused-note-ink", "--zone-paused-wedge", 4.5],
    ["--zone-paused-moving-ink", "--zone-panel", 3],
    ["--zone-destructive-ink", "--zone-panel", 3],
    ["--zone-cap", "--zone-panel-blue", 3],
    ["--zone-dialog-title-ink", "--zone-panel", 3],
    ["--zone-slab-ink", "--zone-slab", 4.5],
  ] as const)("%s on %s reaches %d:1", (ink, ground, floor) => {
    expect(contrast(ink, ground)).toBeGreaterThanOrEqual(floor)
  })
})
