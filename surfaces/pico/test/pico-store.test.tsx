/**
 * Getting a game from a provider, through PicoSurface and the treaty: the
 * MENU entry, the providers, the keyboard, the results, the entry in each
 * state, and the download on the launch stage. Each assertion is about what
 * the player sees or what Korri would be asked.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel, SurfaceStore } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { STORE_GETS, storeDetails, storeFixture } from "../src/fixtures/store"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

const model = (store: SurfaceStore | undefined, extra: Partial<SurfaceModel> = {}): SurfaceModel =>
  ({ ...fixtureModel, ...(store === undefined ? {} : { store }), ...extra })

function openMenu() {
  fireEvent.click(screen.getByRole("button", { name: /MENU/ }))
}

function intoStore(host: ReturnType<typeof createFixtureHost>) {
  openMenu()
  fireEvent.click(screen.getByRole("button", { name: "GET GAMES" }))
  expect(host.calls).toContain("openStore")
}

describe("the store's door", () => {
  test("MENU offers GET GAMES only where Korri publishes a store", () => {
    const host = createFixtureHost()
    const { rerender } = render(<PicoSurface host={host} model={model(undefined)} />)
    openMenu()
    expect(screen.queryByRole("button", { name: "GET GAMES" })).toBeNull()
    cleanup()
    render(<PicoSurface host={host} model={model(storeFixture())} />)
    openMenu()
    expect(screen.getByRole("button", { name: "GET GAMES" })).toBeTruthy()
    void rerender
  })

  test("an empty library still reaches the store", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model(storeFixture(), { catalog: { _tag: "Empty" } })} />)
    intoStore(host)
    expect(screen.getByRole("button", { name: /PICO-8 BBS/ })).toBeTruthy()
  })
})

describe("providers, search and results", () => {
  test("the providers, the cursor on the first, and each one's page", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model(storeFixture())} />)
    intoStore(host)
    const first = screen.getByRole("button", { name: /PICO-8 BBS/ })
    expect(document.activeElement === first).toBe(true)
    fireEvent.click(screen.getByRole("button", { name: /Homebrew Hub/ }))
    // Homebrew Hub was searched already: its results show, under SEARCH.
    expect(screen.getByRole("button", { name: /SEARCH/ })).toBeTruthy()
    expect(screen.getByRole("button", { name: "2048, Game Boy" })).toBeTruthy()
  })

  test("a provider not yet searched opens the Wi-Fi keyboard; SEARCH sends the words", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={model(storeFixture({ search: { _tag: "Idle" } }))} />)
    intoStore(host)
    fireEvent.click(screen.getByRole("button", { name: /PICO-8 BBS/ }))
    expect(screen.getByRole("form", { name: "Search PICO-8 BBS" })).toBeTruthy()
    for (const key of ["c", "e", "l"]) fireEvent.click(screen.getByRole("button", { name: `Type ${key}` }))
    fireEvent.click(screen.getByRole("button", { name: "SEARCH" }))
    expect(host.calls.at(-1)).toBe("searchStore:@korri:pico8:cel")
  })

  test("searching, no answer with Retry, and nothing found each have their body", () => {
    const host = createFixtureHost()
    const pico = "@korri:pico8"
    const { rerender } = render(<PicoSurface host={host} model={model(storeFixture({ search: { _tag: "Searching", providerId: pico, query: "celeste" } }))} />)
    intoStore(host)
    fireEvent.click(screen.getByRole("button", { name: /PICO-8 BBS/ }))
    expect(screen.getByText("Asking PICO-8 BBS for “celeste”.")).toBeTruthy()
    rerender(<PicoSurface host={host} model={model(storeFixture({ search: {
      _tag: "Problem", providerId: pico, query: "celeste", message: "The provider did not answer. Check the connection and try again.",
      actions: [{ id: "store:3:search-retry", label: "Retry", enabled: true }],
    } }))} />)
    expect(screen.getByText("The provider did not answer. Check the connection and try again.")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Retry" }))
    expect(host.calls.at(-1)).toBe("action:store:3:search-retry")
    rerender(<PicoSurface host={host} model={model(storeFixture({ search: { _tag: "Ready", providerId: pico, query: "zelda", entries: [] } }))} />)
    expect(screen.getByText(/Nothing on PICO-8 BBS matches “zelda”/)).toBeTruthy()
  })
})

describe("an entry", () => {
  function atEntry(store: SurfaceStore) {
    const host = createFixtureHost()
    const view = render(<PicoSurface host={host} model={model(store)} />)
    intoStore(host)
    fireEvent.click(screen.getByRole("button", { name: /Homebrew Hub/ }))
    fireEvent.click(screen.getByRole("button", { name: "2048, Game Boy" }))
    expect(host.calls.at(-1)).toBe("openStoreEntry:e:2048")
    return { host, ...view }
  }

  test("Get is offered and runs Korri's action", () => {
    const { host } = atEntry(storeFixture())
    expect(screen.getByRole("heading", { name: "2048" })).toBeTruthy()
    expect(screen.getByText("Slide the tiles and join equal numbers to reach 2048.")).toBeTruthy()
    // Not a game played here: the stage says where it stands, not "never played".
    expect(screen.getByText("NOT ON THIS DEVICE")).toBeTruthy()
    expect(screen.queryByText("NEVER PLAYED")).toBeNull()
    fireEvent.click(screen.getByRole("button", { name: "GET" }))
    expect(host.calls.at(-1)).toBe("action:store:1:get")
  })

  test.each([
    ["confirm", "Download to this device", "ask", ["Download", "Cancel"]],
    ["folders", "Choose a folder", "ask", ["Game Boy", "SD card · Game Boy", "Cancel"]],
    ["unavailable", "NOT AVAILABLE", "tell", []],
    ["staged", "DOWNLOADED", "tell", []],
    ["failed", "Download failed", "warn", ["Retry"]],
    ["stopped", "Download stopped", "warn", ["Retry"]],
  ] as const)("%s is a card in Korri's words with exactly Korri's actions", (state, kicker, tone, labels) => {
    const { host } = atEntry(storeFixture({ details: storeDetails(STORE_GETS[state]) }))
    const card = screen.getByRole("region", { name: kicker })
    expect(card.querySelector(".pico-card")?.getAttribute("data-tone")).toBe(tone)
    const buttons = [...card.querySelectorAll("button")].map(button => button.textContent)
    expect(buttons).toEqual([...labels])
    const first = card.querySelector("button")
    if (first !== null) {
      fireEvent.click(first)
      expect(host.calls.at(-1)?.startsWith("action:store:")).toBe(true)
    }
  })

  test("in the library: PLAY launches the catalog game", () => {
    const { host } = atEntry(storeFixture({ details: storeDetails(STORE_GETS.inLibrary) }))
    fireEvent.click(screen.getByRole("button", { name: "▶ PLAY" }))
    expect(host.calls.at(-1)).toBe("launch:hollow")
  })

  test("a download takes the launch stage: a real bar, Korri's detail, and Cancel", () => {
    const { host, rerender } = atEntry(storeFixture({ details: storeDetails(STORE_GETS.downloading) }))
    const bar = screen.getByRole("progressbar", { name: "Downloading 2048" })
    expect(bar.getAttribute("aria-valuenow")).toBe("48")
    expect(screen.getByText("16 KB of 32.8 KB · 16 KB/s · 0:01 left")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }))
    expect(host.calls.at(-1)).toBe("action:store:5:cancel")
    rerender(<PicoSurface host={host} model={model(storeFixture({ details: storeDetails(STORE_GETS.unsized) }))} />)
    expect(document.querySelector(".pico-launch-stage-meter")?.getAttribute("data-meter")).toBe("indeterminate")
    rerender(<PicoSurface host={host} model={model(storeFixture({ details: storeDetails(STORE_GETS.placing) }))} />)
    expect(screen.getByText("Adding to your library…")).toBeTruthy()
    expect(screen.queryByRole("button", { name: "Cancel" })).toBeNull()
  })

  test("Back from a download goes to the results and asks Korri nothing", () => {
    const { host } = atEntry(storeFixture({ details: storeDetails(STORE_GETS.downloading) }))
    const before = host.calls.length
    act(() => host.press("back"))
    expect(screen.getByRole("button", { name: "2048, Game Boy" })).toBeTruthy()
    expect(host.calls.length).toBe(before)
  })
})
