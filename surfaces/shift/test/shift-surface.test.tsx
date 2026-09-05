import { afterEach, describe, expect, test } from "bun:test"
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react"
import { readFileSync } from "node:fs"
import type {
  SurfaceGameplayOverlayPresentation,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import {
  createFixtureHost,
  fixtureModel,
} from "../src/fixtures/fixture-host"
import {
  shiftDetailGameFromSurfaceGame,
  shiftGameFromSurfaceGame,
  shiftHomeGamesFromCatalog,
  shiftLibraryGameFromSurfaceGame,
  ShiftSurface,
} from "../src/ShiftSurface"
import { shiftPartPreviews } from "../src/preview"

const model = (overrides: Partial<SurfaceModel> = {}): SurfaceModel => ({
  ...fixtureModel,
  ...overrides,
})

afterEach(() => cleanup())

describe("ShiftSurface", () => {
  test("keeps the full catalog in Library instead of pouring it onto Home", () => {
    render(<ShiftSurface model={model()} host={createFixtureHost()} />)

    const homeTiles = screen.getAllByRole("button", {
      name: /Skate 3|Wario Land 4|Neverball/,
    })
    expect(homeTiles).toHaveLength(2)
    expect(homeTiles.map(tile => tile.getAttribute("aria-label"))).toContain(
      "Skate 3",
    )

    const library = screen.getByRole("button", { name: "Library" })
    fireEvent.focus(library)
    fireEvent.click(library)
    const libraryTiles = screen.getAllByRole("button", {
      name: /Skate 3|Wario Land 4|Neverball/,
    })
    expect(libraryTiles.map(tile => tile.getAttribute("aria-label"))).toEqual([
      "Neverball",
      "Skate 3",
      "Wario Land 4",
    ])
  })

  test("builds Continue plus one rotating pick without duplicating it", () => {
    const games = [
      {
        id: "running",
        title: "Running",
        tileArtUrl: "",
        wideArtUrl: "",
        resumable: true,
      },
      { id: "a", title: "A", tileArtUrl: "", wideArtUrl: "" },
      { id: "b", title: "B", tileArtUrl: "", wideArtUrl: "" },
    ]

    expect(shiftHomeGamesFromCatalog(games, () => 1)).toEqual([
      {
        id: "running",
        title: "Running",
        tileArtUrl: "",
        wideArtUrl: "",
        resumable: true,
        section: "Continue",
      },
      {
        id: "b",
        title: "B",
        tileArtUrl: "",
        wideArtUrl: "",
        section: "Random",
      },
    ])
  })

  test("Continue leads with the running session, then recently played newest first", () => {
    const day = 24 * 60 * 60 * 1000
    const games = [
      { id: "old", title: "Old", tileArtUrl: "", wideArtUrl: "", lastPlayedAt: 1 * day },
      { id: "never", title: "Never", tileArtUrl: "", wideArtUrl: "" },
      { id: "new", title: "New", tileArtUrl: "", wideArtUrl: "", lastPlayedAt: 3 * day },
      {
        id: "running",
        title: "Running",
        tileArtUrl: "",
        wideArtUrl: "",
        resumable: true,
      },
      { id: "mid", title: "Mid", tileArtUrl: "", wideArtUrl: "", lastPlayedAt: 2 * day },
    ]

    const home = shiftHomeGamesFromCatalog(games, () => 0)
    expect(home.map(game => [game.id, game.section])).toEqual([
      ["running", "Continue"],
      ["new", "Continue"],
      ["mid", "Continue"],
      ["old", "Continue"],
      ["never", "Random"],
    ])
  })

  test("formats Korri's raw play facts into Shift's own labels", () => {
    const now = new Date("2026-09-04T18:00:00.000Z")
    const played = {
      id: "wl4",
      title: "Wario Land 4",
      lastPlayedAt: Date.UTC(2026, 8, 4, 15, 0, 0),
      playCount: 3,
      totalPlaytimeSeconds: 5400,
    }

    const scene = shiftGameFromSurfaceGame(played, now)
    expect(scene.lastPlayedLabel).toBe("3h ago")
    expect(scene.playtimeLabel).toBe("1.5h")
    expect(scene.lastPlayedAt).toBe(played.lastPlayedAt)

    const detail = shiftDetailGameFromSurfaceGame(played, now)
    expect(detail.lastPlayedLabel).toBe("3h ago")
    expect(detail.playtimeLabel).toBe("1.5h")

    const library = shiftLibraryGameFromSurfaceGame(played)
    expect(library.lastPlayedAt).toBe(played.lastPlayedAt)
    expect(library.playtimeMinutes).toBe(90)

    const never = shiftDetailGameFromSurfaceGame(
      { id: "n", title: "Never" },
      now,
    )
    expect(never.lastPlayedLabel).toBeUndefined()
    expect(never.playtimeLabel).toBeUndefined()

    const running = shiftDetailGameFromSurfaceGame(
      { ...played, resumable: true },
      now,
    )
    expect(running.lastPlayedLabel).toBe("Ready to continue")
  })

  test("draws a title monogram when the host has no cover art", () => {
    const { container } = render(
      <ShiftSurface model={model()} host={createFixtureHost()} />,
    )

    expect(container.querySelectorAll("img")).toHaveLength(0)
    expect(container.querySelectorAll(".shift-monogram").length).toBe(2)
  })

  test("confirms destructive setting actions before calling the host", () => {
    const host = createFixtureHost()
    render(
      <ShiftSurface
        model={model({
          settings: [
            {
              title: "Games",
              items: [
                {
                  id: "game-folder:loc-a",
                  label: "GBA",
                  value: "Registered",
                  interaction: {
                    kind: "action",
                    actionId: "game-folder-remove:loc-a",
                    destructive: true,
                    confirmation: {
                      title: "Remove game folder?",
                      message: "Korri will remove games it added from this folder.",
                      confirmLabel: "Remove folder",
                    },
                  },
                },
              ],
            },
          ],
        })}
        host={host}
      />,
    )

    fireEvent.focus(screen.getByRole("button", { name: "Settings" }))
    fireEvent.click(screen.getByRole("button", { name: "Settings" }))
    fireEvent.click(screen.getByRole("button", { name: /GBA/ }))

    expect(host.calls).toEqual([])
    expect(screen.getByRole("dialog", { name: "Remove game folder?" })).toBeDefined()
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }))
    expect(host.calls).toEqual([])

    fireEvent.click(screen.getByRole("button", { name: /GBA/ }))
    fireEvent.click(screen.getByRole("button", { name: "Remove folder" }))
    expect(host.calls).toEqual(["action:game-folder-remove:loc-a"])
  })

  test("opens configured sensitive settings with an empty password editor", () => {
    const host = createFixtureHost()
    render(
      <ShiftSurface
        model={model({
          settings: [
            {
              title: "Metadata",
              items: [
                {
                  id: "steamgriddb-credential",
                  label: "SteamGridDB API key",
                  value: "Configured",
                  interaction: {
                    kind: "sensitiveText",
                    placeholder: "Paste API key",
                    maxLength: 256,
                  },
                },
              ],
            },
          ],
        })}
        host={host}
      />,
    )

    const settings = screen.getByRole("button", { name: "Settings" })
    fireEvent.focus(settings)
    fireEvent.click(settings)
    fireEvent.click(screen.getByRole("button", { name: /SteamGridDB API key/ }))

    const input = screen.getByLabelText("SteamGridDB API key") as HTMLInputElement
    expect(input.getAttribute("type")).toBe("password")
    expect(input.value).toBe("")
    expect(screen.queryByDisplayValue("Configured")).toBeNull()
    expect(screen.queryByRole("button", { name: "Clear" })).toBeNull()
  })

  test("renders sensitive Clear only from the host clear capability", () => {
    const host = createFixtureHost()
    render(
      <ShiftSurface
        model={model({
          settings: [
            {
              title: "Metadata",
              items: [
                {
                  id: "steamgriddb-credential",
                  label: "SteamGridDB API key",
                  value: "Configurado",
                  interaction: {
                    kind: "sensitiveText",
                    placeholder: "Paste API key",
                    clearLabel: "Clear saved key",
                  },
                },
              ],
            },
          ],
        })}
        host={host}
      />,
    )

    const settings = screen.getByRole("button", { name: "Settings" })
    fireEvent.focus(settings)
    fireEvent.click(settings)
    fireEvent.click(screen.getByRole("button", { name: /SteamGridDB API key/ }))
    fireEvent.click(screen.getByRole("button", { name: "Clear saved key" }))

    expect(host.calls).toEqual(["setting:steamgriddb-credential:"])
  })

  test("setup actions do not pollute the game rail", () => {
    render(<ShiftSurface model={model()} host={createFixtureHost()} />)

    expect(screen.queryByRole("button", { name: "Pair a device" })).toBeNull()
    expect(screen.getByRole("button", { name: "Settings" })).toBeDefined()
  })

  test("confirming a tile asks the host to launch that game", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={model()} host={host} />)

    // The first tile is focused at mount, so a single activation launches it.
    fireEvent.click(screen.getByRole("button", { name: "Skate 3" }))
    expect(host.calls).toEqual(["launch:now-playing:L1"])
  })

  test("busy work shows the host's words, never an error", () => {
    render(
      <ShiftSurface
        model={model({
          status: { _tag: "Busy", kicker: "Starting Wario Land 4…" },
        })}
        host={createFixtureHost()}
      />,
    )

    expect(screen.getByText("Starting Wario Land 4…")).toBeDefined()
  })

  test("a problem offers Back, and back dismisses it", () => {
    const host = createFixtureHost()
    render(
      <ShiftSurface
        model={model({
          status: {
            _tag: "Problem",
            kicker: "Couldn't start",
            reason: "local ROM is missing",
            canRetry: false,
          },
        })}
        host={host}
      />,
    )

    expect(screen.getByText("local ROM is missing")).toBeDefined()
    act(() => host.press("back"))
    expect(host.calls).toEqual(["dismiss"])
  })

  test("a failure is shown against its own game, not the focused hero", () => {
    const originalRandom = Math.random
    Math.random = () => 0.99
    try {
      const { container } = render(
        <ShiftSurface
          model={model({
            status: {
              _tag: "Problem",
              kicker: "Couldn't start Wario Land 4",
              reason: "ActiveSessionConflict: a session must end",
              canRetry: false,
              gameId: "local-game:wl4",
              gameTitle: "Wario Land 4",
            },
          })}
          host={createFixtureHost()}
        />,
      )

      // Home focuses Skate 3 and the seeded random slot picks Neverball, so
      // Wario Land 4 is outside the rendered Home rail.
      expect(screen.queryByRole("button", { name: "Wario Land 4" })).toBeNull()
      // Scope to this render: sibling suites leave their own heroes mounted.
      const hero = container.querySelector("[data-korri-part='shift.cine-hero']")
      expect(hero?.getAttribute("data-korri-instance-id")).toBe("local-game:wl4")
      expect(screen.getByText("Couldn't start Wario Land 4")).toBeDefined()
    } finally {
      Math.random = originalRandom
    }
  })

  test("a retryable problem routes A to retry", () => {
    const host = createFixtureHost()
    render(
      <ShiftSurface
        model={model({
          status: {
            _tag: "Problem",
            kicker: "Couldn't start",
            reason: "That device is offline",
            canRetry: true,
          },
        })}
        host={host}
      />,
    )

    fireEvent.click(screen.getByRole("button", { name: "Skate 3" }))
    expect(host.calls).toEqual(["retry"])
  })

  test("loading, empty, and error each get their own body", () => {
    const host = createFixtureHost()
    const loading = render(
      <ShiftSurface model={model({ catalog: { _tag: "Loading" } })} host={host} />,
    )
    expect(screen.getByText("Loading library…")).toBeDefined()
    loading.unmount()

    const empty = render(
      <ShiftSurface
        model={model({ catalog: { _tag: "Empty" }, settings: [] })}
        host={host}
      />,
    )
    expect(screen.getByText("No games found.")).toBeDefined()
    empty.unmount()

    render(
      <ShiftSurface
        model={model({ catalog: { _tag: "Error", message: "brain offline" } })}
        host={host}
      />,
    )
    expect(screen.getByText("Could not load library.")).toBeDefined()
    expect(screen.getByText("brain offline")).toBeDefined()
    fireEvent.click(screen.getByRole("button", { name: "Retry" }))
    expect(host.calls).toEqual(["reload"])
  })

  test("Options is silent when the host offers no game actions", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={model()} host={host} />)

    expect(screen.queryByText("Options")).toBeNull()
    act(() => host.press("options"))
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  test("Options opens the host's actions for the focused game", () => {
    const host = createFixtureHost({
      "now-playing:L1": [
        { id: "resume", label: "Continue playing", enabled: true },
        { id: "stop", label: "Stop", enabled: true, destructive: true },
      ],
    })
    render(<ShiftSurface model={model()} host={host} />)

    expect(screen.getByText("Options")).toBeDefined()
    act(() => host.press("options"))

    const sheet = screen.getByRole("dialog")
    expect(sheet.getAttribute("aria-label")).toBe("Actions for Skate 3")
    fireEvent.click(screen.getByRole("button", { name: "Stop" }))
    expect(host.calls).toEqual(["game-action:now-playing:L1:stop"])
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  test("disabled game actions stay visible but native activation cannot run them", () => {
    const host = createFixtureHost({
      "now-playing:L1": [
        { id: "resume", label: "Continue playing", enabled: false },
      ],
    })
    render(<ShiftSurface model={model()} host={host} />)

    act(() => host.press("options"))

    const action = screen.getByRole<HTMLButtonElement>("button", {
      name: "Continue playing",
    })
    expect(action.disabled).toBe(true)
    fireEvent.click(action)
    action.click()

    expect(host.calls).toEqual([])
  })
})

