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
  SurfaceGameplayControl,
  SurfaceGameplayOverlayPresentation,
  SurfaceHost,
  SurfaceIdentityManagement,
  SurfaceInputAction,
  SurfaceModel,
  SurfaceNetwork,
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
        { id: "device-name", label: "Name", value: "mini", description: "What other devices call this one", interaction: { kind: "text", placeholder: "This device", maxLength: 64 } },
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
const onScreen = (text: string | RegExp) => screen.queryAllByText(text).length > 0
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

// ---- C7 to C10: Korri's status over any page ----

describe("status", () => {
  test("C7 busy", () => {
    const { host, publish, press } = mount()
    moveTo(button("Hollow Knight"))
    confirm()
    publish({
      ...HOME,
      status: {
        _tag: "Busy",
        kicker: "Starting",
        detail: "Waking zao",
        gameId: "hollow",
        actions: [{ id: "cancel-launch", label: "Cancel", enabled: true }],
      },
    })
    const strip = screen.getByRole("status")
    expect(strip.textContent).toContain("Starting")
    expect(strip.textContent).toContain("Waking zao")
    expect(document.querySelector(".zone-status-layer-busy")?.textContent).toContain("Hollow Knight")
    // The page under it is inert; the cursor is on Korri's own cancel.
    expect(document.querySelector(".zone-status-layer-page")?.hasAttribute("inert")).toBe(true)
    expect(focusedText()).toBe("Cancel")
    press("back")
    expect(calls(host)).toEqual([["launchGame", "hollow"]])
    confirm()
    expect(calls(host).at(-1)).toEqual(["runAction", "cancel-launch"])
    // Korri goes back to browsing: the cursor returns to where it was.
    publish(HOME)
    expect(focusedText()).toContain("Hollow Knight")
  })

  test("C8 running", () => {
    const { host, publish } = mount()
    publish({ ...HOME, status: { _tag: "Running", kicker: "Playing on zao", gameId: "tetris" } })
    const strip = document.querySelector<HTMLElement>(".zone-status-layer-running")!
    expect(strip.textContent).toContain("Playing on zao")
    expect(strip.textContent).toContain("Open")
    expect(strip.textContent).toContain("Tetris")
    // The page stays usable under it.
    expect(document.querySelector(".zone-status-layer-page")?.hasAttribute("inert")).toBe(false)
    moveTo(button("Hollow Knight"))
    confirm()
    expect(calls(host)).toEqual([["launchGame", "hollow"]])
  })

  test("C9 problem", () => {
    const { host, publish, press } = mount()
    moveTo(button("Spelunky"))
    const problem: SurfaceModel = {
      ...HOME,
      status: { _tag: "Problem", kicker: "Could not start", reason: "zao did not answer.", canRetry: true, gameId: "tetris", gameTitle: "Tetris" },
    }
    publish(problem)
    const dialog = screen.getByRole("dialog", { name: "Could not start" })
    expect(dialog.textContent).toContain("zao did not answer.")
    // Stated against Korri's game, not the card on screen.
    expect(dialog.textContent).toContain("Tetris")
    expect(dialog.textContent).not.toContain("Spelunky")
    expect(focusedText()).toBe("Dismiss")
    moveTo(button("Retry", dialog))
    confirm()
    expect(calls(host)).toEqual([["retry"]])
    moveTo(button("Dismiss", dialog))
    confirm()
    expect(calls(host).at(-1)).toEqual(["dismiss"])
    press("back")
    expect(calls(host).at(-1)).toEqual(["dismiss"])
    expect(calls(host)).toHaveLength(3)
    publish(HOME)
    expect(focusedText()).toContain("Spelunky")
  })

  test("C10 reload", () => {
    const { host, publish } = mount()
    publish({ ...HOME, catalog: { _tag: "Error", message: "The disk did not answer." } })
    expect(focusedText()).toBe("Reload")
    confirm()
    expect(calls(host)).toEqual([["reload"]])
  })
})

// ---- C11: Korri's runner choice ----

