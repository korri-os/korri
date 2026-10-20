/**
 * The status layer (C7 to C10): the pure model, then the organism on the
 * real model with a fixture host recording what Korri was asked.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceStatus } from "@contracts/surface/korri-surface"
import { useRef, useSyncExternalStore } from "react"
import {
  fixtureTitleOf,
  statusBrowsing,
  statusBusyCancel,
  statusBusyDetail,
  statusBusyKicker,
  statusBusyTwo,
  statusProblem,
  statusProblemNoRetry,
  statusRunning,
} from "../src/fixtures/fixture-layers"
import { createFixtureHost } from "../src/fixtures/fixture-host"
import { createStatusProgram } from "../src/fixtures/layer-program"
import {
  problemButtons,
  reloadRequest,
  shownFor,
  statusTakesFocus,
  updateStatus,
  ZONE_STATUS_START,
  type ZoneStatusModel,
} from "../src/state/status"
import { ZoneStatusLayer } from "../src/ui/organisms/ZoneStatusLayer"
import { performZoneRequest } from "../src/zone-host"

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

  test("the problem dialog offers RETRY only when Korri can; focus belongs in a layer with a button", () => {
    expect(problemButtons(statusProblem)).toEqual(["dismiss", "retry"])
    expect(problemButtons(statusProblemNoRetry)).toEqual(["dismiss"])
    expect(shownFor(statusBusyKicker)).toBe("busy")
    expect(statusTakesFocus(statusBusyKicker)).toBe(false)
    expect(statusTakesFocus(statusBusyCancel)).toBe(true)
    expect(statusTakesFocus(statusRunning)).toBe(false)
    expect(statusTakesFocus(statusProblemNoRetry)).toBe(true)
  })

  test("C10: reloadRequest asks Korri to read everything again", () => {
    const host = createFixtureHost()
    expect(reloadRequest()).toEqual({ _tag: "Reload" })
    performZoneRequest(host, reloadRequest())
    expect(host.calls).toEqual(["reload"])
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

const pageOf = () => screen.getByText("Page card").closest("[inert]")

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
    expect(within(strip).getByText("Waiting for zao to answer")).toBeDefined()
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
    expect(screen.getByText("Copying save")).toBeDefined()
    expect(screen.getByText("Wario Land 4")).toBeDefined()
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
    expect(within(strip).getByText("Playing on zao")).toBeDefined()
    expect(within(strip).getByText("Open")).toBeDefined()
    expect(within(strip).getByText("The Legend of Zelda: Oracle of Seasons")).toBeDefined()
    expect(within(strip).queryByRole("button")).toBeNull()
    expect(pageOf()).toBeNull()
  })

  test("C9: Problem states Korri's kicker, reason and the game, DISMISS lit before RETRY", () => {
    mount(statusProblem)
    const dialog = screen.getByRole("dialog", { name: "Could not start" })
    expect(within(dialog).getByText("zao did not answer in time. Check that it is awake and on the same network.")).toBeDefined()
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

  test("C10: the problem dialog never offers RELOAD", () => {
    mount(statusProblem)
    expect(screen.queryByRole("button", { name: "Reload" })).toBeNull()
  })
})
