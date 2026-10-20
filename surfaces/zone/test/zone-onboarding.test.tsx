/**
 * First boot (C25): the pure onboarding model, the controller-card plan,
 * then the page on the real model with a fixture host recording every
 * request, and a pretend Korri joining networks and saving the name.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { useSyncExternalStore } from "react"
import {
  controllerPad,
  controllerPadWaiting,
  controllersFour,
  networkOffline,
  networkOnline,
  onboardingModel,
} from "../src/fixtures/fixture-layers"
import { createOnboardingProgram } from "../src/fixtures/layer-program"
import { ONBOARDING_TARGET, onboardingNetworkTarget, ZoneOnboarding } from "../src/pages/ZoneOnboarding"
import {
  planControllerCards,
  ZONE_CONTROLLER_CARD_GAP,
  ZONE_CONTROLLER_CARD_MAX,
  ZONE_CONTROLLER_CARD_MIN,
} from "../src/plan/onboarding"
import {
  canGoOn,
  deviceNameSetting,
  networkTarget,
  onboardingJoin,
  onboardingTargets,
  TARGET,
  updateOnboarding,
  ZONE_ONBOARDING_START,
  type ZoneOnboardingMessage,
  type ZoneOnboardingModel,
} from "../src/state/onboarding"

afterEach(cleanup)

const heard = onboardingModel([controllerPad])
const waiting = onboardingModel([controllerPadWaiting])
const online = onboardingModel([controllerPad], networkOnline)

const send = (model: ZoneOnboardingModel, message: ZoneOnboardingMessage, korri: SurfaceModel = heard) =>
  updateOnboarding(model, message, korri)
const at = (step: ZoneOnboardingModel["step"], korri: SurfaceModel = heard, extra: Partial<ZoneOnboardingModel> = {}) =>
  send({ ...ZONE_ONBOARDING_START, step, ...extra }, { _tag: "KorriPublished" }, korri).model

describe("updateOnboarding", () => {
  test("KorriPublished: a cursor on a network that left the scan goes to the step's first row", () => {
    const lost = { ...ZONE_ONBOARDING_START, step: "wifi" as const, focus: networkTarget("gone") }
    expect(send(lost, { _tag: "KorriPublished" }).model.focus).toBe(networkTarget("net-home"))
    expect(send(ZONE_ONBOARDING_START, { _tag: "KorriPublished" })).toEqual({ model: ZONE_ONBOARDING_START, requests: [] })
  })

  test("KorriPublished: the keyboard goes down if Korri stops offering the name", () => {
    const typing = at("name", heard, { entry: { draft: "mi", caret: 2, layer: "letters", focusedKey: "i" } })
    expect(send(typing, { _tag: "KorriPublished" }, { ...heard, settings: [] }).model.entry).toBeUndefined()
  })

  test("PressedBack: the keyboard goes down first, then setup goes back one step; never before CONTROLLERS", () => {
    const typing = at("name", heard, { entry: { draft: "mi", caret: 2, layer: "letters", focusedKey: "i" } })
    expect(send(typing, { _tag: "PressedBack" }).model).toMatchObject({ step: "name", entry: undefined, focus: TARGET.name })
    expect(send(at("wifi"), { _tag: "PressedBack" }).model).toMatchObject({ step: "controllers", focus: TARGET.next })
    expect(send(at("identity"), { _tag: "PressedBack" }).model.step).toBe("name")
    expect(send(ZONE_ONBOARDING_START, { _tag: "PressedBack" }).model.step).toBe("controllers")
  })

  test("FocusedTarget moves the cursor, but not from under the keyboard", () => {
    expect(send(at("wifi"), { _tag: "FocusedTarget", target: TARGET.skip }).model.focus).toBe(TARGET.skip)
    const typing = at("name", heard, { focus: TARGET.name, entry: { draft: "", caret: 0, layer: "letters", focusedKey: "q" } })
    expect(send(typing, { _tag: "FocusedTarget", target: TARGET.next }).model.focus).toBe(TARGET.name)
  })

  test("ChoseNext: CONTROLLERS needs a heard controller", () => {
    expect(send(ZONE_ONBOARDING_START, { _tag: "ChoseNext" }, waiting).model.step).toBe("controllers")
    expect(send(ZONE_ONBOARDING_START, { _tag: "ChoseNext" }, heard).model).toMatchObject({
      step: "wifi",
      focus: networkTarget("net-home"),
    })
  })

  test("ChoseNext: WI-FI needs a connection, or a host that cannot manage one", () => {
    expect(send(at("wifi"), { _tag: "ChoseNext" }, heard).model.step).toBe("wifi")
    expect(send(at("wifi", online), { _tag: "ChoseNext" }, online).model).toMatchObject({ step: "name", focus: TARGET.name })
    const noNetwork = { ...heard, network: undefined }
    expect(send(at("wifi", noNetwork), { _tag: "ChoseNext" }, noNetwork).model.step).toBe("name")
    expect(canGoOn("name", heard)).toBe(true)
    expect(canGoOn("identity", heard)).toBe(false)
  })

  test("ChoseSkip leaves WI-FI for NAME without a connection", () => {
    expect(send(at("wifi"), { _tag: "ChoseSkip" }).model.step).toBe("name")
    expect(send(ZONE_ONBOARDING_START, { _tag: "ChoseSkip" }).model.step).toBe("controllers")
  })

  test("ChoseScan asks Korri for a fresh scan", () => {
    expect(send(at("wifi"), { _tag: "ChoseScan" }).requests).toEqual([{ _tag: "ScanWifi" }])
    expect(send(ZONE_ONBOARDING_START, { _tag: "ChoseScan" }).requests).toEqual([])
  })

  test("OnboardingJoin: a saved or open network is joined at once; a locked one goes to the join flow", () => {
    expect(onboardingJoin("net-home")).toEqual({ _tag: "OnboardingJoin", networkId: "net-home" })
    expect(send(at("wifi"), onboardingJoin("net-home")).requests).toEqual([{ _tag: "ConnectWifi", networkId: "net-home" }])
    expect(send(at("wifi"), onboardingJoin("net-cafe")).requests).toEqual([{ _tag: "ConnectWifi", networkId: "net-cafe" }])
    const locked = send(at("wifi"), onboardingJoin("net-neighbour"))
    expect(locked.requests).toEqual([])
    expect(locked.model).toMatchObject({ join: "net-neighbour", focus: networkTarget("net-neighbour") })
    expect(send(at("wifi"), onboardingJoin("nope")).requests).toEqual([])
  })

  test("OpenedName raises the keyboard with Korri's name; with no name setting it does nothing", () => {
    expect(send(at("name"), { _tag: "OpenedName" }).model.entry).toEqual({ draft: "mini", caret: 4, layer: "letters", focusedKey: "done" })
    expect(send(at("name"), { _tag: "OpenedName" }, { ...heard, settings: [] }).model.entry).toBeUndefined()
  })

  test("FocusedKey lights the key the host's cursor is on; with the keyboard down it does nothing", () => {
    const typing = send(at("name"), { _tag: "OpenedName" }).model
    expect(send(typing, { _tag: "FocusedKey", id: "q" }).model.entry?.focusedKey).toBe("q")
    expect(send(at("name"), { _tag: "FocusedKey", id: "q" }).model).toEqual(send(at("name"), { _tag: "KorriPublished" }).model)
  })

  test("PressedKey types at the caret, within Korri's maxLength, deletes and switches layers", () => {
    const typing = send(at("name"), { _tag: "OpenedName" }).model
    const typed = send(typing, { _tag: "PressedKey", key: { kind: "char", char: "2" } }).model
    expect(typed.entry).toMatchObject({ draft: "mini2", caret: 5, focusedKey: "2" })
    expect(send(typed, { _tag: "PressedKey", key: { kind: "space" } }).model.entry?.draft).toBe("mini2 ")
    expect(send(typed, { _tag: "PressedKey", key: { kind: "delete" } }).model.entry).toMatchObject({ draft: "mini", caret: 4 })
    expect(send(typed, { _tag: "PressedKey", key: { kind: "layer", layer: "shift" } }).model.entry?.layer).toBe("shift")
    const full = { ...typed, entry: { draft: "x".repeat(24), caret: 24, layer: "letters" as const, focusedKey: "x" } }
    expect(send(full, { _tag: "PressedKey", key: { kind: "char", char: "y" } }).model.entry?.draft).toBe("x".repeat(24))
  })

  test("PressedKey DONE saves a changed name on the setting found by id, and closes", () => {
    const typing = send(at("name"), { _tag: "OpenedName" }).model
    const typed = send(typing, { _tag: "PressedKey", key: { kind: "char", char: "2" } }).model
    const done = send(typed, { _tag: "PressedKey", key: { kind: "done" } })
    expect(done.requests).toEqual([{ _tag: "ChangeSetting", settingId: "device-name", value: "mini2" }])
    expect(done.model).toMatchObject({ entry: undefined, focus: TARGET.name })
    expect(send(typing, { _tag: "PressedKey", key: { kind: "done" } }).requests).toEqual([])
  })

  test("deviceNameSetting finds Korri's setting by its id, not its label", () => {
    const labelled: SurfaceModel = {
      ...heard,
      settings: [{ title: "System", items: [{ id: "host.name", label: "Device name", value: "zao", interaction: { kind: "text" } }] }],
    }
    expect(deviceNameSetting(labelled)).toBeUndefined()
    const portal: SurfaceModel = {
      ...heard,
      settings: [{ title: "Device", items: [{ id: "device-name", label: "Name", value: "zao", interaction: { kind: "text", placeholder: "This device", maxLength: 64 } }] }],
    }
    expect(deviceNameSetting(portal)?.value).toBe("zao")
    // Read-only is still the name, but A on the NAME row opens no keyboard.
    const readOnly: SurfaceModel = { ...heard, settings: [{ title: "Device", items: [{ id: "device-name", label: "Name", value: "zao" }] }] }
    expect(deviceNameSetting(readOnly)?.value).toBe("zao")
    expect(updateOnboarding({ ...ZONE_ONBOARDING_START, step: "name", focus: TARGET.name }, { _tag: "OpenedName" }, readOnly).model.entry)
      .toBeUndefined()
  })

  test("ChoseStart chooses the new save and moves the cursor to FINISH", () => {
    expect(send(at("identity"), { _tag: "ChoseStart" }).model).toMatchObject({ started: true, focus: TARGET.finish })
    expect(send(ZONE_ONBOARDING_START, { _tag: "ChoseStart" }).model.started).toBe(false)
  })

  test("ChoseFinish ends setup once START was chosen, and nothing moves after", () => {
    expect(send(at("identity"), { _tag: "ChoseFinish" }).requests).toEqual([])
    const finished = send(at("identity", heard, { started: true }), { _tag: "ChoseFinish" })
    expect(finished.requests).toEqual([{ _tag: "FinishOnboarding" }])
    expect(finished.model.finishing).toBe(true)
    expect(send(finished.model, { _tag: "PressedBack" }).model.step).toBe("identity")
    expect(send(finished.model, { _tag: "ChoseFinish" }).requests).toEqual([])
  })

  test("the page draws the model's targets by the same ids", () => {
    expect(ONBOARDING_TARGET).toEqual(TARGET)
    expect(onboardingNetworkTarget("x")).toBe(networkTarget("x"))
    expect(onboardingTargets(at("wifi"), heard)).toEqual([
      TARGET.connection, networkTarget("net-home"), networkTarget("net-cafe"), networkTarget("net-neighbour"),
      TARGET.scan, TARGET.skip, TARGET.next,
    ])
  })
})

describe("planControllerCards", () => {
  test("one row while the cards fit at their widest", () => {
    expect(planControllerCards(1200, 500, 4)).toEqual({ columns: 4, rows: 1, cardWidth: ZONE_CONTROLLER_CARD_MAX })
  })

  test("a narrow, tall stage takes fewer columns rather than tiny cards", () => {
    const layout = planControllerCards(150, 350, 4)
    expect(layout.columns).toBe(2)
    expect(layout.cardWidth).toBe(Math.floor((150 - ZONE_CONTROLLER_CARD_GAP) / 2))
  })

  test("a wide, short stage keeps one row", () => {
    expect(planControllerCards(1280, 160, 4).columns).toBe(4)
  })

  test("never narrower than the least width; no controller is one waiting card", () => {
    expect(planControllerCards(40, 40, 6).cardWidth).toBe(ZONE_CONTROLLER_CARD_MIN)
    expect(planControllerCards(1200, 500, 0)).toEqual({ columns: 1, rows: 1, cardWidth: ZONE_CONTROLLER_CARD_MAX })
  })

  test("the plan's constants are the tokens", () => {
    const tokens = readFileSync(join(import.meta.dir, "..", "src", "zone-tokens.css"), "utf8")
    expect(tokens).toContain(`--zone-controller-card-max: ${ZONE_CONTROLLER_CARD_MAX};`)
    expect(tokens).toContain(`--zone-card-gap-wide: ${ZONE_CONTROLLER_CARD_GAP};`)
  })
})

/** The page on the real model, a pretend Korri answering. */
function Harness({ program }: { readonly program: ReturnType<typeof createOnboardingProgram> }) {
  const { model, context } = useSyncExternalStore(program.subscribe, program.get)
  return (
    <ZoneOnboarding
      canGoOn={canGoOn(model.step, context)}
      controllers={context.onboarding?.controllers ?? []}
      entry={model.entry}
      finishing={model.finishing}
      focus={model.focus}
      nameSetting={deviceNameSetting(context)}
      network={context.network}
      onFinish={() => program.dispatch({ _tag: "ChoseFinish" })}
      onFocusTarget={(target) => program.dispatch({ _tag: "FocusedTarget", target })}
      onJoin={(networkId) => program.dispatch(onboardingJoin(networkId))}
      onKey={(key) => program.dispatch({ _tag: "PressedKey", key })}
      onNext={() => program.dispatch({ _tag: "ChoseNext" })}
      onOpenName={() => program.dispatch({ _tag: "OpenedName" })}
      onScan={() => program.dispatch({ _tag: "ChoseScan" })}
      onSkip={() => program.dispatch({ _tag: "ChoseSkip" })}
      onStart={() => program.dispatch({ _tag: "ChoseStart" })}
      settingsStatus={context.settingsStatus}
      started={model.started}
      step={model.step}
    />
  )
}