describe("runner choice", () => {
  const route = {
    runnerId: "mgba",
    systemId: "gba",
    runnerBuild: "/nix/store/abc-mgba-0.10.3",
    program: "mGBA",
    warnings: ["BIOS missing: games may not boot"],
    actions: [{ id: "runner:launch:mgba", label: "Play with mGBA", enabled: true }],
  }
  const choice = (tag: "Loading" | "Ready" | "Stale") => ({
    _tag: tag,
    gameTitle: "Wario Land 4",
    message: "Choose what plays it",
    routes: [route, { ...route, runnerId: "vba", program: "VBA-M", runnerBuild: "/nix/store/def-vbam-2.1", warnings: [], actions: [] }],
    saved: [{ label: "This game", runnerId: "mgba" }],
    warnings: ["Two runners are installed"],
    actions: [{ id: "runner:cancel", label: "Close", enabled: true }],
  })

  test("C11 runner choice", () => {
    const { host, publish, press } = mount()
    // GAME OPTS. lists Korri's own action that opens the runner choice.
    moveTo(button("Wario Land 4"))
    press("options")
    moveTo(button("Runners on this device"))
    confirm()
    expect(calls(host)).toEqual([["runGameAction", "wario", "runners"]])
    publish({ ...HOME, runnerChoice: choice("Loading") })
    expect(onScreen("Loading")).toBe(true)
    publish({ ...HOME, runnerChoice: choice("Ready") })
    const page = document.querySelector<HTMLElement>(".zone-run-with")!
    for (const words of ["Run with", "Wario Land 4", "Ready", "Choose what plays it", "Two runners are installed",
      "mGBA", "/nix/store/abc-mgba-0.10.3", "BIOS missing: games may not boot", "VBA-M", "This game"]) {
      expect(page.textContent).toContain(words)
    }
    // The cursor is in the page; Korri's action ids go back unchanged.
    expect(page.contains(document.activeElement)).toBe(true)
    moveTo(button("Play with mGBA", page))
    confirm()
    expect(calls(host).at(-1)).toEqual(["runAction", "runner:launch:mgba"])
    // A saved runner no route offers is its own row.
    publish({ ...HOME, runnerChoice: { ...choice("Stale"), routes: [], saved: [{ label: "This game", runnerId: "retroarch" }] } })
    expect(document.querySelector(".zone-run-with")?.textContent).toContain("retroarch")
    expect(document.querySelector(".zone-run-with")?.textContent).toContain("UNAVAILABLE")
    // Korri closes it: home again.
    publish({ ...HOME, runnerChoice: { _tag: "Closed" } })
    expect(document.querySelector(".zone-run-with")).toBeNull()
    expect(cardTitles()).toHaveLength(GAMES.length)
  })
})

// ---- C12 to C15: OPTIONS ----

describe("settings", () => {
  test("C12 groups and items", () => {
    const { press } = mount()
    openOptions(press)
    const titles = [...document.querySelectorAll(".zone-options-scroll h2")].map(heading => heading.textContent)
    expect(titles).toEqual(["Device", "System information", "System", "WI-FI"])
    expect(onScreen("Name")).toBe(true)
    expect(onScreen("mini")).toBe(true)
    expect(onScreen("14 games")).toBe(true)
    // The description shows under the focused row.
    expect(onScreen("What other devices call this one")).toBe(false)
    moveTo(row("Name"))
    expect(onScreen("What other devices call this one")).toBe(true)
  })

  test("C13 interactions", () => {
    const { host, press } = mount()
    openOptions(press)
    // choice: right and left step it, sent at once.
    moveTo(row("Player seats"))
    sideways("right")
    sideways("left")
    expect(calls(host)).toEqual([["changeSetting", SEATS, "3"], ["changeSetting", SEATS, "1"]])
    // text: A raises the keyboard with the value; DONE commits.
    moveTo(row("Name"))
    confirm()
    const name = screen.getByRole("dialog", { name: "Name" })
    type("2", name)
    moveTo(button("Done", name))
    confirm()
    expect(calls(host).at(-1)).toEqual(["changeSetting", "device-name", "mini2"])
    // sensitiveText: dots, never the value; Korri's clear label clears it.
    expect(onScreen("Configured")).toBe(false)
    moveTo(row("SteamGridDB API key"))
    expect(onScreen("Clear saved key")).toBe(true)
    confirm()
    const secret = screen.getByRole("dialog", { name: "SteamGridDB API key" })
    type("ab1", secret)
    expect(secret.textContent).toContain("•••")
    expect(secret.textContent).not.toContain("ab1")
    moveTo(button("Clear saved key", secret))
    confirm()
    expect(calls(host).at(-1)).toEqual(["changeSetting", "steamgriddb", ""])
    // action with Korri's confirmation: its title, message and confirm label.
    moveTo(row("Library"))
    confirm()
    const ask = screen.getByRole("dialog", { name: "Rescan the library?" })
    expect(ask.textContent).toContain("Every folder is read again")
    expect(focusedText()).toBe("No")
    moveTo(button("Rescan", ask))
    confirm()
    expect(calls(host).at(-1)).toEqual(["runAction", "rescan-library"])
  })

  test("C14 saving and problem", () => {
    const { host, publish, press } = mount()
    openOptions(press)
    publish({ ...HOME, settingsStatus: { _tag: "Saving", settingId: SEATS } })
    expect(row("Player seats").querySelector("[aria-label='Saving']")).not.toBeNull()
    publish({ ...HOME, settingsStatus: { _tag: "Problem", settingId: SEATS, message: "Seats could not be saved." } })
    expect(screen.getByRole("alert").textContent).toBe("Seats could not be saved.")
    moveTo(row("Dismiss"))
    confirm()
    expect(calls(host)).toEqual([["dismissSettingsProblem"]])
  })

  test("C15 device actions", () => {
    const { host, press } = mount()
    // `system` lands on the SYSTEM group's first row.
    press("system")
    expect(currentTab()).toContain("Options")
    expect(focusedText()).toContain("Pair controller")
    confirm()
    expect(calls(host)).toEqual([["runAction", "pair"]])
    // A destructive one asks first, under its label, with Korri's description.
    moveTo(row("Restart"))
    confirm()
    const ask = screen.getByRole("dialog", { name: "Restart" })
    expect(ask.textContent).toContain("Closes every game")
    moveTo(button("Yes", ask))
    confirm()
    expect(calls(host).at(-1)).toEqual(["runAction", "restart"])
  })
})

