/**
 * Zone as a whole, from home, the way the portal drives it: one test per
 * capability item (docs/design/surfaces/CAPABILITIES.md, C1 to C33).
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
 *
 * Every capability runs once per look (Modern, Blend, Retro): the device
 * keeps the look in `zone.look` before Zone mounts, as a person who picked
 * it on OPTIONS' LOOK row left it, and every mount checks the root wears
 * it. A look changes how Zone is drawn, never what it can do.
 */
import { afterEach, beforeEach, describe, expect, jest, test } from "bun:test"
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
  SurfaceStore,
  SurfaceStoreGet,
} from "@contracts/surface/korri-surface"
import { ZoneSurface } from "../src/index"
import { ZONE_LOOKS, type ZoneLook } from "../src/plan/look"
import { ZONE_LOOK_KEY } from "../src/zone-look-preference"
import { byWords } from "./words"

/** The look the capabilities run in now (each runs once per look). */
let look: ZoneLook = "blend"

afterEach(() => {
  cleanup()
  jest.useRealTimers()
  localStorage.removeItem(ZONE_LOOK_KEY)
})

// ---- Korri's facts ----

/** 19 Oct 2026, 12:00 UTC: the same day in every time zone from -11 to +11. */
const LAST_PLAYED = 1_792_411_200_000
const PIXEL = "data:image/gif;base64,R0lGODlhAQABAAAAACw="
/** Wide art only: a different image, so the test sees which one the card drew. */
const WIDE = "data:image/gif;base64,R0lGODlhAgABAAAAACw="

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
  { id: "wario", title: "Wario Land 4", subtitle: "GBA", section: "zao", wideArtUrl: WIDE },
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
          interaction: { kind: "sensitiveText", placeholder: "Paste API key", clearLabel: "Clear saved key" },
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
    openStore: record("openStore"),
    searchStore: record("searchStore"),
    openStoreEntry: record("openStoreEntry"),
  }
}

// ---- Driving it as the portal does ----

function mount(model: SurfaceModel = HOME, actions: readonly SurfaceAction[] = GAME_ACTIONS) {
  const host = fakeHost(actions)
  const view = render(<ZoneSurface host={host} model={model} />)
  // Zone opened in the look the device kept.
  expect(document.querySelector(".zone-screen")?.getAttribute("data-look")).toBe(look)
  /** Korri publishes a new model. */
  const publish = (next: SurfaceModel) => act(() => view.rerender(<ZoneSurface host={host} model={next} />))
  /** The host's back, options, menu or system. */
  const press = (action: SurfaceInputAction) => act(() => host.press(action))
  return { host, publish, press }
}