const mount = (korri: SurfaceModel, init: Partial<ZoneOnboardingModel> = {}) => {
  const program = createOnboardingProgram(korri, init)
  render(<Harness program={program} />)
  return program
}

const currentStep = () => {
  const strip = screen.getByRole("group", { name: "Setup steps" })
  return within(strip).getAllByText(/^\d /).find(tag => tag.getAttribute("data-kind") === "here")?.textContent
}

describe("ZoneOnboarding", () => {
  test("C25: the strip shows four steps and where you are", () => {
    mount(waiting)
    const strip = screen.getByRole("group", { name: "Setup steps" })
    expect(within(strip).getAllByText(/^\d /).map(tag => tag.textContent)).toEqual(["1 Controllers", "2 Wi-Fi", "3 Name", "4 Identity"])
    expect(currentStep()).toBe("1 Controllers")
  })

  test("C25: a controller not yet heard reads WAITING, and NEXT does nothing", () => {
    const program = mount(waiting)
    expect(screen.getByRole("group", { name: "Waiting" })).toBeDefined()
    expect(screen.queryByText("Xbox Wireless Controller")).toBeNull()
    const next = screen.getByRole("button", { name: "Next" })
    expect(next.getAttribute("aria-disabled")).toBe("true")
    fireEvent.click(next)
    expect(program.get().model.step).toBe("controllers")
  })

  test("C25: a heard controller flips to its name and 1P; NEXT leads to WI-FI", () => {
    mount(heard)
    expect(screen.getByText("Xbox Wireless Controller")).toBeDefined()
    expect(screen.getByText("1P")).toBeDefined()
    fireEvent.click(screen.getByRole("button", { name: "Next" }))
    expect(currentStep()).toBe("2 Wi-Fi")
  })

  test("C25: four controllers take their seats in heard order; the unheard one waits", () => {
    mount(onboardingModel(controllersFour))
    expect(["1P", "2P", "3P"].map(seat => screen.getByText(seat).textContent)).toEqual(["1P", "2P", "3P"])
    expect(screen.getByText("DualSense")).toBeDefined()
    expect(screen.getAllByRole("group", { name: "Waiting" })).toHaveLength(1)
  })

  test("C25: WI-FI lists the connection, each network, SCAN and SKIP; A joins a saved or open one", () => {
    const program = mount(heard, { step: "wifi", focus: networkTarget("net-home") })
    expect(screen.getByText("Connection")).toBeDefined()
    expect(screen.getByText("Offline")).toBeDefined()
    expect(screen.getByText("82% Saved")).toBeDefined()
    expect(screen.getByText("21% Locked")).toBeDefined()
    fireEvent.click(screen.getByText("Cafe Guest"))
    expect(program.host.calls).toEqual(["connectWifi:net-cafe"])
    expect(screen.getByText("Cafe Guest", { selector: ".zone-option-row-text" })).toBeDefined()
  })

  test("C25: a locked network is handed to the join flow; SCAN rescans; SKIP goes on to NAME", () => {
    const program = mount({ ...heard, network: networkOffline }, { step: "wifi" })
    fireEvent.click(screen.getByText("NETGEAR-5G-supercalifragilistic-extension"))
    expect(program.host.calls).toEqual([])
    expect(program.get().model.join).toBe("net-neighbour")
    fireEvent.click(screen.getByText("Scan"))
    expect(program.host.calls).toEqual(["scanWifi"])
    fireEvent.click(screen.getByText("Skip"))
    expect(currentStep()).toBe("3 Name")
  })

  test("C25: NAME opens the keyboard on Korri's name and DONE saves it with changeSetting", () => {
    const program = mount(online, { step: "name", focus: TARGET.name })
    expect(screen.getByText("Name")).toBeDefined()
    fireEvent.click(screen.getByText("Name"))
    const keyboard = screen.getByRole("group", { name: "Keyboard" })
    fireEvent.click(within(keyboard).getByRole("button", { name: "2" }))
    fireEvent.click(within(keyboard).getByRole("button", { name: "Done" }))
    expect(program.host.calls).toEqual(["setting:device-name=mini2"])
    expect(screen.queryByRole("group", { name: "Keyboard" })).toBeNull()
    expect(screen.getByText("mini2")).toBeDefined()
  })

  test("C25: IDENTITY offers the NEW SAVE card; START lights FINISH; FINISH ends setup", () => {
    const program = mount(online, { step: "identity", focus: TARGET.save })
    expect(screen.getByText("New save")).toBeDefined()
    expect(screen.getByText("Start")).toBeDefined()
    const finish = screen.getByRole("button", { name: "Finish" })
    expect(finish.getAttribute("aria-disabled")).toBe("true")
    fireEvent.click(finish)
    expect(program.host.calls).toEqual([])
    fireEvent.click(screen.getByText("Start"))
    expect(screen.getByText("Ready")).toBeDefined()
    expect(document.activeElement?.textContent).toContain("Finish")
    fireEvent.click(screen.getByRole("button", { name: "Finish" }))
    expect(program.host.calls).toEqual(["finishOnboarding"])
  })

  test("C25: B goes back a step", () => {
    const program = mount(heard, { step: "wifi" })
    act(() => program.dispatch({ _tag: "PressedBack" }))
    expect(currentStep()).toBe("1 Controllers")
  })
})