// ---- C16 to C20: SAVE DATA ----

describe("identity", () => {
  const withIdentity = (status: SurfaceIdentityManagement["status"], more: Partial<SurfaceIdentityManagement> = {}): SurfaceModel =>
    ({ ...HOME, identityManagement: { ...IDENTITY, ...more, status } })
  const saveCard = (words: string) => button(words, document.querySelector<HTMLElement>(".zone-save-data-band")!)
  const done = () => {
    moveTo(button("Done", keyboardOnScreen()))
    confirm()
  }

  test("C16 export backup", () => {
    const { host, publish, press } = mount()
    openSaveData(press)
    expect(saveCard("Back up").textContent).toContain("New")
    moveTo(saveCard("Back up"))
    confirm()
    expect(onScreen("Password")).toBe(true)
    type("pw")
    // The password is a secret on screen.
    expect(keyboardOnScreen().textContent).toContain("••")
    expect(keyboardOnScreen().textContent).not.toContain("pw")
    done()
    expect(calls(host)).toEqual([["exportIdentityBackup", "pw"]])
    publish(withIdentity({ _tag: "Working", operation: "Encrypting" }))
    expect(screen.getByRole("status").textContent).toContain("Encrypting")
    // A stand-in, built so it reads as one: never a real backup.
    const secret = `ncryptsec1${"testbackup".repeat(5)}`
    publish(withIdentity({ _tag: "BackupReady", encryptedSecret: secret }, { localBackupAvailable: true }))
    // The QR code in BACK UP's well and the text on its slab, the cursor on the slab.
    expect(screen.getByRole("img", { name: "Backup QR code" })).toBeTruthy()
    const slab = screen.getByRole("region", { name: "Backup secret" })
    expect(slab.textContent?.replace(/\s/g, "").toLowerCase()).toBe(secret)
    expect(document.activeElement).toBe(slab)
    // B puts the backup away: Korri is told, and the cards come back.
    press("back")
    expect(calls(host).at(-1)).toEqual(["dismissIdentityStatus"])
    publish(withIdentity({ _tag: "Idle" }, { localBackupAvailable: true }))
    expect(saveCard("Back up").textContent).toContain("Backed up")
  })

  test("C17 switch from backup", () => {
    const { host, press } = mount()
    openSaveData(press)
    moveTo(saveCard("From backup"))
    confirm()
    expect(onScreen("Backup")).toBe(true)
    type("ncrypt")
    done()
    expect(onScreen("Password")).toBe(true)
    type("pw")
    done()
    const records = screen.getByRole("dialog", { name: "Local records" })
    expect(focusedText()).toBe("Transfer")
    moveTo(button("Transfer", records))
    confirm()
    const trust = screen.getByRole("dialog", { name: "Switch key?" })
    expect(trust.textContent).toContain("Other devices will stop trusting this key")
    expect(focusedText()).toBe("No")
    moveTo(button("Yes", trust))
    confirm()
    expect(calls(host)).toEqual([["switchIdentityFromBackup", "ncrypt", "pw", "transfer", true]])
  })

  test("C18 switch by bunker", () => {
    const { host, press } = mount()
    openSaveData(press)
    moveTo(saveCard("Bunker"))
    confirm()
    expect(keyboardOnScreen().textContent).toContain("bunker://")
    type("relay.ab")
    done()
    moveTo(button("Delete", screen.getByRole("dialog", { name: "Local records" })))
    confirm()
    moveTo(button("Yes", screen.getByRole("dialog", { name: "Switch key?" })))
    confirm()
    expect(calls(host)).toEqual([["switchIdentityToNip46", "bunker://relay.ab", "delete", true]])
  })

  test("C19 retired keys", () => {
    const { host, press } = mount()
    openSaveData(press)
    const retired = saveCard("Retired")
    // A on a retired key moves the cursor into its row: BACK UP, then DELETE.
    moveTo(retired)
    confirm()
    const actions = document.querySelector<HTMLElement>(".zone-save-data-actions")!
    // BACK UP a retired key: the same password flow, the key named to Korri.
    moveTo(button("Back up", actions))
    confirm()
    type("pw")
    done()
    expect(calls(host)).toEqual([["exportIdentityBackup", "pw", IDENTITY.retiredPublicKeys[0]]])
    host.calls.length = 0
    moveTo(saveCard("Retired"))
    confirm()
    moveTo(button("Delete", document.querySelector<HTMLElement>(".zone-save-data-actions")!))
    confirm()
    const ask = screen.getByRole("dialog", { name: "Delete this key?" })
    expect(ask.textContent).toContain("Back it up first.")
    moveTo(button("Yes", ask))
    confirm()
    // Only after the backup is confirmed.
    expect(calls(host)).toEqual([])
    moveTo(button("Yes", screen.getByRole("dialog", { name: "Is it backed up?" })))
    confirm()
    expect(calls(host)).toEqual([["deleteRetiredIdentity", IDENTITY.retiredPublicKeys[0], true]])
  })

  test("C20 identity status", () => {
    const { host, publish, press } = mount()
    openSaveData(press)
    publish(withIdentity({ _tag: "Working", operation: "Switching" }))
    expect(screen.getByRole("status").textContent).toContain("Switching")
    publish(withIdentity({ _tag: "Problem", message: "The bunker did not answer." }))
    const problem = screen.getByRole("dialog", { name: "Identity" })
    expect(problem.textContent).toContain("The bunker did not answer.")
    expect(focusedText()).toBe("Dismiss")
    confirm()
    expect(calls(host)).toEqual([["dismissIdentityStatus"]])
    const owner = "npub1newownerkeyzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
    publish(withIdentity({ _tag: "Switched", ownerPublicKey: owner }))
    const switched = screen.getByRole("dialog", { name: "Switched" })
    expect(switched.textContent?.toLowerCase()).toContain(owner.slice(0, 8))
    expect(focusedText()).toBe("OK")
    confirm()
    expect(calls(host).at(-1)).toEqual(["dismissIdentityStatus"])
  })
})