/** The button whose words are `text`, anywhere on screen. */
const button = (text: string | RegExp, scope: HTMLElement = document.body): HTMLButtonElement => {
  const found = within(scope).getAllByText(byWords(text)).map(node => node.closest("button")).find(node => node !== null)
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
const onScreen = (text: string | RegExp) => screen.queryAllByText(byWords(text)).length > 0
const currentTab = () => document.querySelector("[role='tablist'] [aria-current='page']")?.textContent ?? ""
const cardTitles = () => [...document.querySelectorAll(".zone-card-title")].map(node => node.textContent)
const calls = (host: FakeHost) => host.calls.map(call => [call.method, ...call.args])

/** The yellow-on-blue banner is drawn once: on the active tab. */
const oneBanner = (tab: string) => {
  const banners = [...document.querySelectorAll(".zone-banner")]
  expect(banners).toHaveLength(1)
  expect(banners[0]?.closest("[role='tablist']")?.querySelector("[aria-current='page']")?.textContent).toContain(tab)
}

/** From home to OPTIONS with the d-pad: menu to the tabs, right, A. */
function openOptions(press: (action: SurfaceInputAction) => void) {
  press("menu")
  moveTo(button("Options"))
  confirm()
  expect(currentTab()).toContain("Options")
  oneBanner("Options")
}

/** From home to SAVE DATA with the d-pad. */
function openSaveData(press: (action: SurfaceInputAction) => void) {
  press("menu")
  moveTo(button("Save data"))
  confirm()
  expect(currentTab()).toContain("Save data")
  oneBanner("Save data")
}

/** An OPTIONS row by its label. */
const row = (label: string | RegExp) => button(label, document.querySelector<HTMLElement>(".zone-options-scroll")!)

for (const each of ZONE_LOOKS) {
  describe(`look: ${each}`, () => {
    beforeEach(() => {
      look = each
      localStorage.setItem(ZONE_LOOK_KEY, each)
    })

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
        // Cover art fills the well; a game with only wide art wears that, letterboxed; none wears the stripe well with initials.
        const well = (title: string) => button(title).querySelector(".zone-cover-well")?.getAttribute("data-filled")
        expect(well("Hollow Knight")).toBe("art")
        expect(well("Spelunky")).toBe("initials")
        expect(well("Wario Land 4")).toBe("art")
        expect(button("Wario Land 4").querySelector(".zone-cover-well img")?.getAttribute("src")).toBe(WIDE)
        // The d-pad walks the band.
        moveTo(button("Tetris"))
        expect(document.querySelector(".zone-card[data-focused='true'] .zone-card-title")?.textContent).toBe("Tetris")
      })

      test("C3 game facts", () => {
        mount()
        expect(document.querySelector(".zone-games-facts")?.textContent).toBe("Last played19 Oct 2026Plays23Time11h 31m")
        expect(button("Continue", document.querySelector<HTMLElement>(".zone-games-actions")!)).toBeTruthy()
        moveTo(button("Hollow Knight"))
        // Never recorded: TIME ATTACK's empty record, not a blank.
        expect(document.querySelector(".zone-games-facts")?.hasAttribute("data-empty")).toBe(true)
        expect(document.querySelector(".zone-games-facts")?.textContent).toBe(`Last played--Plays--Time--'--"--`)
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
        // d-pad and A alone: OPTS. at the end of the action row, never the options button.
        const { host } = mount()
        const opts = () => button("Opts.", document.querySelector<HTMLElement>(".zone-games-actions")!)
        moveTo(button("Spelunky"))
        moveTo(opts())
        confirm()
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
        moveTo(opts())
        confirm()
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

      test("C33 download", () => {
        const needing = withGames(GAMES.map(game =>
          game.id === "hollow" ? { ...game, download: { bytes: 1_500_000_000 } }
            : game.id === "spelunky" ? { ...game, download: {} }
            : game))
        const { host, publish } = mount(needing)
        const tags = (title: string) => [...button(title).querySelectorAll(".zone-tag")].map(tag => tag.textContent)
        const facts = () => document.querySelector(".zone-games-facts")?.textContent ?? ""
        // The card says so, with Korri's size formatted; a size Korri does not know is the word alone.
        expect(tags("Hollow Knight")).toEqual(["Switch", "Here", "Download 1.5 GB"])
        expect(tags("Spelunky")).toEqual(["PC", "Here", "zao", "Download"])
        expect(tags("Celeste Classic")).not.toContain("Download")
        // The facts line under the band adds it for the focused game.
        moveTo(button("Hollow Knight"))
        expect(facts()).toBe(`Last played--Plays--Time--'--"--Download1.5 GB`)
        expect(document.querySelector(".zone-games-facts")?.hasAttribute("data-empty")).toBe(false)
        moveTo(button("Spelunky"))
        expect(facts()).toBe(`Last played--Plays--Time--'--"--Download`)
        moveTo(button("Celeste Classic"))
        expect(facts()).not.toContain("Download")
        // Nothing else changes: A launches as before, and the portal raises its question.
        moveTo(button("Hollow Knight"))
        confirm()
        expect(calls(host)).toEqual([["launchGame", "hollow"]])
        publish({
          ...needing,
          status: {
            _tag: "Confirm",
            kicker: "Needs a download",
            detail: "FEX rootfs and Proton-CachyOS, 1.5 GB",
            gameId: "hollow",
            actions: [
              { id: "acquisition:1:download", label: "Download", enabled: true },
              { id: "acquisition:1:cancel", label: "Cancel", enabled: true },
            ],
          },
        })
        expect(focusedText()).toBe("Download")
        confirm()
        expect(calls(host).at(-1)).toEqual(["runAction", "acquisition:1:download"])
        // Korri fetched it: the game no longer carries `download`, and the tag and fact go.
        publish(withGames(GAMES))
        expect(tags("Hollow Knight")).toEqual(["Switch", "Here"])
        expect(facts()).not.toContain("Download")
      })

      test("C10 reload", () => {
        const { host, publish } = mount()
        publish({ ...HOME, catalog: { _tag: "Error", message: "The disk did not answer." } })
        expect(focusedText()).toBe("Reload")
        confirm()
        expect(calls(host)).toEqual([["reload"]])
      })

      test("C31 confirm", () => {
        const { host, publish, press } = mount()
        moveTo(button("Hollow Knight"))
        confirm()
        const question: SurfaceModel = {
          ...HOME,
          status: {
            _tag: "Confirm",
            kicker: "Needs a download",
            detail: "FEX rootfs and Proton-CachyOS, 1.5 GB",
            gameId: "hollow",
            actions: [
              { id: "acquisition:1:download", label: "Download", enabled: true },
              { id: "acquisition:1:cancel", label: "Cancel", enabled: true },
            ],
          },
        }
        publish(question)
        const dialog = screen.getByRole("dialog", { name: "Needs a download" })
        expect(dialog.textContent).toContain("FEX rootfs and Proton-CachyOS, 1.5 GB")
        expect(dialog.textContent).toContain("Hollow Knight")
        expect(within(dialog).getAllByRole("button").map(node => node.textContent)).toEqual(["Download", "Cancel"])
        // The page is inert and dimmed under it; the cursor is on the host's first action.
        expect(document.querySelector(".zone-status-layer-page")?.hasAttribute("inert")).toBe(true)
        expect(document.querySelector(".zone-status-layer-page")?.hasAttribute("data-covered")).toBe(true)
        expect(focusedText()).toBe("Download")
        // B answers nothing, and the other buttons leave the question up.
        press("back")
        press("menu")
        press("options")
        press("system")
        expect(calls(host)).toEqual([["launchGame", "hollow"]])
        expect(screen.getByRole("dialog", { name: "Needs a download" })).toBeTruthy()
        // The last action is reported by its id like any other: none is read as no.
        moveTo(button("Cancel", dialog))
        confirm()
        expect(calls(host).at(-1)).toEqual(["runAction", "acquisition:1:cancel"])
        // Korri answers: the cursor goes back to the card.
        publish(HOME)
        expect(screen.queryByRole("dialog")).toBeNull()
        expect(focusedText()).toContain("Hollow Knight")
        // Asked again, A on the lit first action runs it.
        publish(question)
        expect(focusedText()).toBe("Download")
        confirm()
        expect(calls(host).at(-1)).toEqual(["runAction", "acquisition:1:download"])
      })

      test("C32 progress", () => {
        const { host, publish, press } = mount()
        moveTo(button("Hollow Knight"))
        confirm()
        const cancel = { id: "acquisition:1:cancel", label: "Cancel", enabled: true }
        const busy = (progress?: { readonly done: number; readonly total?: number }, detail?: string): SurfaceModel => ({
          ...HOME,
          status: {
            _tag: "Busy",
            kicker: "Downloading FEX rootfs",
            gameId: "hollow",
            actions: [cancel],
            ...(detail === undefined ? {} : { detail }),
            ...(progress === undefined ? {} : { progress }),
          },
        })
        // A known end: a real bar, its share and both sizes.
        publish(busy({ done: 612_000_000, total: 1_160_000_000 }, "612 MB of 1.2 GB · 6.4 MB/s · 1:30 left"))
        const strip = screen.getByRole("status")
        expect(strip.textContent).toContain("Downloading FEX rootfs")
        expect(strip.textContent).toContain("612 MB of 1.2 GB · 6.4 MB/s · 1:30 left")
        expect(document.querySelector(".zone-status-layer-busy")?.textContent).toContain("Hollow Knight")
        const bar = within(strip).getByRole("progressbar")
        expect(bar.getAttribute("data-meter")).toBe("progress")
        expect(bar.getAttribute("aria-valuenow")).toBe("52")
        // The figures once: Korri's detail is the meter's readout; Zone's are its value text.
        expect(bar.textContent).toBe("612 MB of 1.2 GB · 6.4 MB/s · 1:30 left")
        expect(bar.getAttribute("aria-valuetext")).toBe("612.0 MB / 1.2 GB")
        // Korri counts on: the same strip, the bar moves, the cursor stays on CANCEL.
        publish(busy({ done: 1_154_000_000, total: 1_160_000_000 }))
        expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("99")
        expect(screen.getByRole("progressbar").textContent).toBe("1.2 GB/1.2 GB")
        expect(focusedText()).toBe("Cancel")
        // How far, with no end: the size so far and no share claimed.
        publish(busy({ done: 212_400_000 }))
        expect(screen.getByRole("progressbar").getAttribute("data-meter")).toBe("indeterminate")
        expect(screen.getByRole("progressbar").hasAttribute("aria-valuenow")).toBe(false)
        expect(screen.getByRole("progressbar").textContent).toBe("212.4 MB")
        // B cancels nothing; Korri's CANCEL is reported by its id.
        press("back")
        expect(calls(host)).toEqual([["launchGame", "hollow"]])
        confirm()
        expect(calls(host).at(-1)).toEqual(["runAction", "acquisition:1:cancel"])
        // Without progress the strip says only that work is under way.
        publish(busy(undefined, "Keeping what was downloaded so far"))
        expect(screen.queryByRole("progressbar")).toBeNull()
        expect(screen.getByRole("status").textContent).toContain("Keeping what was downloaded so far")
        publish(HOME)
        expect(focusedText()).toContain("Hollow Knight")
      })
    })

    // ---- GET GAMES: a game from a provider ----

    describe("get games", () => {
      const PICO8 = "@korri:pico8"
      const CELESTE_2 = { id: "celeste_classic_2-5", title: "Celeste Classic 2", system: "PICO-8" }
      const WIP = { id: "wip-3", title: "untitled work in progress" }
      const action = (id: string, label: string): SurfaceAction => ({ id, label, enabled: true })
      const store = (over: Partial<SurfaceStore> = {}): SurfaceStore => ({
        providers: { _tag: "Ready", providers: [{ id: PICO8, title: "PICO-8 BBS" }] },
        search: { _tag: "Idle" },
        details: { _tag: "Closed" },
        ...over,
      })
      const found = store({ search: { _tag: "Ready", providerId: PICO8, query: "cel", entries: [CELESTE_2, WIP] } })
      const entry = (get: SurfaceStoreGet): SurfaceStore =>
        ({ ...found, details: { _tag: "Ready", entry: CELESTE_2, description: "Madeline climbs again.", get } })
      const EMPTY: SurfaceModel = { ...HOME, catalog: { _tag: "Ready", games: [] } }
      const page = () => document.querySelector(".zone-store")

      test("an empty library offers GET GAMES; a game is found, got and played", () => {
        const { host, publish } = mount({ ...EMPTY, store: store({ providers: { _tag: "Loading" } }) })
        expect(onScreen("No games")).toBe(true)
        // The one thing to do on an empty library has the cursor.
        expect(focusedText()).toBe("Get games")
        confirm()
        expect(calls(host)).toEqual([["openStore"]])
        expect(onScreen("Loading")).toBe(true)

        publish({ ...EMPTY, store: store() })
        expect(focusedText()).toContain("PICO-8 BBS")
        confirm()
        // A provider not asked yet opens on the keyboard.
        type("cel")
        moveTo(button("Done", keyboardOnScreen()))
        confirm()
        expect(calls(host).at(-1)).toEqual(["searchStore", PICO8, "cel"])

        publish({ ...EMPTY, store: store({ search: { _tag: "Searching", providerId: PICO8, query: "cel" } }) })
        expect(onScreen("Searching")).toBe(true)
        // Korri's trouble in its words, with its RETRY.
        publish({ ...EMPTY, store: store({ search: {
          _tag: "Problem", providerId: PICO8, query: "cel",
          message: "The provider did not answer.", actions: [action("store:1:search-retry", "Retry")],
        } }) })
        expect(onScreen("The provider did not answer.")).toBe(true)
        moveTo(button("Retry"))
        confirm()
        expect(calls(host).at(-1)).toEqual(["runAction", "store:1:search-retry"])

        // Results: the system as the provider says it, UNKNOWN when it does not.
        publish({ ...EMPTY, store: found })
        expect(button("Celeste Classic 2").textContent).toContain("PICO-8")
        expect(button("untitled work in progress").textContent).toContain("Unknown")
        moveTo(button("Celeste Classic 2"))
        confirm()
        expect(calls(host).at(-1)).toEqual(["openStoreEntry", "celeste_classic_2-5"])

        publish({ ...EMPTY, store: { ...found, details: { _tag: "Loading", entry: CELESTE_2 } } })
        expect(onScreen("Loading")).toBe(true)
        publish({ ...EMPTY, store: entry({ _tag: "Offer", actions: [action("store:2:get", "Get")] }) })
        expect(onScreen("Madeline climbs again.")).toBe(true)
        expect(focusedText()).toBe("Get")
        confirm()
        expect(calls(host).at(-1)).toEqual(["runAction", "store:2:get"])

        // Korri's question, its own buttons in its order, the first lit.
        publish({ ...EMPTY, store: entry({
          _tag: "Confirm", kicker: "Download to this device", detail: "Celeste Classic 2, 15.9 KB",
          actions: [action("store:3:download", "Download"), action("store:3:cancel", "Cancel")],
        }) })
        expect(screen.getByRole("dialog", { name: "Download to this device" }).textContent).toContain("15.9 KB")
        expect(focusedText()).toBe("Download")
        confirm()
        expect(calls(host).at(-1)).toEqual(["runAction", "store:3:download"])

        // The meter from Korri's bytes, and its CANCEL.
        publish({ ...EMPTY, store: entry({
          _tag: "Working", kicker: "Downloading Celeste Classic 2", detail: "8 KB of 15.9 KB",
          progress: { done: 8_000, total: 15_872 }, actions: [action("store:4:cancel", "Cancel")],
        }) })
        expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("50")
        expect(focusedText()).toBe("Cancel")

        // Waiting its turn: QUEUED, Korri's words, still CANCEL.
        publish({ ...EMPTY, store: entry({
          _tag: "Working", kicker: "Waiting to download", detail: "Waiting for the download before it",
          queued: true, actions: [action("store:5:cancel", "Cancel")],
        }) })
        expect(onScreen("Queued")).toBe(true)
        expect(screen.queryByRole("progressbar")).toBeNull()

        // In the library: PLAY launches it, and GAMES comes back on its card.
        const got: SurfaceGame = { id: "bbs:celeste_classic_2-5", title: "Celeste Classic 2", subtitle: "PICO-8" }
        const library: SurfaceModel = { ...HOME, catalog: { _tag: "Ready", games: [...GAMES, got] } }
        publish({ ...library, store: entry({ _tag: "InLibrary", detail: "In your library", gameId: got.id }) })
        expect(onScreen("In your library")).toBe(true)
        expect(focusedText()).toBe("Play")
        confirm()
        expect(calls(host).at(-1)).toEqual(["launchGame", got.id])
        expect(page()).toBeNull()
        expect(focusedText()).toContain("Celeste Classic 2")
      })

      test("B walks back out and never cancels; menu, options and system do nothing there", () => {
        const working = entry({
          _tag: "Working", kicker: "Downloading Celeste Classic 2",
          progress: { done: 8_000, total: 15_872 }, actions: [action("store:4:cancel", "Cancel")],
        })
        const { host, press, publish } = mount({ ...HOME, store: found })
        press("menu")
        press("menu")
        press("menu")
        // Menu: the tabs, FIND, then GET GAMES.
        expect(focusedText()).toBe("Get games")
        confirm()
        moveTo(button("PICO-8 BBS"))
        confirm()
        moveTo(button("Celeste Classic 2"))
        confirm()
        publish({ ...HOME, store: working })
        expect(focusedText()).toBe("Cancel")
        for (const action of ["menu", "options", "system"] as const) press(action)
        expect(page()).not.toBeNull()
        const asked = calls(host).length
        press("back")
        // The results, the cursor on the entry it came from; the download runs on.
        expect(focusedText()).toContain("Celeste Classic 2")
        press("back")
        expect(focusedText()).toContain("PICO-8 BBS")
        press("back")
        expect(page()).toBeNull()
        expect(focusedText()).toBe("Get games")
        expect(calls(host).length).toBe(asked)
      })

      test("a host that cannot get games shows no GET GAMES", () => {
        mount(EMPTY)
        expect(screen.queryByText("Get games")).toBeNull()
        mount(HOME)
        expect(screen.queryAllByText("Get games")).toEqual([])
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
        // The banner stands where the tabs stand on every other page: the page is its whole shell.
        const page = document.querySelector<HTMLElement>(".zone-run-with")!.closest<HTMLElement>(".zone-shell")!
        for (const words of ["Run with", "Wario Land 4", "Ready", "Choose what plays it", "Two runners are installed",
          "mGBA", "/nix/store/abc-mgba-0.10.3", "BIOS missing: games may not boot", "VBA-M", "This game"]) {
          expect(page.textContent).toContain(words)
        }
        // A program and its build are facts: they keep their case.
        const mgba = [...page.querySelectorAll(".zone-option-row")].find(node => node.textContent?.includes("mGBA"))
        expect(mgba?.querySelector(".zone-option-row-label")?.getAttribute("data-case")).toBe("verbatim")
        expect(mgba?.querySelector(".zone-option-row-value")?.getAttribute("data-case")).toBe("verbatim")
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
        const { press, publish } = mount()
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
        // Korri's value is a fact and keeps its case; the label is set in caps.
        const kept = (node: Element | null | undefined) => node?.getAttribute("data-case") === "verbatim"
        expect(kept(row("Name").querySelector(".zone-option-row-value"))).toBe(true)
        expect(kept(row("Name").querySelector(".zone-option-row-label"))).toBe(false)
        expect(kept(row("Games").querySelector(".zone-option-row-value"))).toBe(true)
        // No name from Korri: the row shows Korri's placeholder, dimmed, in its place.
        const unnamed = HOME.settings.map(group => ({
          ...group,
          items: group.items.map(item => (item.id === "device-name" ? { ...item, value: undefined } : item)),
        }))
        publish({ ...HOME, settings: unnamed })
        const value = row("Name").querySelector(".zone-option-row-text")
        expect(value?.textContent).toBe("This device")
        expect(value?.hasAttribute("data-placeholder")).toBe(true)
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
        // sensitiveText: Korri's status word on the row as given; dots only for what is typed.
        expect(row("SteamGridDB API key").textContent).toContain("Configured")
        moveTo(row("SteamGridDB API key"))
        expect(onScreen("Clear saved key")).toBe(true)
        confirm()
        const secret = screen.getByRole("dialog", { name: "SteamGridDB API key" })
        // Korri's placeholder on the empty slab, dimmed, until the first key.
        expect(secret.querySelector(".zone-keyboard-placeholder")?.textContent).toBe("Paste API key")
        type("ab1", secret)
        expect(secret.querySelector(".zone-keyboard-placeholder")).toBeNull()
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
        // A key is a fact: it keeps its case on the card.
        expect(retired.querySelector(".zone-save-card-text")?.getAttribute("data-case")).toBe("verbatim")
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
        expect(ask.querySelector(".zone-dialog-title")?.getAttribute("data-case")).toBe("verbatim")
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
        // Strongest first, with strength, LOCKED or OPEN, and SAVED.
        const listed = [...document.querySelectorAll(".zone-options-scroll .zone-option-row-label")]
          .map(label => label.textContent).filter(label => ["hummingbird-lake", "Neighbour", "Cafe"].includes(label ?? ""))
        expect(listed).toEqual(["hummingbird-lake", "Neighbour", "Cafe"])
        // A listed network's row, not the connection row that also names it.
        expect(network("hummingbird-lake").textContent).toContain("72%")
        expect(network("hummingbird-lake").textContent).toContain("LOCKED SAVED")
        expect(network("Neighbour").textContent).toContain("48%")
        expect(network("Neighbour").textContent).toContain("LOCKED")
        expect(network("Neighbour").textContent).not.toContain("SAVED")
        expect(network("Cafe").textContent).toContain("OPEN")
        expect(network("Cafe").textContent).not.toContain("LOCKED")
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
        // destructive, answered: nothing is invoked until YES.
        publish(HOME)
        publish(paused)
        const before = calls(host).length
        moveTo(control("Exit"))
        confirm()
        const exit = screen.getByRole("dialog", { name: "Exit" })
        expect(calls(host)).toHaveLength(before)
        moveTo(button("Yes", exit))
        expect(calls(host)).toHaveLength(before)
        confirm()
        expect(calls(host).slice(before)).toEqual([["invokeGameplayControl", "exit"]])
        // dismissOnSuccess: Zone invokes and lets the host return to the game.
        publish(HOME)
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
        // IDENTITY: START on the save card headed NEW, then FINISH.
        moveTo(button("New"))
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
        // The attract gives each game 6 s: its title card, then its stage (M6).
        act(() => jest.advanceTimersByTime(6_000))
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

    // ---- The moments: the launch title card, the results tally, the attract ----

    describe("moments", () => {
      const launchCard = () => document.querySelector<HTMLElement>(".zone-launch-card")
      const tallyOnScreen = () => document.querySelector<HTMLElement>(".zone-tally")
      const tallyRows = () => [...document.querySelectorAll(".zone-tally-row")]
        .map(row => [row.querySelector(".zone-tally-label")?.textContent, row.querySelector(".zone-tally-value")?.textContent])
      const celesteAfter = (plays: number, seconds: number | undefined): SurfaceModel => withGames(GAMES.map(game =>
        game.id === "celeste" ? { ...game, playCount: plays, totalPlaytimeSeconds: seconds } : game))
      const running = (gameId: string, model: SurfaceModel = HOME): SurfaceModel =>
        ({ ...model, status: { _tag: "Running", kicker: "Playing", gameId } })

      test("title card on launch", () => {
        jest.useFakeTimers()
        const { host, publish } = mount()
        // CONTINUE on Celeste, played 23 times: ACT 24, the system on its tag, ZONE on the bar.
        confirm()
        expect(calls(host)).toEqual([["launchGame", "celeste"]])
        expect(launchCard()?.querySelector(".zone-title-card-title")?.textContent).toBe("Celeste Classic")
        expect(launchCard()?.querySelector("[role='img']")?.getAttribute("aria-label")).toBe("Act 24")
        expect(launchCard()?.querySelector(".zone-title-card-bar")?.textContent).toBe("Zone")
        expect(launchCard()?.querySelector(".zone-tag")?.textContent).toBe("PICO-8")
        // Korri takes it up: Busy, then Running. The card is the status layer's
        // stage: the busy strip's kicker stands at its foot, and no veil lies over it.
        publish({ ...HOME, status: { _tag: "Busy", kicker: "Starting", gameId: "celeste" } })
        const stage = document.querySelector<HTMLElement>(".zone-status-layer-stage")
        expect(launchCard()?.closest(".zone-status-layer-stage")).toBe(stage)
        expect(screen.getByRole("status").textContent).toContain("Starting")
        expect(screen.getByText("Starting").closest(".zone-status-layer-busy")?.getAttribute("data-at")).toBe("foot")
        expect(stage?.querySelector(".zone-veil")).toBeNull()
        // Korri asks over the card: the question stands on top of it, veiled under
        // it, and the host's action answers by runAction; the card holds.
        publish({
          ...HOME,
          status: {
            _tag: "Confirm",
            kicker: "Needs a download",
            detail: "FEX rootfs, 1.5 GB",
            gameId: "celeste",
            actions: [
              { id: "acquisition:1:download", label: "Download", enabled: true },
              { id: "acquisition:1:cancel", label: "Cancel", enabled: true },
            ],
          },
        })
        const question = screen.getByRole("dialog", { name: "Needs a download" })
        expect(launchCard()?.hasAttribute("data-leaving")).toBe(false)
        // The question and its veil are drawn after the stage, outside it: over the card.
        const veil = question.previousElementSibling
        expect(veil?.classList.contains("zone-veil")).toBe(true)
        expect(stage?.contains(veil!)).toBe(false)
        expect((stage!.compareDocumentPosition(veil!) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0).toBe(true)
        expect(focusedText()).toBe("Download")
        confirm()
        expect(calls(host).at(-1)).toEqual(["runAction", "acquisition:1:download"])
        publish({ ...HOME, status: { _tag: "Busy", kicker: "Starting", gameId: "celeste" } })
        expect(launchCard()?.hasAttribute("data-leaving")).toBe(false)
        publish(running("celeste"))
        expect(launchCard()?.hasAttribute("data-leaving")).toBe(false)
        expect(document.querySelector(".zone-games-page")?.hasAttribute("inert")).toBe(true)
        // Korri lets go of the game: the card leaves (M3 out, 400 ms) and the cursor is home.
        publish(HOME)
        expect(launchCard()?.hasAttribute("data-leaving")).toBe(true)
        act(() => jest.advanceTimersByTime(400))
        expect(launchCard()).toBeNull()
        expect(focusedText()).toContain("Celeste Classic")
        // A game Korri never counted opens ACT 1; a problem before Korri takes it up ends the card.
        moveTo(button("Hollow Knight"))
        confirm()
        expect(launchCard()?.querySelector("[role='img']")?.getAttribute("aria-label")).toBe("Act 1")
        publish({ ...HOME, status: { _tag: "Problem", kicker: "Could not start", reason: "zao did not answer.", canRetry: false, gameId: "hollow" } })
        expect(launchCard()?.hasAttribute("data-leaving")).toBe(true)
        act(() => jest.advanceTimersByTime(400))
        expect(launchCard()).toBeNull()
      })

      test("results tally", () => {
        jest.useFakeTimers()
        const { publish, press } = mount()
        // Celeste runs from 23 plays and 11h 31m, and comes back with 24 and 72'14" more.
        publish(running("celeste"))
        publish(celesteAfter(24, 41_460 + 4_334))
        expect(tallyOnScreen()?.textContent).toContain("Celeste Classic")
        expect(tallyRows()).toEqual([["Time", "72'14\""], ["Plays", "24"], ["Total", "12H 43M"]])
        expect(focusedText()).toContain("Celeste Classic")
        expect(document.activeElement?.closest(".zone-tally")).not.toBeNull()
        // The first press skips the count-up; the tally stays.
        press("back")
        expect(tallyOnScreen()?.hasAttribute("data-leaving")).toBe(false)
        // The next closes it, and does nothing else.
        press("menu")
        expect(tallyOnScreen()?.hasAttribute("data-leaving")).toBe(true)
        act(() => jest.advanceTimersByTime(300))
        expect(tallyOnScreen()).toBeNull()
        expect(currentTab()).toContain("Games")
        expect(focusedText()).toContain("Celeste Classic")
        // Alone, it closes after 8 s. Korri did not move the total: no TIME row.
        publish(running("celeste", celesteAfter(24, 45_794)))
        publish(celesteAfter(25, 45_794))
        expect(tallyRows()).toEqual([["Plays", "25"], ["Total", "12H 43M"]])
        act(() => jest.advanceTimersByTime(8_000))
        expect(tallyOnScreen()?.hasAttribute("data-leaving")).toBe(true)
        act(() => jest.advanceTimersByTime(300))
        expect(tallyOnScreen()).toBeNull()
        // Ambiguous: the count rose by two, or did not rise. No tally.
        publish(running("celeste", celesteAfter(25, 45_794)))
        publish(celesteAfter(27, 50_000))
        expect(tallyOnScreen()).toBeNull()
        publish(running("celeste", celesteAfter(27, 50_000)))
        publish(celesteAfter(27, 50_000))
        expect(tallyOnScreen()).toBeNull()
      })

      test("attract wakes to the same card", () => {
        jest.useFakeTimers()
        const { press } = mount()
        moveTo(button("Spelunky"))
        act(() => jest.advanceTimersByTime(30_000))
        const attract = () => document.querySelector<HTMLElement>(".zone-attract")
        const titleCard = () => attract()?.querySelector(".zone-title-card-title")?.textContent
        // Spelunky's turn: its title card, never played (ACT 1), then its stage with the HUD.
        expect(titleCard()).toBe("Spelunky")
        expect(attract()?.querySelector("[role='img']")?.getAttribute("aria-label")).toBe("Act 1")
        expect(attract()?.textContent).toContain("Press any button")
        expect(attract()?.textContent).toContain("21:47")
        // Two turns on: Wario Land 4's stage is its wide art.
        act(() => jest.advanceTimersByTime(12_000))
        expect(titleCard()).toBe("Wario Land 4")
        expect(attract()?.querySelector(".zone-attract-art")?.getAttribute("src")).toBe(WIDE)
        // Any input wakes it, on the card the person left, not the attract's.
        press("menu")
        expect(attract()).toBeNull()
        expect(focusedText()).toContain("Spelunky")
        expect(document.querySelector(".zone-card[data-focused='true'] .zone-card-title")?.textContent).toBe("Spelunky")
      })
    })
  })
}
