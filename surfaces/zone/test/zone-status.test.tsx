/**
 * The status layer (C7 to C10, C31, C32): the pure model, then the organism on the
 * real model with a fixture host recording what Korri was asked.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceStatus } from "@contracts/surface/korri-surface"
import { useRef, useSyncExternalStore } from "react"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import {
  fixtureTitleOf,
  statusBrowsing,
  statusBusyCancel,
  statusBusyDetail,
  statusBusyKicker,
  statusBusyTwo,
  statusConfirm,
  statusConfirmForGame,
  statusConfirmThree,
  statusEnded,
  statusPaused,
  statusProblem,
  statusProblemNoRetry,
  statusProgressKnown,
  statusProgressNearlyDone,
  statusProgressNoTotal,
  statusProgressUnpacking,
  statusRunning,
} from "../src/fixtures/fixture-layers"
import { createStatusProgram } from "../src/fixtures/layer-program"
import {
  problemButtons,
  shownFor,
  statusTakesFocus,
  updateStatus,
  ZONE_STATUS_START,
  type ZoneStatusModel,
} from "../src/state/status"
import { ZoneStatusLayer } from "../src/ui/organisms/ZoneStatusLayer"
import { byWords } from "./words"

afterEach(cleanup)

const settled = (status: SurfaceStatus, model: ZoneStatusModel = ZONE_STATUS_START) =>
  updateStatus(model, { _tag: "KorriPublished" }, status).model

describe("updateStatus", () => {
  test("KorriPublished: the layer follows Korri's status", () => {
    expect(settled(statusBrowsing).shown).toBe("none")
    expect(settled(statusBusyCancel).shown).toBe("busy")
    expect(settled(statusRunning).shown).toBe("running")
    expect(settled(statusProblem).shown).toBe("problem")
    expect(updateStatus(ZONE_STATUS_START, { _tag: "KorriPublished" }, statusBrowsing).requests).toEqual([])
  })

  test("KorriPublished: a layer that changes opens with its first button lit", () => {
    const onRetry = { shown: "problem", focus: 1 } as const
    expect(settled(statusBusyTwo, onRetry)).toEqual({ shown: "busy", focus: 0 })
  })

  test("KorriPublished: a focus past the buttons Korri now offers comes back in", () => {
    const onStop = { shown: "busy", focus: 1 } as const
    expect(settled(statusBusyCancel, onStop).focus).toBe(0)
    expect(settled(statusBusyTwo, onStop)).toBe(onStop)
  })

  test("PressedBack dismisses a problem and never cancels busy work", () => {
    expect(updateStatus(settled(statusProblem), { _tag: "PressedBack" }, statusProblem).requests)
      .toEqual([{ _tag: "DismissProblem" }])
    expect(updateStatus(settled(statusBusyCancel), { _tag: "PressedBack" }, statusBusyCancel).requests).toEqual([])
    expect(updateStatus(settled(statusRunning), { _tag: "PressedBack" }, statusRunning).requests).toEqual([])
    expect(updateStatus(ZONE_STATUS_START, { _tag: "PressedBack" }, statusBrowsing).requests).toEqual([])
  })

  test("FocusedStatusButton moves the lit button", () => {
    expect(updateStatus(settled(statusProblem), { _tag: "FocusedStatusButton", index: 1 }, statusProblem).model.focus).toBe(1)
    expect(updateStatus(settled(statusProblem), { _tag: "FocusedStatusButton", index: 5 }, statusProblem).model.focus).toBe(1)
  })

  test("ChoseBusyAction runs Korri's action by its id, and only an enabled one", () => {
    const busy = settled(statusBusyTwo)
    expect(updateStatus(busy, { _tag: "ChoseBusyAction", actionId: "stop" }, statusBusyTwo).requests)
      .toEqual([{ _tag: "RunAction", actionId: "stop" }])
    const disabled: SurfaceStatus = { _tag: "Busy", kicker: "Starting", actions: [{ id: "cancel", label: "Cancel", enabled: false }] }
    expect(updateStatus(settled(disabled), { _tag: "ChoseBusyAction", actionId: "cancel" }, disabled).requests).toEqual([])
    expect(updateStatus(busy, { _tag: "ChoseBusyAction", actionId: "nope" }, statusBusyTwo).requests).toEqual([])
    expect(updateStatus(settled(statusProblem), { _tag: "ChoseBusyAction", actionId: "cancel" }, statusProblem).requests).toEqual([])
  })

  test("ChoseProblemButton: DISMISS dismisses, RETRY retries only when Korri can", () => {
    expect(updateStatus(settled(statusProblem), { _tag: "ChoseProblemButton", button: "dismiss" }, statusProblem).requests)
      .toEqual([{ _tag: "DismissProblem" }])
    expect(updateStatus(settled(statusProblem), { _tag: "ChoseProblemButton", button: "retry" }, statusProblem).requests)
      .toEqual([{ _tag: "Retry" }])
    expect(updateStatus(settled(statusProblemNoRetry), { _tag: "ChoseProblemButton", button: "retry" }, statusProblemNoRetry).requests)
      .toEqual([])
    expect(updateStatus(settled(statusBusyCancel), { _tag: "ChoseProblemButton", button: "dismiss" }, statusBusyCancel).requests)
      .toEqual([])
  })

  test("C31 KorriPublished: Confirm is its own layer, its first action lit, the focus brought in", () => {
    expect(settled(statusConfirm)).toEqual({ shown: "confirm", focus: 0 })
    expect(shownFor(statusConfirm)).toBe("confirm")
    expect(statusTakesFocus(statusConfirm)).toBe(true)
    const onThird = { shown: "confirm", focus: 2 } as const
    expect(settled(statusConfirmThree, onThird)).toBe(onThird)
    expect(settled(statusConfirm, onThird).focus).toBe(1)
    expect(settled(statusConfirm, { shown: "busy", focus: 1 })).toEqual({ shown: "confirm", focus: 0 })
    const none: SurfaceStatus = { _tag: "Confirm", kicker: "Needs a download", actions: [] }
    expect(statusTakesFocus(none)).toBe(false)
    expect(settled(none)).toEqual({ shown: "confirm", focus: 0 })
    // An inert first answer cannot hold the cursor: the first enabled one is lit.
    const inertFirst: SurfaceStatus = {
      _tag: "Confirm",
      kicker: "Needs a download",
      actions: [{ id: "go", label: "Download", enabled: false }, { id: "no", label: "Cancel", enabled: true }],
    }
    expect(settled(inertFirst)).toEqual({ shown: "confirm", focus: 1 })
  })

  test("C31 ChoseConfirmAction runs Korri's action by its id, and only an enabled one of the question", () => {
    const asked = settled(statusConfirmThree)
    expect(updateStatus(asked, { _tag: "ChoseConfirmAction", actionId: "acquisition:1:remote" }, statusConfirmThree).requests)
      .toEqual([{ _tag: "RunAction", actionId: "acquisition:1:remote" }])
    expect(updateStatus(asked, { _tag: "ChoseConfirmAction", actionId: "nope" }, statusConfirmThree).requests).toEqual([])
    const inert: SurfaceStatus = { _tag: "Confirm", kicker: "Needs a download", actions: [{ id: "go", label: "Download", enabled: false }] }
    expect(updateStatus(settled(inert), { _tag: "ChoseConfirmAction", actionId: "go" }, inert).requests).toEqual([])
    // Not asked any more: nothing runs.
    expect(updateStatus(settled(statusBusyCancel), { _tag: "ChoseConfirmAction", actionId: "cancel" }, statusBusyCancel).requests)
      .toEqual([])
    expect(updateStatus(asked, { _tag: "ChoseBusyAction", actionId: "acquisition:1:cancel" }, statusConfirmThree).requests)
      .toEqual([])
  })

  test("C31 PressedBack answers nothing: Korri's actions are the only ways out", () => {
    const step = updateStatus(settled(statusConfirm), { _tag: "PressedBack" }, statusConfirm)
    expect(step.requests).toEqual([])
    expect(step.model).toEqual(settled(statusConfirm))
  })

  test("the problem dialog offers RETRY only when Korri can; focus belongs in a layer with a button", () => {
    expect(problemButtons(statusProblem)).toEqual(["dismiss", "retry"])
    expect(problemButtons(statusProblemNoRetry)).toEqual(["dismiss"])
    expect(shownFor(statusBusyKicker)).toBe("busy")
    expect(statusTakesFocus(statusBusyKicker)).toBe(false)
    expect(statusTakesFocus(statusBusyCancel)).toBe(true)
    expect(statusTakesFocus(statusRunning)).toBe(false)
    expect(statusTakesFocus(statusProblemNoRetry)).toBe(true)
  })
})

describe("stream play: paused and ended", () => {
  test("Paused is its own layer on its first enabled action; Back leaves nothing, LEAVE runs by its id", () => {
    expect(settled(statusPaused)).toEqual({ shown: "paused", focus: 0 })
    expect(statusTakesFocus(statusPaused)).toBe(true)
    const paused = settled(statusPaused)
    expect(updateStatus(paused, { _tag: "PressedBack" }, statusPaused).requests).toEqual([])
    expect(updateStatus(paused, { _tag: "ChosePausedAction", actionId: "stream:leave" }, statusPaused).requests)
      .toEqual([{ _tag: "RunAction", actionId: "stream:leave" }])
    expect(updateStatus(paused, { _tag: "ChosePausedAction", actionId: "nope" }, statusPaused).requests).toEqual([])
    expect(updateStatus(settled(statusConfirm), { _tag: "ChosePausedAction", actionId: "stream:leave" }, statusConfirm).requests).toEqual([])
  })

  test("Ended: DISMISS and Back both acknowledge it; DISMISS does nothing once it is gone", () => {
    expect(settled(statusEnded)).toEqual({ shown: "ended", focus: 0 })
    const ended = settled(statusEnded)
    expect(updateStatus(ended, { _tag: "PressedBack" }, statusEnded).requests).toEqual([{ _tag: "DismissProblem" }])
    expect(updateStatus(ended, { _tag: "ChoseEndedDismiss" }, statusEnded).requests).toEqual([{ _tag: "DismissProblem" }])
    expect(updateStatus(settled(statusBrowsing), { _tag: "ChoseEndedDismiss" }, statusBrowsing).requests).toEqual([])
  })
})

/** The organism on the real model, Korri answering, over a page with one button. */
function Harness({ program }: { readonly program: ReturnType<typeof createStatusProgram> }) {
  const { model, context } = useSyncExternalStore(program.subscribe, program.get)
  const page = useRef(<button type="button">Page card</button>)
  return (
    <ZoneStatusLayer
      focus={model.focus}
      onBusyAction={(actionId) => program.dispatch({ _tag: "ChoseBusyAction", actionId })}
      onConfirmAction={(actionId) => program.dispatch({ _tag: "ChoseConfirmAction", actionId })}
      onEndedDismiss={() => program.dispatch({ _tag: "ChoseEndedDismiss" })}
      onPausedAction={(actionId) => program.dispatch({ _tag: "ChosePausedAction", actionId })}
      onFocusButton={(index) => program.dispatch({ _tag: "FocusedStatusButton", index })}
      onProblemButton={(button) => program.dispatch({ _tag: "ChoseProblemButton", button })}
      page={page.current}
      status={context}
      titleOf={fixtureTitleOf}
    />
  )
}