describe("Shift gameplay overlay", () => {
  const presentation: SurfaceGameplayOverlayPresentation = {
    kind: "gameplay-overlay",
    title: "Skate 3",
    controls: [
      {
        id: "overlay:resume",
        label: "Resume",
        enabled: true,
        destructive: false,
        dismissOnSuccess: true,
        interaction: { kind: "command" },
      },
    ],
    groups: [
      {
        id: "@korri:moonlight",
        label: "Streaming",
        controls: [
          {
            id: "keyboard",
            label: "Keyboard",
            description: "Show the streaming keyboard.",
            enabled: true,
            destructive: false,
            dismissOnSuccess: true,
            interaction: { kind: "command" },
          },
          {
            id: "fill",
            label: "Fill screen",
            description: "Crop the stream to fill the display.",
            enabled: true,
            destructive: false,
            dismissOnSuccess: false,
            interaction: {
              kind: "toggle",
              value: true,
              trueLabel: "crop to fill",
              falseLabel: "fit (letterbox)",
            },
          },
          {
            id: "mouse-mode",
            label: "Mouse mode",
            description: "Choose how pointer input behaves.",
            enabled: true,
            destructive: false,
            dismissOnSuccess: false,
            interaction: {
              kind: "choice",
              value: "trackpad",
              options: [
                { value: "trackpad", label: "Trackpad" },
                { value: "direct", label: "Direct" },
              ],
            },
          },
          {
            id: "sharpness",
            label: "Sharpness",
            description: "Tune stream sharpening.",
            enabled: true,
            destructive: false,
            dismissOnSuccess: false,
            interaction: {
              kind: "range",
              value: 50,
              min: 0,
              max: 100,
              step: 5,
            },
          },
          {
            id: "quit",
            label: "Quit host game",
            enabled: true,
            destructive: true,
            dismissOnSuccess: true,
            interaction: { kind: "command" },
          },
        ],
      },
      {
        id: "@korri:retroarch",
        label: "RetroArch",
        controls: [
          {
            id: "retroarch-menu",
            label: "Open RetroArch menu",
            description: "Open the emulator menu over the game.",
            enabled: false,
            disabledReason: "The game menu is unavailable right now.",
            destructive: false,
            dismissOnSuccess: true,
            interaction: { kind: "command" },
          },
        ],
      },
    ],
  }

  const overlayModel = (overrides: Partial<SurfaceModel> = {}): SurfaceModel =>
    model({
      presentation,
      catalog: { _tag: "Empty" },
      status: { _tag: "Browsing" },
      actions: [],
      settings: [],
      ...overrides,
    })

  test("renders the exact sheet composition, ordering, groups, and initial Resume focus", () => {
    render(<ShiftSurface model={overlayModel()} host={createFixtureHost()} />)

    const dialog = screen.getByRole("dialog", { name: "Gameplay controls for Skate 3" })
    expect(within(dialog).getByText("Skate 3")).toBeDefined()
    expect(within(dialog).getByText("Streaming")).toBeDefined()
    expect(within(dialog).getByText("RetroArch")).toBeDefined()
    expect(
      Array.from(
        dialog.querySelectorAll<HTMLElement>(
          ".shift-sheet-action, .shift-sheet-control",
        ),
      ).map(row =>
        row.querySelector(".shift-sheet-control-label")?.textContent ??
        row.querySelector(".shift-sheet-action-label")?.textContent,
      ),
    ).toEqual([
      "Resume",
      "Keyboard",
      "Fill screen",
      "Mouse mode",
      "Sharpness",
      "Quit host game",
      "Open RetroArch menu",
    ])
    expect(document.activeElement).toBe(
      within(dialog).getByRole("button", { name: "Resume" }),
    )
    expect(
      within(dialog).getByRole("button", { name: "Quit host game" }).getAttribute("data-tone"),
    ).toBe("danger")
    expect(
      within(dialog).getByRole("button", { name: /Open RetroArch menu/ }).getAttribute(
        "aria-disabled",
      ),
    ).toBe("true")
  })

  test("uses dedicated controls for toggle, choice, and range with touch and ARIA values", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={overlayModel()} host={host} />)

    expect(screen.getByText("crop to fill")).toBeDefined()
    fireEvent.click(screen.getByRole("button", { name: "Keyboard" }))
    fireEvent.click(screen.getByRole("switch", { name: "Fill screen" }))
    expect(screen.getByText("fit (letterbox)")).toBeDefined()
    fireEvent.change(screen.getByRole("combobox", { name: "Mouse mode" }), {
      target: { value: "direct" },
    })
    const sharpness = screen.getByRole("slider", { name: "Sharpness" })
    fireEvent.change(sharpness, { target: { value: "55" } })
    fireEvent.change(sharpness, { target: { value: "60" } })
    fireEvent.change(sharpness, { target: { value: "55" } })
    expect(host.calls).toEqual([
      "gameplay-control:keyboard",
      'gameplay-control:fill:{"kind":"toggle","value":false}',
      'gameplay-control:mouse-mode:{"kind":"choice","value":"direct"}',
    ])
    fireEvent.pointerUp(sharpness)

    expect(host.calls).toEqual([
      "gameplay-control:keyboard",
      'gameplay-control:fill:{"kind":"toggle","value":false}',
      'gameplay-control:mouse-mode:{"kind":"choice","value":"direct"}',
      'gameplay-control:sharpness:{"kind":"range","value":55}',
    ])
    expect(screen.getByRole("switch", { name: "Fill screen" }).getAttribute("aria-checked"))
      .toBe("false")
    expect(screen.getByRole<HTMLSelectElement>("combobox", { name: "Mouse mode" }).value)
      .toBe("direct")
    expect(screen.getByRole<HTMLInputElement>("slider", { name: "Sharpness" }).value)
      .toBe("55")
    expect(screen.getByRole("slider", { name: "Sharpness" }).getAttribute("min")).toBe("0")
    expect(screen.getByRole("slider", { name: "Sharpness" }).getAttribute("max")).toBe("100")
    expect(screen.getByRole("slider", { name: "Sharpness" }).getAttribute("step")).toBe("5")
  })

  test("horizontal repeat commits one final range value on release", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={overlayModel()} host={host} />)
    const choice = screen.getByRole("combobox", { name: "Mouse mode" })
    const range = screen.getByRole("slider", { name: "Sharpness" })

    act(() => {
      choice.dispatchEvent(
        new CustomEvent("korri-semantic-direction", {
          detail: { direction: "right", repeat: false },
        }),
      )
      choice.dispatchEvent(
        new CustomEvent("korri-semantic-direction", {
          detail: { direction: "right", repeat: true },
        }),
      )
      range.dispatchEvent(
        new CustomEvent("korri-semantic-direction", {
          detail: {
            direction: "right",
            repeat: false,
            releaseExpected: true,
            source: "gamepad",
            gestureId: 1,
          },
        }),
      )
      range.dispatchEvent(
        new CustomEvent("korri-semantic-direction", {
          detail: {
            direction: "right",
            repeat: true,
            releaseExpected: true,
            source: "gamepad",
            gestureId: 1,
          },
        }),
      )
    })

    expect(host.calls).toEqual([
      'gameplay-control:mouse-mode:{"kind":"choice","value":"direct"}',
    ])
    act(() => {
      range.dispatchEvent(
        new CustomEvent("korri-semantic-direction-end", {
          detail: { direction: "right", source: "gamepad", gestureId: 1 },
        }),
      )
    })
    expect(host.calls).toEqual([
      'gameplay-control:mouse-mode:{"kind":"choice","value":"direct"}',
      'gameplay-control:sharpness:{"kind":"range","value":60}',
    ])
  })

  test("slow repeats and opposite presses remain newest-wins until release", async () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={overlayModel()} host={host} />)
    const range = screen.getByRole("slider", { name: "Sharpness" })

    act(() => {
      range.dispatchEvent(new CustomEvent("korri-semantic-direction", {
        detail: {
          direction: "right",
          repeat: false,
          releaseExpected: true,
          source: "gamepad",
          gestureId: 1,
        },
      }))
    })
    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 220))
    })
    act(() => {
      range.dispatchEvent(new CustomEvent("korri-semantic-direction", {
        detail: {
          direction: "right",
          repeat: true,
          releaseExpected: true,
          source: "gamepad",
          gestureId: 1,
        },
      }))
      range.dispatchEvent(new CustomEvent("korri-semantic-direction", {
        detail: {
          direction: "left",
          repeat: false,
          releaseExpected: true,
          source: "gamepad",
          gestureId: 2,
        },
      }))
    })
    expect(host.calls).toEqual([])
    act(() => {
      range.dispatchEvent(new CustomEvent("korri-semantic-direction-end", {
        detail: { direction: "right", source: "gamepad", gestureId: 1 },
      }))
    })
    expect(host.calls).toEqual([])
    act(() => {
      range.dispatchEvent(new CustomEvent("korri-semantic-direction", {
        detail: {
          direction: "left",
          repeat: true,
          releaseExpected: true,
          source: "gamepad",
          gestureId: 2,
        },
      }))
      range.dispatchEvent(new CustomEvent("korri-semantic-direction-end", {
        detail: { direction: "left", source: "gamepad", gestureId: 2 },
      }))
    })
    expect(host.calls).toEqual([
      'gameplay-control:sharpness:{"kind":"range","value":50}',
    ])
  })

  test("cross-source gesture ids cannot release one another", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={overlayModel()} host={host} />)
    const range = screen.getByRole("slider", { name: "Sharpness" })

    act(() => {
      range.dispatchEvent(new CustomEvent("korri-semantic-direction", {
        detail: {
          direction: "right",
          repeat: false,
          releaseExpected: true,
          source: "gamepad",
          gestureId: 1,
        },
      }))
      range.dispatchEvent(new CustomEvent("korri-semantic-direction", {
        detail: {
          direction: "right",
          repeat: true,
          releaseExpected: true,
          source: "keyboard",
          gestureId: 1,
        },
      }))
      range.dispatchEvent(new CustomEvent("korri-semantic-direction-end", {
        detail: { direction: "right", source: "gamepad", gestureId: 1 },
      }))
    })
    expect(host.calls).toEqual([])
    act(() => {
      range.dispatchEvent(new CustomEvent("korri-semantic-direction-end", {
        detail: { direction: "right", source: "keyboard", gestureId: 1 },
      }))
    })
    expect(host.calls).toEqual([
      'gameplay-control:sharpness:{"kind":"range","value":60}',
    ])
  })

  test("an identified release cannot flush an unidentified pending gesture", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={overlayModel()} host={host} />)
    const range = screen.getByRole("slider", { name: "Sharpness" })

    act(() => {
      range.dispatchEvent(new CustomEvent("korri-semantic-direction", {
        detail: {
          direction: "right",
          repeat: false,
          releaseExpected: true,
          source: "gamepad",
        },
      }))
      range.dispatchEvent(new CustomEvent("korri-semantic-direction-end", {
        detail: { direction: "right", source: "gamepad", gestureId: 99 },
      }))
    })
    expect(host.calls).toEqual([])
    act(() => {
      range.dispatchEvent(new CustomEvent("korri-semantic-direction-end", {
        detail: { direction: "right", source: "gamepad" },
      }))
    })
    expect(host.calls).toEqual([
      'gameplay-control:sharpness:{"kind":"range","value":55}',
    ])
  })

  test("resets optimistic values when an authoritative range value changes", async () => {
    const host = createFixtureHost()
    const rendered = render(
      <ShiftSurface model={overlayModel()} host={host} />,
    )

    fireEvent.click(screen.getByRole("switch", { name: "Fill screen" }))
    fireEvent.change(screen.getByRole("combobox", { name: "Mouse mode" }), {
      target: { value: "direct" },
    })
    fireEvent.change(screen.getByRole("slider", { name: "Sharpness" }), {
      target: { value: "55" },
    })

    const refreshedPresentation: SurfaceGameplayOverlayPresentation = {
      ...presentation,
      groups: presentation.groups.map(group => ({
        ...group,
        controls: group.controls.map(control =>
          control.id === "sharpness" && control.interaction.kind === "range"
            ? {
                ...control,
                interaction: { ...control.interaction, value: 45 },
              }
            : { ...control },
        ),
      })),
    }
    rendered.rerender(
      <ShiftSurface
        model={overlayModel({
          presentation: refreshedPresentation,
          status: {
            _tag: "Problem",
            kicker: "Controls unavailable",
            reason: "Try again.",
            canRetry: true,
          },
        })}
        host={host}
      />,
    )

    expect(screen.getByRole("switch", { name: "Fill screen" }).getAttribute("aria-checked"))
      .toBe("true")
    expect(screen.getByRole<HTMLSelectElement>("combobox", { name: "Mouse mode" }).value)
      .toBe("trackpad")
    expect(screen.getByRole<HTMLInputElement>("slider", { name: "Sharpness" }).value)
      .toBe("45")
    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 2100))
    })
    expect(host.calls).not.toContain(
      'gameplay-control:sharpness:{"kind":"range","value":55}',
    )
  })

  test("equivalent parent renders preserve one pending range commit", () => {
    const host = createFixtureHost()
    const rendered = render(<ShiftSurface model={overlayModel()} host={host} />)
    fireEvent.change(screen.getByRole("slider", { name: "Sharpness" }), {
      target: { value: "55" },
    })

    rendered.rerender(
      <ShiftSurface
        model={overlayModel({
          presentation: {
            ...presentation,
            groups: presentation.groups.map(group => ({
              ...group,
              controls: group.controls.map(control => ({ ...control })),
            })),
          },
        })}
        host={host}
      />,
    )
    fireEvent.pointerUp(screen.getByRole("slider", { name: "Sharpness" }))
    expect(host.calls).toEqual([
      'gameplay-control:sharpness:{"kind":"range","value":55}',
    ])
  })

  test("unmount cancels a pending range commit", async () => {
    const host = createFixtureHost()
    const rendered = render(<ShiftSurface model={overlayModel()} host={host} />)
    fireEvent.change(screen.getByRole("slider", { name: "Sharpness" }), {
      target: { value: "55" },
    })
    rendered.unmount()
    await act(async () => {
      await new Promise(resolve => setTimeout(resolve, 2100))
    })
    expect(host.calls).toEqual([])
  })

  test("describes every form and keeps reasoned unavailable values focusable but inert", () => {
    const unavailablePresentation: SurfaceGameplayOverlayPresentation = {
      ...presentation,
      groups: presentation.groups.map(group => ({
        ...group,
        controls: group.controls.map(control =>
          control.id === "fill"
            ? { ...control, enabled: false, disabledReason: undefined }
            : control.id === "mouse-mode"
              ? { ...control, enabled: false, disabledReason: "Mouse input is disconnected." }
              : control.id === "sharpness"
                ? { ...control, enabled: false, disabledReason: "Sharpness is locked." }
                : control),
      })),
    }
    const host = createFixtureHost()
    const rendered = render(
      <ShiftSurface
        model={overlayModel({ presentation: unavailablePresentation })}
        host={host}
      />,
    )

    const command = screen.getByRole("button", { name: "Keyboard" })
    expect(command.getAttribute("aria-describedby"))
      .toBe("gameplay-control-keyboard-description")
    expect(screen.getByText("Show the streaming keyboard.").getAttribute("id"))
      .toBe("gameplay-control-keyboard-description")

    const toggle = screen.getByRole("switch", { name: "Fill screen" })
    expect(toggle.getAttribute("aria-describedby"))
      .toBe("gameplay-control-fill-description")
    expect(toggle.hasAttribute("disabled")).toBe(true)

    const choice = screen.getByRole("combobox", { name: "Mouse mode" })
    expect(choice.tagName).toBe("DIV")
    expect(choice.getAttribute("tabindex")).toBe("0")
    expect(choice.getAttribute("aria-describedby")).toBe(
      "gameplay-control-mouse-mode-description gameplay-control-mouse-mode-reason",
    )
    expect(choice.getAttribute("data-unavailable")).toBe("true")

    const range = screen.getByRole("slider", { name: "Sharpness" })
    expect(range.tagName).toBe("DIV")
    expect(range.getAttribute("tabindex")).toBe("0")
    expect(range.getAttribute("aria-valuenow")).toBe("50")
    expect(range.getAttribute("aria-describedby")).toBe(
      "gameplay-control-sharpness-description gameplay-control-sharpness-reason",
    )
    expect(range.getAttribute("data-unavailable")).toBe("true")

    act(() => {
      choice.dispatchEvent(
        new CustomEvent("korri-semantic-direction", {
          detail: { direction: "right", repeat: false },
        }),
      )
      range.dispatchEvent(
        new CustomEvent("korri-semantic-direction", {
          detail: { direction: "right", repeat: false },
        }),
      )
    })
    fireEvent.click(choice)
    fireEvent.click(range)

    const unavailableCommand = screen.getByRole("button", {
      name: "Open RetroArch menu",
    })
    expect(unavailableCommand.getAttribute("aria-describedby")).toBe(
      "gameplay-control-retroarch-menu-description gameplay-control-retroarch-menu-reason",
    )
    expect(host.calls).toEqual([])

    const noReasonPresentation: SurfaceGameplayOverlayPresentation = {
      ...presentation,
      groups: presentation.groups.map(group => ({
        ...group,
        controls: group.controls.map(control =>
          control.id === "mouse-mode" || control.id === "sharpness"
            ? { ...control, enabled: false, disabledReason: undefined }
            : control),
      })),
    }
    rendered.rerender(
      <ShiftSurface
        model={overlayModel({ presentation: noReasonPresentation })}
        host={host}
      />,
    )

    const nativeChoice = screen.getByRole("combobox", { name: "Mouse mode" })
    const nativeRange = screen.getByRole("slider", { name: "Sharpness" })
    expect(nativeChoice.tagName).toBe("SELECT")
    expect(nativeChoice.hasAttribute("disabled")).toBe(true)
    expect(nativeRange.tagName).toBe("INPUT")
    expect(nativeRange.hasAttribute("disabled")).toBe(true)
  })

  test("keeps each native gameplay touch target at the 48px accessibility floor", () => {
    const css = readFileSync("src/shift.css", "utf8")
    expect(css).toMatch(
      /\.shift-sheet-action \{[^}]*min-block-size: max\(48px/s,
    )
    expect(css).toMatch(
      /\.shift-sheet-control \{[^}]*min-block-size: max\(48px/s,
    )
    expect(css).toMatch(
      /\.shift-sheet-choice-input,\n\.shift-sheet-range-input \{[^}]*min-block-size: max\(48px/s,
    )
  })

  test("Resume, Back, Guide, Start, and scrim dismiss locally", () => {
    for (const dismiss of ["resume", "back", "system", "menu", "scrim"] as const) {
      const host = createFixtureHost()
      const rendered = render(<ShiftSurface model={overlayModel()} host={host} />)
      if (dismiss === "resume") {
        fireEvent.click(screen.getByRole("button", { name: "Resume" }))
      } else if (dismiss === "scrim") {
        fireEvent.pointerDown(rendered.container.querySelector(".shift-sheet-scrim")!)
      } else {
        act(() => host.press(dismiss))
      }
      expect(host.calls).toEqual(["gameplay-overlay-dismiss"])
      rendered.unmount()
    }
  })

  test("keeps Resume available while presenting a calm gameplay-control failure", () => {
    const host = createFixtureHost()
    render(
      <ShiftSurface
        model={overlayModel({
          status: {
            _tag: "Problem",
            kicker: "Controls unavailable",
            reason: "Gameplay controls could not be refreshed. Resume is still available.",
            canRetry: true,
          },
        })}
        host={host}
      />,
    )

    expect(screen.getByText("Gameplay controls could not be refreshed. Resume is still available."))
      .toBeDefined()
    fireEvent.click(screen.getByRole("button", { name: "Resume" }))
    expect(host.calls).toEqual(["gameplay-overlay-dismiss"])
  })
})

describe("Shift library", () => {
  const openLibrary = () => {
    const cap = screen.getByRole("button", { name: "Library" })
    fireEvent.focus(cap)
    fireEvent.click(cap)
  }

  test("uses the original dedicated Library hero and tile", () => {
    render(<ShiftSurface model={model()} host={createFixtureHost()} />)

    fireEvent.focus(screen.getByRole("button", { name: "Library" }))

    expect(screen.getByText("Your collection")).toBeDefined()
    expect(screen.getByText("Browse every game")).toBeDefined()
  })

  test("restores the original browse-everything destination", () => {
    const { container } = render(
      <ShiftSurface model={model()} host={createFixtureHost()} />,
    )

    openLibrary()

    expect(container.querySelector("[data-shift-library]")).toBeDefined()
    expect(screen.getByRole("heading", { name: "Library" })).toBeDefined()
    expect(screen.getByRole("tab", { name: "All" })).toBeDefined()
    expect(screen.getByRole("tab", { name: "Favorites" })).toBeDefined()
    expect(screen.getByRole("tab", { name: "By Genre" })).toBeDefined()
    expect(
      screen.getByRole("button", { name: "Sorted by Recent. Press to change." }),
    ).toBeDefined()
    expect(
      screen
        .getAllByRole("button")
        .map(button => button.getAttribute("aria-label"))
        .filter(Boolean),
    ).toEqual([
      "Sorted by Recent. Press to change.",
      "Neverball",
      "Skate 3",
      "Wario Land 4",
    ])
  })

  test("opens the original detail screen before launching", () => {
    const host = createFixtureHost()
    const { container } = render(<ShiftSurface model={model()} host={host} />)
    openLibrary()

    fireEvent.click(screen.getByRole("button", { name: "Wario Land 4" }))

    expect(container.querySelector("[data-shift-library]")).toBeNull()
    const detail = container.querySelector("[data-shift-detail]")
    expect(detail).toBeDefined()
    expect(detail?.getAttribute("data-shift-detail-game-id")).toBe("local-game:wl4")
    expect(host.calls).toEqual([])

    fireEvent.click(screen.getByRole("button", { name: "▶ Play" }))
    expect(host.calls).toEqual(["launch:local-game:wl4"])
    expect(container.querySelector("[data-shift-detail]")).toBeNull()
  })

  test("a combined game asks for a host with local first", () => {
    const host = createFixtureHost()
    const games =
      fixtureModel.catalog._tag === "Ready"
        ? fixtureModel.catalog.games.map(game =>
            game.id === "local-game:wl4"
              ? {
                  ...game,
                  launchLocations: [
                    { id: "local-wl4", label: "This device" },
                    { id: "zao-wl4", label: "zao" },
                  ],
                }
              : game,
          )
        : []
    const { container } = render(
      <ShiftSurface
        model={model({ catalog: { _tag: "Ready", games } })}
        host={host}
      />,
    )
    openLibrary()
    fireEvent.click(screen.getByRole("button", { name: "Wario Land 4" }))
    fireEvent.click(screen.getByRole("button", { name: "▶ Play" }))

    const dialog = screen.getByRole("dialog", {
      name: "Choose where to play Wario Land 4",
    })
    const choices = within(dialog)
      .getAllByRole("button")
      .filter(button =>
        ["This device", "zao"].includes(button.textContent ?? ""),
      )
    expect(choices.map(button => button.textContent)).toEqual([
      "This device",
      "zao",
    ])
    expect(choices.map(button => button.getAttribute("data-launch-location-id")))
      .toEqual(["local-wl4", "zao-wl4"])
    expect(document.activeElement).toBe(choices[0]!)
    expect(host.calls).toEqual([])

    act(() => host.press("back"))
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(container.querySelector("[data-shift-detail]")).toBeDefined()
    expect(host.calls).toEqual([])

    fireEvent.click(screen.getByRole("button", { name: "▶ Play" }))
    const reopened = screen.getByRole("dialog", {
      name: "Choose where to play Wario Land 4",
    })
    fireEvent.click(within(reopened).getByRole("button", { name: "zao" }))
    expect(host.calls).toEqual(["launch:local-game:wl4:zao-wl4"])
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  test("a catalog refresh cannot leave a hidden chooser trapping Back", () => {
    const host = createFixtureHost()
    const games =
      fixtureModel.catalog._tag === "Ready"
        ? fixtureModel.catalog.games.map(game =>
            game.id === "local-game:wl4"
              ? {
                  ...game,
                  launchLocations: [
                    { id: "local-wl4", label: "This device" },
                    { id: "zao-wl4", label: "zao" },
                  ],
                }
              : game,
          )
        : []
    const rendered = render(
      <ShiftSurface
        model={model({ catalog: { _tag: "Ready", games } })}
        host={host}
      />,
    )
    openLibrary()
    fireEvent.click(screen.getByRole("button", { name: "Wario Land 4" }))
    fireEvent.click(screen.getByRole("button", { name: "▶ Play" }))
    expect(screen.getByRole("dialog")).toBeDefined()

    rendered.rerender(<ShiftSurface model={model()} host={host} />)
    expect(screen.queryByRole("dialog")).toBeNull()
    act(() => host.press("back"))

    expect(rendered.container.querySelector("[data-shift-detail]")).toBeNull()
    expect(screen.getByRole("button", { name: "Library" })).toBeDefined()
    expect(host.calls).toEqual([])
  })

  test("Back returns to Home without calling the host", () => {
    const host = createFixtureHost()
    const { container } = render(<ShiftSurface model={model()} host={host} />)
    openLibrary()

    act(() => host.press("back"))

    expect(container.querySelector("[data-shift-library]")).toBeNull()
    expect(screen.getByRole("button", { name: "Library" })).toBeDefined()
    expect(host.calls).toEqual([])
  })
})

describe("Shift settings", () => {
  const openSettings = () => {
    const cap = screen.getByRole("button", { name: "Settings" })
    fireEvent.focus(cap)
    fireEvent.click(cap)
  }

  test("the rail offers Settings once Korri has facts to state", () => {
    render(<ShiftSurface model={model()} host={createFixtureHost()} />)

    expect(screen.getByRole("button", { name: "Settings" })).toBeDefined()
  })

  test("Settings stays reachable when plugin choices leave no games", () => {
    render(
      <ShiftSurface
        model={model({ catalog: { _tag: "Empty" } })}
        host={createFixtureHost()}
      />,
    )

    expect(screen.getByRole("button", { name: "Settings" })).toBeDefined()
  })

  test("no Settings destination when Korri can state nothing", () => {
    // A surface must not advertise a screen that would open empty.
    render(
      <ShiftSurface model={model({ settings: [] })} host={createFixtureHost()} />,
    )

    expect(screen.queryByRole("button", { name: "Settings" })).toBeNull()
  })

  test("opening settings is Shift's own business, not a host action", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={model()} host={host} />)

    openSettings()

    // Korri never hears about it: which screens exist is the surface's call.
    expect(host.calls).toEqual([])
    expect(screen.getByText("This device")).toBeDefined()
  })

  test("settings shows each group's facts as label and value", () => {
    render(<ShiftSurface model={model()} host={createFixtureHost()} />)
    openSettings()

    expect(screen.getByText("Software")).toBeDefined()
    expect(screen.getByText("korrid 0.4.1")).toBeDefined()
    expect(screen.getByText("File access")).toBeDefined()
    expect(screen.getByText("Granted")).toBeDefined()
  })

  test("only settings backed by a real interaction become buttons", () => {
    const { container } = render(
      <ShiftSurface model={model()} host={createFixtureHost()} />,
    )
    openSettings()

    expect(container.querySelectorAll("button.shift-setting-row").length).toBe(3)
    expect(container.querySelectorAll("div.shift-setting-row").length).toBe(2)
  })

  test("an actionable row advertises Select and Back", () => {
    const { container } = render(
      <ShiftSurface model={model()} host={createFixtureHost()} />,
    )
    openSettings()

    const hints = Array.from(
      container.querySelectorAll(".shift-cine-hint"),
    ).map(hint => hint.textContent)
    expect(hints).toEqual(["ASelect", "BBack"])
  })

  test("text editing saves through the side sheet", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={model()} host={host} />)
    openSettings()

    fireEvent.click(screen.getByRole("button", { name: "Name: usu" }))
    const input = screen.getByRole("textbox", { name: "Name" })
    expect(document.activeElement).toBe(input)
    fireEvent.change(input, { target: { value: "pocket" } })
    fireEvent.click(screen.getByRole("button", { name: "Save" }))

    expect(host.calls).toEqual(["setting:device-name:pocket"])
  })

  test("saving a setting disables duplicate submission until the model changes", () => {
    const host = createFixtureHost()
    const rendered = render(<ShiftSurface model={model()} host={host} />)
    openSettings()

    fireEvent.click(screen.getByRole("button", { name: "Name: usu" }))
    const input = screen.getByRole<HTMLInputElement>("textbox", { name: "Name" })
    fireEvent.change(input, { target: { value: "pocket" } })
    fireEvent.click(screen.getByRole("button", { name: "Save" }))
    expect(host.calls).toEqual(["setting:device-name:pocket"])

    rendered.rerender(
      <ShiftSurface
        model={model({
          settingsStatus: { _tag: "Saving", settingId: "device-name" },
        })}
        host={host}
      />,
    )
    const savingInput = screen.getByRole<HTMLInputElement>("textbox", {
      name: "Name",
    })
    const save = screen.getByRole<HTMLButtonElement>("button", {
      name: "Saving…",
    })

    expect(savingInput.disabled).toBe(true)
    expect(save.disabled).toBe(true)
    fireEvent.click(save)
    save.click()
    expect(host.calls).toEqual(["setting:device-name:pocket"])

    const savedSettings = fixtureModel.settings.map(group =>
      group.title === "Device"
        ? {
            ...group,
            items: group.items.map(item =>
              item.id === "device-name" ? { ...item, value: "pocket" } : item,
            ),
          }
        : group,
    )
    rendered.rerender(
      <ShiftSurface model={model({ settings: savedSettings })} host={host} />,
    )
    expect(screen.queryByRole("dialog", { name: "Change Name" })).toBeNull()
  })

  test("an editor-level settings problem is dismissible without changing the value", () => {
    const host = createFixtureHost()
    const rendered = render(<ShiftSurface model={model()} host={host} />)
    openSettings()
    fireEvent.click(screen.getByRole("button", { name: "Name: usu" }))

    rendered.rerender(
      <ShiftSurface
        model={model({
          settingsStatus: {
            _tag: "Problem",
            settingId: "device-name",
            message: "Name already changed elsewhere",
          },
        })}
        host={host}
      />,
    )

    expect(screen.getByText("Name already changed elsewhere")).toBeDefined()
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }))

    expect(host.calls).toEqual(["settings-dismiss"])
  })

  test("a background refresh does not erase an unfinished text edit", () => {
    const host = createFixtureHost()
    const rendered = render(<ShiftSurface model={model()} host={host} />)
    openSettings()
    fireEvent.click(screen.getByRole("button", { name: "Name: usu" }))
    const input = screen.getByRole("textbox", { name: "Name" })
    fireEvent.change(input, { target: { value: "unfinished" } })

    rendered.rerender(<ShiftSurface model={model()} host={host} />)

    expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Name" }).value)
      .toBe("unfinished")
  })

  test("a choice opens the side sheet and publishes the chosen value", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={model()} host={host} />)
    openSettings()

    fireEvent.click(screen.getByRole("button", { name: "mGBA: On" }))
    fireEvent.click(screen.getByRole("button", { name: "Off" }))

    expect(host.calls).toEqual(["setting:@korri:mgba:false"])
  })

  test("an Android-owned row runs its native action", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={model()} host={host} />)
    openSettings()

    fireEvent.click(screen.getByRole("button", { name: "File access: Granted" }))

    expect(host.calls).toEqual(["action:storage-access"])
  })

  test("a page-level settings problem is visible and dismissible", () => {
    const host = createFixtureHost()
    render(
      <ShiftSurface
        model={model({
          settingsStatus: {
            _tag: "Problem",
            settingId: "storage-access",
            message: "Android settings could not open",
          },
        })}
        host={host}
      />,
    )
    openSettings()

    expect(screen.queryByRole("dialog")).toBeNull()
    fireEvent.click(
      screen.getByRole("button", { name: "Android settings could not open" }),
    )

    expect(host.calls).toEqual(["settings-dismiss"])
  })

  test("a saving setting row is busy instead of selectable", () => {
    const host = createFixtureHost()
    const { container } = render(
      <ShiftSurface
        model={model({
          settingsStatus: { _tag: "Saving", settingId: "file-access" },
        })}
        host={host}
      />,
    )
    openSettings()

    expect(screen.queryByRole("button", { name: "File access: Granted" })).toBeNull()
    expect(container.querySelector(".shift-setting-row[data-saving='true']"))
      .toBeDefined()
  })

  test("back returns to the games without touching the host", () => {
    const host = createFixtureHost()
    render(<ShiftSurface model={model()} host={host} />)
    openSettings()

    act(() => host.press("back"))

    expect(screen.getByRole("button", { name: "Library" })).toBeDefined()
    expect(host.calls).toEqual([])
  })

  test("publishes every currently rendered design part as a preview", () => {
    expect(shiftPartPreviews).toHaveLength(49)
    expect(shiftPartPreviews.map(part => part.id)).toContain("shift.clock")
    expect(shiftPartPreviews.map(part => part.id)).toContain(
      "shift.library-tile",
    )
    expect(new Set(shiftPartPreviews.map(part => part.id)).size).toBe(
      shiftPartPreviews.length,
    )
  })
})
