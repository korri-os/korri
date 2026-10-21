import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel, SurfaceStore } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { STORE_ENTRIES, STORE_GETS, storeDetails, storeFixture } from "../src/fixtures/store-fixtures"
import { ShiftSurface } from "../src/ShiftSurface"

afterEach(() => cleanup())

const model = (store: SurfaceStore | undefined, overrides: Partial<SurfaceModel> = {}): SurfaceModel => ({
  ...fixtureModel,
  ...(store === undefined ? {} : { store }),
  ...overrides,
})

/** Open the store from the rail, the way a person does. */
function openStore(host: ReturnType<typeof createFixtureHost>) {
  const tile = screen.getByRole("button", { name: "Get games" })
  fireEvent.focus(tile)
  fireEvent.click(tile)
  expect(host.calls).toContain("openStore")
}

function choose(name: string) {
  fireEvent.click(screen.getByRole("button", { name }))
}

describe("Shift store", () => {
  test("the rail offers Get games only where Korri publishes a store", () => {
    const host = createFixtureHost()
    const { rerender } = render(<ShiftSurface host={host} model={model(undefined)} />)
    expect(screen.queryByRole("button", { name: "Get games" })).toBeNull()
    rerender(<ShiftSurface host={host} model={model(storeFixture())} />)
    expect(screen.getByRole("button", { name: "Get games" })).toBeTruthy()
  })

  test("an empty library still reaches the store", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={model(storeFixture(), { catalog: { _tag: "Empty" } })} />)
    openStore(host)
    expect(screen.getByRole("heading", { name: "Get games" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "PICO-8 BBS" })).toBeTruthy()
  })

  test("providers: loading, a refusal with Retry, none turned on, and the list", () => {
    const host = createFixtureHost()
    const { rerender } = render(<ShiftSurface host={host} model={model(storeFixture({ providers: { _tag: "Loading" } }))} />)
    openStore(host)
    expect(screen.getByText("Looking for providers…")).toBeTruthy()
    rerender(<ShiftSurface host={host} model={model(storeFixture({ providers: {
      _tag: "Problem", message: "This portal cannot list providers.", actions: [{ id: "store:1:providers-retry", label: "Retry", enabled: true }],
    } }))} />)
    expect(screen.getByText("This portal cannot list providers.")).toBeTruthy()
    choose("Retry")
    expect(host.calls).toContain("action:store:1:providers-retry")
    rerender(<ShiftSurface host={host} model={model(storeFixture({ providers: { _tag: "Ready", providers: [] } }))} />)
    expect(screen.getByText(/No provider is turned on/)).toBeTruthy()
    rerender(<ShiftSurface host={host} model={model(storeFixture())} />)
    expect(["PICO-8 BBS"].every(name => screen.queryByRole("button", { name }) !== null)).toBe(true)
  })

  test("search: typing and Search ask the one provider shown, then every answer has a body", () => {
    const host = createFixtureHost()
    const idle = storeFixture({ search: { _tag: "Idle" } })
    const { rerender } = render(<ShiftSurface host={host} model={model(idle)} />)
    openStore(host)
    choose("PICO-8 BBS")
    expect(screen.getByRole("heading", { name: "PICO-8 BBS" })).toBeTruthy()
    expect(screen.getByText("Type what to look for, then search PICO-8 BBS.")).toBeTruthy()
    const field = screen.getByRole("searchbox", { name: "Search PICO-8 BBS" })
    fireEvent.change(field, { target: { value: " celeste " } })
    fireEvent.click(screen.getByRole("button", { name: "Search" }))
    expect(host.calls).toContain("searchStore:@korri:pico8:celeste")

    rerender(<ShiftSurface host={host} model={model(storeFixture({ search: { _tag: "Searching", providerId: "@korri:pico8", query: "celeste" } }))} />)
    expect(document.querySelector(".shift-store-empty")?.textContent).toBe("Searching…")
    expect((screen.getByRole("button", { name: "Searching…" }) as HTMLButtonElement).disabled).toBe(true)

    rerender(<ShiftSurface host={host} model={model(storeFixture({ search: {
      _tag: "Problem", providerId: "@korri:pico8", query: "celeste",
      message: "The BBS did not answer. Check the connection and try again.",
      actions: [{ id: "store:2:search-retry", label: "Retry", enabled: true }],
    } }))} />)
    expect(screen.getByText("The BBS did not answer. Check the connection and try again.")).toBeTruthy()
    choose("Retry")
    expect(host.calls).toContain("action:store:2:search-retry")

    rerender(<ShiftSurface host={host} model={model(storeFixture({ search: { _tag: "Ready", providerId: "@korri:pico8", query: "zelda", entries: [] } }))} />)
    expect(screen.getByText("Nothing on PICO-8 BBS matches “zelda”. Try other words.")).toBeTruthy()

    rerender(<ShiftSurface host={host} model={model(storeFixture())} />)
    expect(screen.getAllByRole("button").filter(button => button.classList.contains("shift-store-tile"))).toHaveLength(STORE_ENTRIES.length)
    // A result from another provider is not this page's to show.
    rerender(<ShiftSurface host={host} model={model(storeFixture({
      providers: { _tag: "Ready", providers: [{ id: "@korri:pico8", title: "PICO-8 BBS" }, { id: "@korri:other", title: "Other" }] },
      search: { _tag: "Ready", providerId: "@korri:other", query: "", entries: STORE_ENTRIES },
    }))} />)
    expect(screen.queryAllByRole("button").filter(button => button.classList.contains("shift-store-tile"))).toHaveLength(0)
  })

  test("a result opens its entry, and Back walks home one view at a time without asking Korri", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={model(storeFixture())} />)
    openStore(host)
    choose("PICO-8 BBS")
    choose("Celeste Classic 2, PICO-8")
    expect(host.calls).toContain("openStoreEntry:e:celeste")
    expect(screen.getByRole("heading", { name: "Celeste Classic 2" })).toBeTruthy()
    expect(screen.getByText("PICO-8 BBS · PICO-8")).toBeTruthy()
    const before = host.calls.length
    act(() => host.press("back"))
    expect(screen.getByRole("heading", { name: "PICO-8 BBS" })).toBeTruthy()
    act(() => host.press("back"))
    expect(screen.getByRole("heading", { name: "Get games" })).toBeTruthy()
    act(() => host.press("back"))
    expect(screen.getByRole("button", { name: "Get games" })).toBeTruthy()
    expect(host.calls.length).toBe(before)
  })

  test("the store opens where the person left it, with the typing kept", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={model(storeFixture({ search: { _tag: "Idle" } }))} />)
    openStore(host)
    choose("PICO-8 BBS")
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "celeste" } })
    act(() => host.press("back"))
    act(() => host.press("back"))
    openStore(host)
    choose("PICO-8 BBS")
    expect((screen.getByRole("searchbox") as HTMLInputElement).value).toBe("celeste")
  })

  test.each([
    ["offer", ["Get"], undefined],
    ["cancelled", ["Get"], "Download cancelled. Nothing was kept."],
    ["confirm", ["Download", "Cancel"], "Celeste Classic 2, size unknown"],
    ["downloading", ["Cancel"], "8 KB of 15.9 KB · 8 KB/s · 0:01 left"],
    ["placing", [], undefined],
    ["stopped", ["Retry"], "Korri restarted before the download finished. Nothing was kept, and your library is unchanged."],
  ] as const)("the %s state shows Korri's words and runs exactly its actions", (state, labels, words) => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={model(storeFixture({ details: storeDetails(STORE_GETS[state]) }))} />)
    openStore(host)
    choose("PICO-8 BBS")
    choose("Celeste Classic 2, PICO-8")
    const get = document.querySelector("[data-shift-store-get]")
    expect(get?.getAttribute("data-shift-store-get")).toBe(STORE_GETS[state]._tag)
    const buttons = [...(get?.querySelectorAll("button") ?? [])].map(button => button.textContent)
    expect(buttons).toEqual([...labels])
    if (words !== undefined) expect(screen.getByText(words)).toBeTruthy()
    const first = get?.querySelector("button")
    if (first) {
      fireEvent.click(first)
      expect(host.calls.at(-1)).toBe(`action:${STORE_GETS[state]._tag === "Offer" || STORE_GETS[state]._tag === "Confirm" || STORE_GETS[state]._tag === "Working" || STORE_GETS[state]._tag === "Problem" ? (STORE_GETS[state] as { actions: readonly { id: string }[] }).actions[0]!.id : ""}`)
    }
  })

  test("progress is a real bar from Korri's bytes, and a moving band without a total", () => {
    const host = createFixtureHost()
    const { rerender } = render(<ShiftSurface host={host} model={model(storeFixture({ details: storeDetails(STORE_GETS.downloading) }))} />)
    openStore(host)
    choose("PICO-8 BBS")
    choose("Celeste Classic 2, PICO-8")
    const bar = screen.getByRole("progressbar", { name: "Downloading Celeste Classic 2" })
    expect(bar.getAttribute("aria-valuenow")).toBe("50")
    rerender(<ShiftSurface host={host} model={model(storeFixture({ details: storeDetails(STORE_GETS.unsized) }))} />)
    const band = screen.getByRole("progressbar", { name: "Downloading Celeste Classic 2" })
    expect(band.getAttribute("aria-valuenow")).toBeNull()
    expect(band.hasAttribute("data-indeterminate")).toBe(true)
  })

  test("not available, downloaded-not-recognised, and in the library with Play", () => {
    const host = createFixtureHost()
    const { rerender } = render(<ShiftSurface host={host} model={model(storeFixture({ details: storeDetails(STORE_GETS.unavailable) }))} />)
    openStore(host)
    choose("PICO-8 BBS")
    choose("Celeste Classic 2, PICO-8")
    expect(screen.getByText(STORE_GETS.unavailable.reason)).toBeTruthy()
    expect(document.querySelectorAll("[data-shift-store-get] button")).toHaveLength(0)
    rerender(<ShiftSurface host={host} model={model(storeFixture({ details: storeDetails(STORE_GETS.staged) }))} />)
    expect(screen.getByText(STORE_GETS.staged.message)).toBeTruthy()
    rerender(<ShiftSurface host={host} model={model(storeFixture({ details: storeDetails(STORE_GETS.inLibrary) }))} />)
    expect(screen.getByText("In your library")).toBeTruthy()
    choose("▶ Play")
    expect(host.calls.at(-1)).toBe("launch:local-game:wl4")
    // Play leaves the store for the launch.
    expect(screen.queryByRole("heading", { name: "Celeste Classic 2" })).toBeNull()
  })

  test("in the library before the catalog has it: no Play to press yet", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={model(storeFixture({ details: storeDetails({ _tag: "InLibrary", detail: "In your library" }) }))} />)
    openStore(host)
    choose("PICO-8 BBS")
    choose("Celeste Classic 2, PICO-8")
    expect(screen.queryByRole("button", { name: "▶ Play" })).toBeNull()
    expect(screen.getByText("In your library")).toBeTruthy()
  })

  test("details that are loading or failed have their own bodies", () => {
    const host = createFixtureHost()
    const entry = STORE_ENTRIES[0]!
    const { rerender } = render(<ShiftSurface host={host} model={model(storeFixture({ details: { _tag: "Loading", entry } }))} />)
    openStore(host)
    choose("PICO-8 BBS")
    choose("Celeste Classic 2, PICO-8")
    expect(screen.getByText("Reading this game…")).toBeTruthy()
    rerender(<ShiftSurface host={host} model={model(storeFixture({ details: {
      _tag: "Problem", entry, message: "The BBS did not answer.", actions: [{ id: "store:9:details-retry", label: "Retry", enabled: true }],
    } }))} />)
    choose("Retry")
    expect(host.calls.at(-1)).toBe("action:store:9:details-retry")
  })
})