// ---- C21 to C23: the network ----

describe("network", () => {
  const withNetwork = (network: SurfaceNetwork): SurfaceModel => ({ ...HOME, network })
  const wifi = (more: Partial<Extract<SurfaceNetwork["wifi"], { _tag: "Available" }>>): SurfaceNetwork => {
    if (NETWORK.wifi._tag !== "Available") throw new Error("the fixture has Wi-Fi")
    return { ...NETWORK, wifi: { ...NETWORK.wifi, ...more } }
  }
  const connectionRow = () => row("CONNECTION").textContent
  /** The row whose label is exactly this network's name. */
  const network = (ssid: string): HTMLButtonElement => {
    const label = [...document.querySelectorAll(".zone-options-scroll .zone-option-row-label")].find(node => node.textContent === ssid)
    const found = label?.closest("button")
    if (found === null || found === undefined) throw new Error(`no network ${ssid}`)
    return found
  }

  test("C21 connection", () => {
    const { publish, press } = mount()
    // The foot names the network on every page; OPTIONS says it in full.
    expect(document.querySelector(".zone-foot")?.textContent).toContain("hummingbird-lake")
    openOptions(press)
    expect(connectionRow()).toContain("hummingbird-lake")
    expect(connectionRow()).toContain("72%")
    publish(withNetwork({ ...NETWORK, connection: { _tag: "Wired" } }))
    // The connection is ZoneSignal's badge; its caps are the face's, not the text's.
    expect(connectionRow()).toMatch(/wired/i)
    publish(withNetwork({ ...NETWORK, connection: { _tag: "Offline" } }))
    expect(connectionRow()).toMatch(/offline/i)
  })

  test("C22 wi-fi list", () => {
    const { host, publish, press } = mount()
    openOptions(press)
    // Strongest first, with strength, LOCK and KNOWN.
    const listed = [...document.querySelectorAll(".zone-options-scroll .zone-option-row-label")]
      .map(label => label.textContent).filter(label => ["hummingbird-lake", "Neighbour", "Cafe"].includes(label ?? ""))
    expect(listed).toEqual(["hummingbird-lake", "Neighbour", "Cafe"])
    // A listed network's row, not the connection row that also names it.
    expect(network("hummingbird-lake").textContent).toContain("72% LOCK KNOWN")
    expect(network("Neighbour").textContent).toContain("48% LOCK")
    expect(network("Cafe").textContent).not.toContain("LOCK")
    expect(row("SCAN").textContent).toContain("3 FOUND")
    moveTo(row("SCAN"))
    confirm()
    expect(calls(host)).toEqual([["scanWifi"]])
    publish(withNetwork(wifi({ scanning: true })))
    expect(row("SCAN").textContent).toContain("SCANNING…")
    publish(withNetwork(wifi({ attempt: { _tag: "Connecting", ssid: "Cafe" } })))
    expect(row("Cafe").querySelector("[aria-label='Saving']")).not.toBeNull()
    publish(withNetwork(wifi({ attempt: { _tag: "Failed", ssid: "Cafe", message: "Cafe did not answer.", passwordRejected: false } })))
    expect(screen.getByRole("alert").textContent).toBe("Cafe did not answer.")
    // A rejected key: the keyboard comes back for that network with Korri's reason.
    publish(withNetwork(wifi({ attempt: { _tag: "Failed", ssid: "Neighbour", message: "The password was wrong.", passwordRejected: true } })))
    const again = screen.getByRole("dialog", { name: "Password" })
    expect(again.textContent).toContain("Neighbour")
    expect(again.textContent).toContain("The password was wrong.")
    press("back")
    publish(withNetwork({ ...NETWORK, wifi: { _tag: "NoAdapter" } }))
    expect(onScreen("NO WI-FI ADAPTER")).toBe(true)
  })

  test("C23 join", () => {
    const { host, press } = mount()
    openOptions(press)
    // A secured network Korri has no key for asks its password.
    moveTo(row("Neighbour"))
    confirm()
    const password = screen.getByRole("dialog", { name: "Password" })
    expect(password.textContent).toContain("Neighbour")
    type("pw", password)
    moveTo(button("Done", password))
    confirm()
    expect(calls(host)).toEqual([["connectWifi", "n-near", "pw"]])
    // An open one joins at once.
    moveTo(row("Cafe"))
    confirm()
    expect(calls(host).at(-1)).toEqual(["connectWifi", "n-cafe"])
    // A hidden one: its name, then its password.
    moveTo(row("HIDDEN NETWORK"))
    confirm()
    const name = screen.getByRole("dialog", { name: "Network name" })
    type("lab", name)
    moveTo(button("Done", name))
    confirm()
    const hidden = screen.getByRole("dialog", { name: "Password" })
    type("pw", hidden)
    moveTo(button("Done", hidden))
    confirm()
    expect(calls(host).at(-1)).toEqual(["connectHiddenWifi", "lab", "pw"])
  })
})