const mount = (status: SurfaceStatus) => {
  const program = createStatusProgram(status)
  render(<Harness program={program} />)
  return program
}

const pageOf = () => screen.getByText(byWords("Page card")).closest("[inert]")

describe("ZoneStatusLayer", () => {
  test("Browsing draws nothing over the page", () => {
    mount(statusBrowsing)
    expect(screen.getByRole("button", { name: "Page card" })).toBeDefined()
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(screen.queryByRole("status")).toBeNull()
    expect(pageOf()).toBeNull()
  })

  test("C7: Busy shows Korri's kicker, its detail and the game; the page goes inert", () => {
    mount(statusBusyDetail)
    const strip = screen.getByRole("status")
    expect(within(strip).getByText("Starting")).toBeDefined()
    expect(within(strip).getByText(byWords("Waiting for zao to answer"))).toBeDefined()
    expect(screen.getByText("Spelunky")).toBeDefined()
    expect(within(strip).queryByRole("button")).toBeNull()
    expect(pageOf()).not.toBeNull()
  })

  test("C7: Busy with the kicker alone shows no game and no button", () => {
    mount(statusBusyKicker)
    expect(screen.getByText("Starting")).toBeDefined()
    expect(screen.queryByText("Spelunky")).toBeNull()
    expect(screen.getAllByRole("button").map(button => button.textContent)).toEqual(["Page card"])
  })

  test("C7: Korri's CANCEL is lit, takes focus, and runs its action", () => {
    const program = mount(statusBusyCancel)
    const cancel = screen.getByRole("button", { name: "Cancel" })
    expect(cancel.getAttribute("data-focused")).toBe("true")
    expect(document.activeElement).toBe(cancel)
    fireEvent.click(cancel)
    expect(program.host.calls).toEqual(["action:cancel"])
  })

  test("C7: two actions; focusing STOP lights it and A runs it", () => {
    const program = mount(statusBusyTwo)
    expect(screen.getByText(byWords("Copying save"))).toBeDefined()
    expect(screen.getByText(byWords("Wario Land 4"))).toBeDefined()
    const stop = screen.getByRole("button", { name: "Stop" })
    act(() => {
      fireEvent.focus(stop)
    })
    expect(program.get().model.focus).toBe(1)
    expect(screen.getByRole("button", { name: "Stop" }).getAttribute("data-focused")).toBe("true")
    fireEvent.click(stop)
    expect(program.host.calls).toEqual(["action:stop"])
  })

  test("C8: Running shows the kicker, OPEN and the game's title, takes no focus and leaves the page usable", () => {
    mount(statusRunning)
    const strip = screen.getByRole("status")
    expect(within(strip).getByText(byWords("Playing on zao"))).toBeDefined()
    expect(within(strip).getByText("Open")).toBeDefined()
    expect(within(strip).getByText(byWords("The Legend of Zelda: Oracle of Seasons"))).toBeDefined()
    expect(within(strip).queryByRole("button")).toBeNull()
    expect(pageOf()).toBeNull()
  })

  test("C9: Problem states Korri's kicker, reason and the game, DISMISS lit before RETRY", () => {
    mount(statusProblem)
    const dialog = screen.getByRole("dialog", { name: "Could not start" })
    expect(within(dialog).getByText(byWords("zao did not answer in time. Check that it is awake and on the same network."))).toBeDefined()
    expect(within(dialog).getByText("Spelunky")).toBeDefined()
    const buttons = within(dialog).getAllByRole("button")
    expect(buttons.map(button => button.textContent)).toEqual(["Dismiss", "Retry"])
    expect(buttons[0]?.getAttribute("data-focused")).toBe("true")
    expect(document.activeElement).toBe(buttons[0] ?? null)
    expect(pageOf()).not.toBeNull()
  })

  test("C9: RETRY asks Korri to retry, and Korri's answer replaces the dialog", () => {
    const program = mount(statusProblem)
    fireEvent.click(screen.getByRole("button", { name: "Retry" }))
    expect(program.host.calls).toEqual(["retry"])
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(screen.getByText("Starting")).toBeDefined()
  })

  test("C9: DISMISS dismisses, and focus goes back to the page", () => {
    const program = createStatusProgram(statusBrowsing)
    const view = render(<Harness program={program} />)
    const card = screen.getByRole("button", { name: "Page card" })
    card.focus()
    expect(document.activeElement).toBe(card)
    const problem = createStatusProgram(statusProblemNoRetry)
    view.rerender(<Harness program={problem} />)
    expect(screen.getAllByRole("button", { name: /Dismiss|Retry/ }).map(button => button.textContent)).toEqual(["Dismiss"])
    expect(document.activeElement?.textContent).toBe("Dismiss")
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }))
    expect(problem.host.calls).toEqual(["dismiss"])
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(document.activeElement?.textContent).toBe("Page card")
  })

  test("C31: Confirm asks Korri's question: the kicker as headline, the detail, the host's actions in order, the first lit", () => {
    mount(statusConfirmThree)
    const dialog = screen.getByRole("dialog", { name: "Needs a download" })
    expect(within(dialog).getByText(byWords("Proton-CachyOS, size unknown. This device is on a metered connection."))).toBeDefined()
    const buttons = within(dialog).getAllByRole("button")
    expect(buttons.map(button => button.textContent)).toEqual(["Download", "Play on zao instead", "Cancel"])
    expect(buttons[0]?.getAttribute("data-focused")).toBe("true")
    expect(document.activeElement).toBe(buttons[0] ?? null)
    expect(pageOf()).not.toBeNull()
    expect(within(dialog).queryByText("Spelunky")).toBeNull()
  })

  test("C31: the game the question belongs to stands in yellow", () => {
    mount(statusConfirmForGame)
    const dialog = screen.getByRole("dialog", { name: "Needs a download" })
    const title = within(dialog).getByText(byWords("Hollow Knight")).closest<HTMLElement>(".zone-dialog-title")
    expect(title).not.toBeNull()
    // Each drawn word is HUD caps.
    const words = [...title!.querySelectorAll(".zone-caps")]
    expect(words.map(word => word.textContent)).toEqual(["Hollow", "Knight"])
    expect(words.every(word => word.getAttribute("data-ink") === "hud")).toBe(true)
  })

  test("C31: any action, the last as well, is reported by its id; none is treated as no", () => {
    const program = mount(statusConfirmThree)
    const cancel = screen.getByRole("button", { name: "Cancel" })
    act(() => {
      fireEvent.focus(cancel)
    })
    expect(program.get().model.focus).toBe(2)
    expect(screen.getByRole("button", { name: "Cancel" }).getAttribute("data-focused")).toBe("true")
    fireEvent.click(cancel)
    expect(program.host.calls).toEqual(["action:acquisition:1:cancel"])
  })

  test("C31: an inert action is drawn dimmed and runs nothing", () => {
    const inert: SurfaceStatus = {
      _tag: "Confirm",
      kicker: "Needs a download",
      actions: [{ id: "go", label: "Download", enabled: false }, { id: "no", label: "Cancel", enabled: true, destructive: true }],
    }
    const program = mount(inert)
    const go = screen.getByRole("button", { name: "Download" })
    expect(go.hasAttribute("disabled")).toBe(true)
    fireEvent.click(go)
    expect(program.host.calls).toEqual([])
    expect(screen.getByRole("button", { name: "Cancel" }).hasAttribute("data-destructive")).toBe(true)
  })

  test("C31: focus moves into the question and back to the page when Korri answers", () => {
    const program = createStatusProgram(statusBrowsing)
    const view = render(<Harness program={program} />)
    screen.getByRole("button", { name: "Page card" }).focus()
    const asking = createStatusProgram(statusConfirm)
    view.rerender(<Harness program={asking} />)
    expect(document.activeElement?.textContent).toBe("Download")
    fireEvent.click(screen.getByRole("button", { name: "Download" }))
    expect(asking.host.calls).toEqual(["action:acquisition:1:download"])
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(document.activeElement?.textContent).toBe("Page card")
  })

  test("C32: a known total is a real bar: its share, and the figures once, Korri's detail at its end", () => {
    mount(statusProgressKnown)
    const strip = screen.getByRole("status")
    const bar = within(strip).getByRole("progressbar", { name: "Downloading FEX rootfs" })
    expect(bar.getAttribute("data-meter")).toBe("progress")
    expect(bar.getAttribute("aria-valuenow")).toBe("52")
    expect(bar.getAttribute("aria-valuetext")).toBe("612.0 MB / 1.2 GB")
    expect(bar.style.getPropertyValue("--zone-busy-strip-share")).toBe(String(612_000_000 / 1_160_000_000))
    // Korri's detail, verbatim, is the meter's one readout: Zone draws no figures of its own beside it.
    expect(bar.textContent).toBe("612 MB of 1.2 GB · 6.4 MB/s · 1:30 left")
    expect(strip.querySelectorAll(".zone-busy-strip-detail")).toHaveLength(1)
    expect(strip.querySelector(".zone-busy-strip-figures")).toBeNull()
    expect(screen.getByRole("button", { name: "Cancel" }).getAttribute("data-focused")).toBe("true")
  })

  test("C32: with no detail of Korri's, the meter's readout is Zone's DONE / TOTAL", () => {
    mount({ ...statusProgressNearlyDone, detail: undefined } as SurfaceStatus)
    const bar = screen.getByRole("progressbar")
    expect(bar.getAttribute("aria-valuenow")).toBe("99")
    expect(bar.textContent).toBe("1.2 GB/1.2 GB")
  })

  test("C32: done without a total claims no share: the size so far (Korri's words, else Zone's) and the moving stripe", () => {
    mount(statusProgressNoTotal)
    const bar = screen.getByRole("progressbar")
    expect(bar.getAttribute("data-meter")).toBe("indeterminate")
    expect(bar.hasAttribute("aria-valuenow")).toBe(false)
    expect(bar.getAttribute("aria-valuetext")).toBe("212.4 MB")
    expect(bar.textContent).toBe("212 MB so far · 6.4 MB/s")
    cleanup()
    mount({ ...statusProgressNoTotal, detail: undefined } as SurfaceStatus)
    expect(screen.getByRole("progressbar").textContent).toBe("212.4 MB")
  })

  test("C32: the busy strip stands at the page's foot, clear of the band's cards", () => {
    mount(statusProgressKnown)
    expect(document.querySelector(".zone-status-layer-busy")?.getAttribute("data-at")).toBe("foot")
  })

  test("C32: nothing done and no end (unpacking) is the stripe alone; no progress is no bar", () => {
    mount(statusProgressUnpacking)
    const bar = screen.getByRole("progressbar")
    expect(bar.getAttribute("data-meter")).toBe("indeterminate")
    expect(bar.textContent).toBe("")
    cleanup()
    mount(statusBusyCancel)
    expect(screen.queryByRole("progressbar")).toBeNull()
  })

  test("C10: the problem dialog never offers RELOAD", () => {
    mount(statusProblem)
    expect(screen.queryByRole("button", { name: "Reload" })).toBeNull()
  })
})

