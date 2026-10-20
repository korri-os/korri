/**
 * Zone as a whole, from home, the way the portal drives it: one test per
 * capability item (docs/design/surfaces/CAPABILITIES.md, C1 to C30).
 *
 * ZoneSurface is rendered with a fake host that records every call and its
 * arguments. The test drives it as the portal does: it moves DOM focus to
 * the control the d-pad would reach (the portal moves focus by geometry,
 * which happy-dom cannot measure), confirms with a click on the focused
 * element, delivers left and right as `korri-semantic-direction` to a
 * focused `data-korri-horizontal-control`, and fires the host's back,
 * options, menu and system. Korri's answers are new models, published the
 * way the portal publishes them.
 *
 * Each test asserts the words the item requires on screen and, where the
 * item calls the host, the call and its arguments. It does not re-test a
 * page's internals: the pages' own suites do. The point here is that the
 * item is reachable from home with d-pad, confirm and back.
 */
import { afterEach, describe, expect, jest, test } from "bun:test"
import { act, cleanup, render, screen, within } from "@testing-library/react"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import type {
  SurfaceAction,
  SurfaceGame,
  SurfaceHost,
  SurfaceIdentityManagement,
  SurfaceInputAction,
  SurfaceModel,
  SurfaceNetwork,
  SurfaceRunnerChoice,
} from "@contracts/surface/korri-surface"
import { ZoneSurface } from "../src/index"

afterEach(() => {
  cleanup()
  jest.useRealTimers()
})

// ---- Korri's facts ----

/** 19 Oct 2026, 12:00 UTC: the same day in every time zone from -11 to +11. */
const LAST_PLAYED = 1_792_411_200_000
const PIXEL = "data:image/gif;base64,R0lGODlhAQABAAAAACw="

const HERE = { id: "here", label: "This device" }
const ZAO = { id: "zao", label: "zao" }

const GAMES: readonly SurfaceGame[] = [
  {
    id: "celeste",
    title: "Celeste Classic",
    subtitle: "PICO-8",
    section: "Continue",
    resumable: true,
    coverArtUrl: PIXEL,
    lastPlayedAt: LAST_PLAYED,
    playCount: 23,
    totalPlaytimeSeconds: 41_460,
  },
  { id: "hollow", title: "Hollow Knight", subtitle: "Switch", section: "This device", coverArtUrl: PIXEL },
  { id: "spelunky", title: "Spelunky", subtitle: "PC", section: "This device", launchLocations: [HERE, ZAO] },
  { id: "tetris", title: "Tetris", subtitle: "GB", section: "zao", launchLocations: [ZAO] },
  { id: "wario", title: "Wario Land 4", subtitle: "GBA", section: "zao", wideArtUrl: PIXEL },
]

const GAME_ACTIONS: readonly SurfaceAction[] = [
  { id: "verify", label: "Verify files", enabled: true },
  { id: "delete-save", label: "Delete save", description: "Removes the save on this device", enabled: true, destructive: true },
  { id: "copy", label: "Copy to zao", description: "zao is not on the network", enabled: false },
  { id: "runners", label: "Runners on this device", enabled: true },
]

const NETWORK: SurfaceNetwork = {
  connection: { _tag: "Wifi", ssid: "hummingbird-lake", strengthPercent: 72 },
  wifi: {
    _tag: "Available",
    scanning: false,
    networks: [
      { id: "n-home", ssid: "hummingbird-lake", strengthPercent: 72, secured: true, known: true },
      { id: "n-near", ssid: "Neighbour", strengthPercent: 48, secured: true, known: false },
      { id: "n-cafe", ssid: "Cafe", strengthPercent: 30, secured: false, known: false },
    ],
    attempt: { _tag: "Idle" },
  },
}

const SEATS = "player-seats"