// ---- C24: PAUSED ----

describe("gameplay overlay", () => {
  const command = (id: string, label: string, more: Partial<SurfaceGameplayControl> = {}): SurfaceGameplayControl =>
    ({ id, label, enabled: true, destructive: false, dismissOnSuccess: false, interaction: { kind: "command" }, ...more })
  const overlay: SurfaceGameplayOverlayPresentation = {
    kind: "gameplay-overlay",
    title: "Wario Land 4",
    controls: [
      command("resume", "Continue", { dismissOnSuccess: true }),
      command("exit", "Exit", { destructive: true, dismissOnSuccess: true, description: "The game closes" }),
    ],
    groups: [{
      id: "mgba",
      label: "mGBA",
      controls: [
        { ...command("fill", "Fill screen"), interaction: { kind: "toggle", value: false, trueLabel: "On", falseLabel: "Off" } },
        {
          ...command("filter", "Filter"),
          interaction: { kind: "choice", value: "sharp", options: [{ value: "sharp", label: "Sharp" }, { value: "smooth", label: "Smooth" }] },
        },
        { ...command("volume", "Volume"), interaction: { kind: "range", value: 5, min: 0, max: 10, step: 1 } },
        command("save", "Save state", { enabled: false, disabledReason: "Saving is off for this game" }),
      ],
    }],
  }
  const paused: SurfaceModel = { ...HOME, presentation: overlay }
  const control = (label: string) => button(label, document.querySelector<HTMLElement>(".zone-paused-wedge")!)

  test("C24 overlay", () => {
    const { host, publish, press } = mount()
    moveTo(button("Hollow Knight"))
    confirm()
    publish(paused)
    const page = document.querySelector<HTMLElement>(".zone-paused")!
    expect(page.querySelector(".zone-paused-title")?.textContent).toBe("Wario Land 4")
    expect(page.textContent).toContain("mGBA")
    // The cursor opens on Korri's first control.
    expect(focusedText()).toContain("Continue")
    // toggle, choice and range answer left and right, each invoked as it moves.
    moveTo(control("Fill screen"))
    expect(focusedText()).toContain("Off")
    sideways("right")
    moveTo(control("Filter"))
    sideways("right")
    moveTo(control("Volume"))
    sideways("right")
    expect(focusedText()).toContain("6")
    expect(calls(host).slice(1)).toEqual([
      ["invokeGameplayControl", "fill", { kind: "toggle", value: true }],
      ["invokeGameplayControl", "filter", { kind: "choice", value: "smooth" }],
      ["invokeGameplayControl", "volume", { kind: "range", value: 6 }],
    ])
    // disabled: its reason under it, and A does nothing.
    expect(page.textContent).toContain("Saving is off for this game")
    moveTo(control("Save state"))
    confirm()
    expect(calls(host)).toHaveLength(4)
    // destructive: NO / YES first; B withdraws the question, then returns to the game.
    moveTo(control("Exit"))
    confirm()
    expect(screen.getByRole("dialog", { name: "Exit" }).textContent).toContain("The game closes")
    expect(focusedText()).toBe("No")
    press("back")
    expect(screen.queryByRole("dialog", { name: "Exit" })).toBeNull()
    expect(calls(host)).toHaveLength(4)
    press("back")
    expect(calls(host).at(-1)).toEqual(["dismissGameplayOverlay"])
    // dismissOnSuccess: Zone invokes and lets the host return to the game.
    publish(paused)
    moveTo(control("Continue"))
    confirm()
    expect(calls(host).at(-1)).toEqual(["invokeGameplayControl", "resume"])
    expect(calls(host).filter(call => call[0] === "dismissGameplayOverlay")).toHaveLength(1)
    // The host closes it: home again, Zone's own page under it.
    publish(HOME)
    expect(document.querySelector(".zone-paused")).toBeNull()
    expect(cardTitles()).toHaveLength(GAMES.length)
  })
})