describe("the veil covers the whole page", () => {
  const read = (...path: string[]) => readFileSync(join(import.meta.dir, "..", "src", ...path), "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "")
  const css = read("ui", "organisms", "ZoneStatusLayer.css")
  test("the page's own layers are shut in a stacking context under the veil", () => {
    expect(css).toMatch(/\.zone-status-layer-page \{[^}]*isolation: isolate;/)
    expect(css).toMatch(/\.zone-status-layer-content \{[^}]*z-index: 0;[^}]*isolation: isolate;/)
    // The veil is drawn after the content, in the same stacking context: over it.
    mount(statusBusyDetail)
    const covered = document.querySelector(".zone-status-layer-page[data-covered]")!
    expect([...covered.children].map(child => child.className)).toEqual(["zone-status-layer-content", "zone-veil"])
  })
  test("the veil is a dither of single art pixels: a hard-stop checker, no opacity but Modern's", () => {
    const veil = read("ui", "atoms", "ZoneVeil.css")
    expect(veil).toMatch(/conic-gradient\(/)
    expect(veil).toMatch(/var\(--zone-veil-tile\) \* var\(--zone-px\)/)
    // The checker's own rule draws no opacity; the one see-through is the
    // Modern look's rule, which the authoring gate names.
    const checker = /\.zone-veil \{[^}]*\}/.exec(veil)?.[0] ?? ""
    expect(checker).toMatch(/conic-gradient/)
    expect(checker).not.toMatch(/opacity/)
    expect([...veil.matchAll(/([^{}]+)\{[^}]*opacity/g)].map(match => match[1]?.trim().split("\n").at(-1)))
      .toEqual(['[data-look="modern"] .zone-veil'])
    expect(read("zone-tokens.css")).toMatch(/--zone-veil-tile: 2;/)
  })
  test("busy over home: FIND is inside the covered, inert content", async () => {
    const games = await import("../src/pages/ZoneGames.page.part")
    render(<games.BusyOverHome />)
    const find = screen.getByRole("button", { name: /find/i })
    expect(find.closest(".zone-status-layer-content")).not.toBeNull()
    expect(find.closest(".zone-status-layer-page")?.hasAttribute("data-covered")).toBe(true)
    expect(find.closest("[inert]")).not.toBeNull()
  })
})

describe("ZoneStatusLayer, stream play", () => {
  test("Paused: Korri's kicker and detail over the inert page, LEAVE lit and run by its id", () => {
    const program = mount(statusPaused)
    const dialog = screen.getByRole("dialog", { name: "Paused on Odin 2 Portal" })
    expect(within(dialog).getByText(byWords("Turtles in Time goes on when Simon returns to it."))).toBeDefined()
    const leave = within(dialog).getByRole("button", { name: "Leave" })
    expect(document.activeElement).toBe(leave)
    expect(pageOf()).not.toBeNull()
    fireEvent.click(leave)
    expect(program.host.calls).toEqual(["action:stream:leave"])
  })

  test("Ended: Korri's reason and DISMISS, which calls dismiss", () => {
    const program = mount(statusEnded)
    const dialog = screen.getByRole("dialog", { name: "Session ended" })
    expect(within(dialog).getByText(byWords("Turtles in Time ended on Odin 2 Portal."))).toBeDefined()
    fireEvent.click(within(dialog).getByRole("button", { name: "Dismiss" }))
    expect(program.host.calls).toEqual(["dismiss"])
  })
})