const IDENTITY: SurfaceIdentityManagement = {
  localBackupAvailable: false,
  retiredPublicKeys: ["npub1retiredkeyaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
  status: { _tag: "Idle" },
}

const HOME: SurfaceModel = {
  presentation: { kind: "catalog" },
  catalog: { _tag: "Ready", games: GAMES },
  status: { _tag: "Browsing" },
  clockLabel: "21:47",
  buildLabel: "korri 0.4.1 · 9a1fb22",
  actions: [
    { id: "pair", label: "Pair controller", enabled: true },
    { id: "restart", label: "Restart", description: "Closes every game", enabled: true, destructive: true },
  ],
  settings: [
    {
      title: "Device",
      items: [
        { id: "device-name", label: "Device name", value: "mini", description: "What other devices call this one", interaction: { kind: "text", maxLength: 24 } },
        {
          id: SEATS,
          label: "Player seats",
          value: "2",
          interaction: { kind: "choice", choices: ["1", "2", "3", "4"].map(value => ({ value, label: value })) },
        },
        {
          id: "steamgriddb",
          label: "SteamGridDB API key",
          value: "Configured",
          interaction: { kind: "sensitiveText", clearLabel: "Clear saved key" },
        },
        {
          id: "rescan",
          label: "Library",
          value: "Rescan",
          interaction: {
            kind: "action",
            actionId: "rescan-library",
            confirmation: { title: "Rescan the library?", message: "Every folder is read again", confirmLabel: "Rescan" },
          },
        },
      ],
    },
    { title: "System information", items: [{ id: "games-count", label: "Games", value: "14 games" }] },
  ],
  settingsStatus: { _tag: "Idle" },
  network: NETWORK,
  identityManagement: IDENTITY,
}

const withGames = (games: readonly SurfaceGame[]): SurfaceModel => ({ ...HOME, catalog: { _tag: "Ready", games } })

// ---- The fake host ----

interface Call {
  readonly method: string
  readonly args: readonly unknown[]
}

interface FakeHost extends SurfaceHost {
  readonly calls: Call[]
  press(action: SurfaceInputAction): void
}

function fakeHost(actions: readonly SurfaceAction[] = GAME_ACTIONS): FakeHost {
  const calls: Call[] = []
  const listeners = new Map<SurfaceInputAction, Set<() => void>>()
  const record = (method: string) => (...args: unknown[]) => {
    // Optional arguments the surface left out are not recorded as undefined.
    while (args.length > 0 && args.at(-1) === undefined) args.pop()
    calls.push({ method, args })
  }
  return {
    calls,
    press(action) {
      for (const listener of [...(listeners.get(action) ?? [])]) listener()
    },
    input: {
      on(action, handler) {
        const set = listeners.get(action) ?? new Set()
        set.add(handler)
        listeners.set(action, set)
        return () => set.delete(handler)
      },
    },
    gameActions: () => actions,
    launchGame: record("launchGame"),
    runAction: record("runAction"),
    changeSetting: record("changeSetting"),
    dismissSettingsProblem: record("dismissSettingsProblem"),
    exportIdentityBackup: record("exportIdentityBackup"),
    switchIdentityFromBackup: record("switchIdentityFromBackup"),
    switchIdentityToNip46: record("switchIdentityToNip46"),
    deleteRetiredIdentity: record("deleteRetiredIdentity"),
    dismissIdentityStatus: record("dismissIdentityStatus"),
    runGameAction: record("runGameAction"),
    invokeGameplayControl: record("invokeGameplayControl"),
    dismissGameplayOverlay: record("dismissGameplayOverlay"),
    retry: record("retry"),
    dismiss: record("dismiss"),
    reload: record("reload"),
    scanWifi: record("scanWifi"),
    connectWifi: record("connectWifi"),
    connectHiddenWifi: record("connectHiddenWifi"),
    finishOnboarding: record("finishOnboarding"),
  }
}

// ---- Driving it as the portal does ----

function mount(model: SurfaceModel = HOME, actions: readonly SurfaceAction[] = GAME_ACTIONS) {
  const host = fakeHost(actions)
  const view = render(<ZoneSurface host={host} model={model} />)
  /** Korri publishes a new model. */
  const publish = (next: SurfaceModel) => act(() => view.rerender(<ZoneSurface host={host} model={next} />))
  /** The host's back, options, menu or system. */
  const press = (action: SurfaceInputAction) => act(() => host.press(action))
  return { host, publish, press }
}

/** The button whose words are `text`, anywhere on screen. */
const button = (text: string | RegExp, scope: HTMLElement = document.body): HTMLButtonElement => {
  const found = within(scope).getAllByText(text).map(node => node.closest("button")).find(node => node !== null)
  if (found === undefined || found === null) throw new Error(`no button ${String(text)}`)
  return found
}
/** The d-pad moved DOM focus here. */
const moveTo = (element: HTMLElement) => act(() => element.focus())
/** A: the portal clicks the focused element. */
function confirm() {
  const focused = document.activeElement
  if (!(focused instanceof HTMLElement) || focused === document.body) throw new Error("A with nothing focused")
  act(() => focused.click())
}
/** Left or right on a stepper: only a focused horizontal control hears it. */
function sideways(direction: "left" | "right") {
  const focused = document.activeElement
  if (!(focused instanceof HTMLElement) || !focused.hasAttribute("data-korri-horizontal-control")) {
    throw new Error(`${direction} on a control that does not step: the portal would move focus`)
  }
  act(() => {
    focused.dispatchEvent(new CustomEvent("korri-semantic-direction", { detail: { direction, repeat: false } }))
  })
}
/** Type on Zone's keyboard: the d-pad to each key, A on it. */
function type(text: string, keyboard: HTMLElement = keyboardOnScreen()) {
  for (const char of text) {
    moveTo(button(char === " " ? "Space" : char, keyboard))
    confirm()
  }
}
const keyboardOnScreen = () => screen.getByRole("group", { name: "Keyboard" })
const focusedText = () => document.activeElement?.textContent ?? ""
const onScreen = (text: string | RegExp) => screen.getAllByText(text).length > 0
const currentTab = () => document.querySelector("[role='tablist'] [aria-current='page']")?.textContent ?? ""
const cardTitles = () => [...document.querySelectorAll(".zone-card-title")].map(node => node.textContent)
const calls = (host: FakeHost) => host.calls.map(call => [call.method, ...call.args])

/** From home to OPTIONS with the d-pad: menu to the tabs, right, A. */
function openOptions(press: (action: SurfaceInputAction) => void) {
  press("menu")
  moveTo(button("Options"))
  confirm()
  expect(currentTab()).toContain("Options")
}

/** From home to SAVE DATA with the d-pad. */
function openSaveData(press: (action: SurfaceInputAction) => void) {
  press("menu")
  moveTo(button("Save data"))
  confirm()
  expect(currentTab()).toContain("Save data")
}

/** An OPTIONS row by its label. */
const row = (label: string | RegExp) => button(label, document.querySelector<HTMLElement>(".zone-options-scroll")!)

// ---- C1 to C6: the catalog ----

describe("catalog", () => {
  test("C1 catalog states", () => {
    const { host, publish } = mount({ ...HOME, catalog: { _tag: "Loading" } })
    expect(onScreen("Loading")).toBe(true)
    expect(document.querySelectorAll(".zone-games-loading .zone-card")).toHaveLength(3)

    publish({ ...HOME, catalog: { _tag: "Error", message: "The library could not be read." } })
    expect(screen.getByRole("dialog", { name: "Games" }).textContent).toContain("The library could not be read.")
    expect(focusedText()).toBe("Reload")
    confirm()
    expect(calls(host)).toEqual([["reload"]])

    publish({ ...HOME, catalog: { _tag: "Empty" } })
    expect(onScreen("No games")).toBe(true)

    publish(HOME)
    expect(cardTitles()).toEqual(GAMES.map(game => game.title))
    expect(focusedText()).toContain("Celeste Classic")
  })

  test("C2 browse", () => {
    mount()
    // Titles and subtitles on the cards, each section opened by a spine.
    expect(cardTitles()).toEqual(["Celeste Classic", "Hollow Knight", "Spelunky", "Tetris", "Wario Land 4"])
    expect([...document.querySelectorAll(".zone-section-card")].map(spine => spine.textContent))
      .toEqual(["Continue", "This device", "zao"])
    expect(onScreen("PICO-8")).toBe(true)
    // Cover art fills the well; a game with only wide art, or none, wears the stripe well with initials.
    const well = (title: string) => button(title).querySelector(".zone-cover-well")?.getAttribute("data-filled")
    expect(well("Hollow Knight")).toBe("art")
    expect(well("Spelunky")).toBe("initials")
    expect(well("Wario Land 4")).toBe("initials")
    // The d-pad walks the band.
    moveTo(button("Tetris"))
    expect(document.querySelector(".zone-card[data-focused='true'] .zone-card-title")?.textContent).toBe("Tetris")
  })

  test("C3 game facts", () => {
    mount()
    expect(document.querySelector(".zone-games-facts")?.textContent).toBe("Last played19 Oct 2026Plays23Time11h 31m")
    expect(button("Continue", document.querySelector<HTMLElement>(".zone-games-actions")!)).toBeTruthy()
    moveTo(button("Hollow Knight"))
    expect(document.querySelector(".zone-games-facts")?.hasAttribute("data-empty")).toBe(true)
    expect(button("Play", document.querySelector<HTMLElement>(".zone-games-actions")!)).toBeTruthy()
  })

  test("C4 launch", () => {
    const { host } = mount()
    // One place: A on the card is A on its one lit button.
    moveTo(button("Hollow Knight"))
    confirm()
    expect(calls(host)).toEqual([["launchGame", "hollow"]])
    // A choice: A drops into the row and launches nothing until the person picks.
    moveTo(button("Spelunky"))
    confirm()
    expect(calls(host)).toHaveLength(1)
    expect(focusedText()).toBe("Play here")
    moveTo(button("Play on zao"))
    confirm()
    expect(calls(host).at(-1)).toEqual(["launchGame", "spelunky", "zao"])
  })

  test("C5 per-game actions", () => {
    const { host, press } = mount()
    moveTo(button("Spelunky"))
    press("options")
    expect(onScreen("Game opts.")).toBe(true)
    expect(onScreen("zao is not on the network")).toBe(true)
    // Disabled: A does nothing.
    moveTo(button("Copy to zao"))
    confirm()
    expect(calls(host)).toEqual([])
    // Destructive: NO / YES first, NO lit.
    moveTo(button("Delete save"))
    confirm()
    expect(screen.getByRole("dialog", { name: "Delete save" }).textContent).toContain("Removes the save on this device")
    expect(focusedText()).toBe("No")
    moveTo(button("Yes"))
    confirm()
    expect(calls(host)).toEqual([["runGameAction", "spelunky", "delete-save"]])
    // A plain one runs at once.
    press("options")
    moveTo(button("Verify files"))
    confirm()
    expect(calls(host).at(-1)).toEqual(["runGameAction", "spelunky", "verify"])
  })

  test("C6 find", () => {
    const { press } = mount()
    press("menu")
    press("menu")
    expect(focusedText()).toBe("Find")
    confirm()
    type("ar")
    expect(cardTitles()).toEqual(["Wario Land 4"])
    moveTo(button("Done", keyboardOnScreen()))
    confirm()
    expect(onScreen("Find: ar")).toBe(true)
    press("back")
    expect(cardTitles()).toHaveLength(GAMES.length)
  })
})