// ---- C25: first boot ----

describe("onboarding", () => {
  const offline: SurfaceNetwork = { ...NETWORK, connection: { _tag: "Offline" } }
  const pad = { id: "pad-1", name: "Xbox Wireless Controller", heard: false }
  const firstBoot = (heard: boolean, network: SurfaceNetwork = offline): SurfaceModel =>
    ({ ...HOME, network, onboarding: { controllers: [{ ...pad, heard }] } })
  const next = () => button("Next", document.querySelector<HTMLElement>(".zone-onboarding-actions")!)

  test("C25 first boot", () => {
    const { host, publish } = mount(firstBoot(false))
    expect(document.querySelector("[aria-label='Setup steps']")?.textContent).toContain("Controllers")
    expect(onScreen("Waiting")).toBe(true)
    expect(next().closest("[data-disabled], [aria-disabled='true']") !== null || next().disabled).toBe(true)
    // A controller is heard: its name on the card, and NEXT leads on.
    publish(firstBoot(true))
    expect(onScreen("Xbox Wireless Controller")).toBe(true)
    moveTo(next())
    confirm()
    // WI-FI: a secured network goes to OPTIONS' join flow for its password.
    expect(document.querySelector(".zone-onboarding")?.getAttribute("data-step")).toBe("wifi")
    moveTo(button("Neighbour"))
    confirm()
    const password = screen.getByRole("dialog", { name: "Password" })
    type("pw", password)
    moveTo(button("Done", password))
    confirm()
    expect(calls(host)).toEqual([["connectWifi", "n-near", "pw"]])
    // Back on the step; an open network joins at once.
    expect(document.querySelector(".zone-onboarding")?.getAttribute("data-step")).toBe("wifi")
    moveTo(button("Cafe"))
    confirm()
    expect(calls(host).at(-1)).toEqual(["connectWifi", "n-cafe"])
    publish(firstBoot(true, NETWORK))
    moveTo(next())
    confirm()
    // NAME: Korri's device-name setting, typed on the keyboard.
    moveTo(button("Name", document.querySelector<HTMLElement>(".zone-onboarding-group")!))
    confirm()
    type("x")
    moveTo(button("Done", keyboardOnScreen()))
    confirm()
    expect(calls(host).at(-1)).toEqual(["changeSetting", "device-name", "minix"])
    moveTo(next())
    confirm()
    // IDENTITY: START on the NEW SAVE card, then FINISH.
    moveTo(button("New save"))
    confirm()
    moveTo(button("Finish"))
    confirm()
    expect(calls(host).at(-1)).toEqual(["finishOnboarding"])
    publish(HOME)
    expect(document.querySelector(".zone-onboarding")).toBeNull()
    expect(cardTitles()).toHaveLength(GAMES.length)
  })
})