describe("Shift answers Confirm (#115)", () => {
  test("a question before a launch puts exactly Korri's answers on the rail", () => {
    const host = createFixtureHost()
    render(<ShiftSurface host={host} model={model(undefined, { status: {
      _tag: "Confirm", kicker: "Needs a download", detail: "FEX rootfs and Proton-CachyOS, 1.5 GB", gameId: "local-game:wl4",
      actions: [
        { id: "acquisition:3:download", label: "Download", enabled: true },
        { id: "acquisition:3:cancel", label: "Cancel", enabled: true },
      ],
    } })} />)
    const download = screen.getByRole("button", { name: "Download" })
    expect(document.activeElement === download).toBe(true)
    expect(screen.getByText("Needs a download")).toBeTruthy()
    expect(screen.getByText("FEX rootfs and Proton-CachyOS, 1.5 GB")).toBeTruthy()
    expect(screen.queryByRole("button", { name: "Skate 3" })).toBeNull()
    fireEvent.click(download)
    expect(host.calls).toEqual(["action:acquisition:3:download"])
    // The host moves focus first, then confirms: a press acts on what has focus.
    const cancel = screen.getByRole("button", { name: "Cancel" })
    fireEvent.focus(cancel)
    expect(screen.getByRole("heading", { name: "Cancel" })).toBeTruthy()
    fireEvent.click(cancel)
    expect(host.calls).toEqual(["action:acquisition:3:download", "action:acquisition:3:cancel"])
  })

  test("a question outranks the store and the library, as work under way does", () => {
    const host = createFixtureHost()
    const { rerender } = render(<ShiftSurface host={host} model={model(storeFixture())} />)
    openStore(host)
    rerender(<ShiftSurface host={host} model={model(storeFixture(), { status: {
      _tag: "Confirm", kicker: "Needs a download", actions: [{ id: "acquisition:1:download", label: "Download", enabled: true }],
    } })} />)
    expect(screen.queryByRole("heading", { name: "Get games" })).toBeNull()
    expect(screen.getByRole("button", { name: "Download" })).toBeTruthy()
  })
})