// ---- C26 to C30: chrome and input ----

describe("chrome and input", () => {
  const foot = () => document.querySelector(".zone-foot")?.textContent ?? ""

  test("C26 clock and build", () => {
    const { publish, press } = mount()
    // Korri's clock as the stage timer, its build, and the device's name, on every page.
    for (const open of [() => undefined, () => openOptions(press), () => openSaveData(press)]) {
      open()
      expect(foot()).toContain("21:47")
      expect(foot()).toContain("korri 0.4.1 · 9a1fb22")
      expect(foot()).toContain("mini")
    }
    publish({ ...HOME, clockLabel: undefined })
    expect(document.querySelector(".zone-foot-timer")).toBeNull()
  })

  test("C27 idle", () => {
    jest.useFakeTimers()
    const { press } = mount()
    moveTo(button("Spelunky"))
    act(() => jest.advanceTimersByTime(30_000))
    const titleCard = () => document.querySelector(".zone-title-card-title")?.textContent
    expect(titleCard()).toBe("Spelunky")
    act(() => jest.advanceTimersByTime(4_000))
    expect(titleCard()).toBe("Tetris")
    // Any input wakes it, the cursor where it was.
    press("back")
    expect(titleCard()).toBeUndefined()
    expect(focusedText()).toContain("Spelunky")
    // Idle belongs to the zone select: it never starts over OPTIONS.
    openOptions(press)
    act(() => jest.advanceTimersByTime(60_000))
    expect(titleCard()).toBeUndefined()
  })

  test("C28 input actions", () => {
    const { press } = mount()
    // options: GAME OPTS. on GAMES; nothing on OPTIONS or SAVE DATA.
    press("options")
    expect(onScreen("Game opts.")).toBe(true)
    press("back")
    expect(onScreen("Game opts.")).toBe(false)
    // menu: the cursor to the tabs, on every tab.
    press("menu")
    expect(focusedText()).toBe("Games")
    // d-pad, confirm and back alone reach every tab and come home.
    moveTo(button("Options"))
    confirm()
    press("options")
    expect(onScreen("Game opts.")).toBe(false)
    moveTo(row("Player seats"))
    press("menu")
    expect(focusedText()).toContain("Options")
    moveTo(button("Save data"))
    confirm()
    press("options")
    expect(onScreen("Game opts.")).toBe(false)
    // system: OPTIONS at its SYSTEM group, from anywhere.
    press("system")
    expect(currentTab()).toContain("Options")
    expect(focusedText()).toContain("Pair controller")
    // back: the page's own rule, then the GAMES tab, then nothing.
    press("back")
    expect(currentTab()).toContain("Games")
    expect(focusedText()).toContain("Celeste Classic")
    press("back")
    expect(currentTab()).toContain("Games")
    expect(focusedText()).toContain("Celeste Classic")
  })

  test("C29 text entry", () => {
    const { host, press } = mount()
    openSaveData(press)
    moveTo(button("From backup", document.querySelector<HTMLElement>(".zone-save-data-band")!))
    confirm()
    // A forty-character secret, digits and letters, on the one keyboard.
    // A stand-in, built so it reads as one: never a real backup.
    const backup = `ncryptsec1${"abcd".repeat(7)}90`
    expect(backup).toHaveLength(40)
    type(backup)
    expect(document.querySelector(".zone-keyboard-draft")?.textContent).toContain(backup)
    moveTo(button("Done", keyboardOnScreen()))
    confirm()
    // Capitals and symbols are layers of the same keyboard.
    moveTo(button("ABC", keyboardOnScreen()))
    confirm()
    type("P")
    moveTo(button("#?!", keyboardOnScreen()))
    confirm()
    type("!")
    moveTo(button("Done", keyboardOnScreen()))
    confirm()
    moveTo(button("Transfer"))
    confirm()
    moveTo(button("Yes"))
    confirm()
    expect(calls(host)).toEqual([["switchIdentityFromBackup", backup, "P!", "transfer", true]])
  })

  test("C30 focus and motion", () => {
    const { press } = mount()
    const LIT = "[data-focused]:not([data-focused='off']):not([data-focused='false'])"
    /** DOM focus is on a control, and that control draws itself focused. */
    const drawn = () => {
      const focused = document.activeElement
      expect(focused === null || focused === document.body).toBe(false)
      expect(focused?.closest(LIT) !== null || focused?.querySelector(LIT) !== null).toBe(true)
    }
    drawn()
    moveTo(button("Tetris"))
    drawn()
    press("options")
    drawn()
    press("back")
    drawn()
    press("menu")
    drawn()
    moveTo(button("Options"))
    confirm()
    drawn()
    moveTo(row("Name"))
    drawn()
    confirm()
    drawn()
    press("back")
    drawn()
    press("system")
    drawn()
    press("menu")
    moveTo(button("Save data"))
    confirm()
    drawn()
    moveTo(button("Back up"))
    drawn()
    press("back")
    drawn()
    // Motion answers input and stops for a person who asks for less.
    const css = readFileSync(join(import.meta.dir, "..", "src", "zone.css"), "utf8")
    expect(css).toContain("@media (prefers-reduced-motion: reduce)")
    expect(css).toMatch(/animation: none !important;\s*transition: none !important;/)
  })

  test("C30 focus on first boot's keyboard, PAUSED and RUN WITH", () => {
    const LIT = "[data-focused]:not([data-focused='off']):not([data-focused='false'])"
    /** The one control drawn lit in `scope` is the one with DOM focus. */
    const markOnFocus = (scope: HTMLElement) => {
      const focused = document.activeElement
      expect(focused instanceof HTMLElement && scope.contains(focused)).toBe(true)
      const lit = [...scope.querySelectorAll<HTMLElement>(LIT)]
        .map(node => (node.matches("button, [tabindex]") ? node : node.querySelector<HTMLElement>("button, [tabindex]") ?? node))
      expect(lit).toEqual([focused as HTMLElement])
    }
    const pad = { id: "pad-1", name: "Xbox Wireless Controller", heard: true }
    const { publish } = mount({ ...HOME, onboarding: { controllers: [pad] } })
    // First boot's NAME keyboard: the mark follows the cursor off DONE.
    moveTo(button("Next", document.querySelector<HTMLElement>(".zone-onboarding-actions")!))
    confirm()
    moveTo(button("Next", document.querySelector<HTMLElement>(".zone-onboarding-actions")!))
    confirm()
    moveTo(button("Name", document.querySelector<HTMLElement>(".zone-onboarding-group")!))
    confirm()
    markOnFocus(keyboardOnScreen())
    expect(focusedText()).toBe("Done")
    moveTo(button("q", keyboardOnScreen()))
    markOnFocus(keyboardOnScreen())
    moveTo(button("Space", keyboardOnScreen()))
    markOnFocus(keyboardOnScreen())
    // PAUSED: its rows, then its question.
    const overlay: SurfaceGameplayOverlayPresentation = {
      kind: "gameplay-overlay",
      title: "Wario Land 4",
      controls: [
        { id: "resume", label: "Continue", enabled: true, destructive: false, dismissOnSuccess: true, interaction: { kind: "command" } },
        { id: "exit", label: "Exit", enabled: true, destructive: true, dismissOnSuccess: true, interaction: { kind: "command" } },
      ],
      groups: [],
    }
    publish({ ...HOME, presentation: overlay })
    const wedge = document.querySelector<HTMLElement>(".zone-paused-wedge")!
    markOnFocus(wedge)
    moveTo(button("Exit", wedge))
    markOnFocus(wedge)
    confirm()
    const question = screen.getByRole("dialog", { name: "Exit" })
    markOnFocus(question)
    moveTo(button("Yes", question))
    markOnFocus(question)
    // RUN WITH: Korri's routes and actions.
    publish({
      ...HOME,
      runnerChoice: {
        _tag: "Ready",
        gameTitle: "Wario Land 4",
        message: "Choose what plays it",
        routes: [{ runnerId: "mgba", systemId: "gba", runnerBuild: "/nix/store/abc-mgba", program: "mGBA", warnings: [],
          actions: [{ id: "runner:launch:mgba", label: "Play with mGBA", enabled: true }] }],
        saved: [],
        warnings: [],
        actions: [{ id: "runner:cancel", label: "Close", enabled: true }],
      },
    })
    const page = document.querySelector<HTMLElement>(".zone-run-with")!
    markOnFocus(page)
    moveTo(button("Close", page))
    markOnFocus(page)
    moveTo(button("Play with mGBA", page))
    markOnFocus(page)
  })
})
