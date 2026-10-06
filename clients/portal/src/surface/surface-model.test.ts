import { describe, expect, test } from "bun:test"
import { PendingLaunchPhase } from "@contracts/generated/korrid"
import { LaunchablesState, launchSubjectForGame, type LaunchSubject, type PortalEntry } from "../launchables/state"
import {
  entryForId,
  entryForLaunchLocation,
  gameActionsForEntry,
  surfaceModelFrom,
} from "./surface-model"

const ready = (
  entries: readonly PortalEntry[],
  notice: string | null = null,
  subject?: LaunchSubject,
): LaunchablesState => ({
  _tag: "Ready",
  entries,
  notice:
    notice === null
      ? null
      : { _tag: "Launch", message: notice, ...(subject ? { subject } : {}) },
})

const source = (label: string) => ({ label, isLocal: false }) as const

const localGame: PortalEntry = {
  kind: "local-game",
  game: { id: "wl4", title: "Wario Land 4", system: "GBA" },
}
const hostGame: PortalEntry = {
  kind: "game",
  game: {
    id: "neverball",
    title: "Neverball",
    host: "zao",
    supportsRunnerSelection: false,
    source: source("zao"),
  },
}
const nowPlaying: PortalEntry = {
  kind: "now-playing",
  session: { launchId: "L1", title: "Skate 3", host: "aka" },
}

describe("surfaceModelFrom", () => {
  test("catalog failures keep their cause and are not labelled as attempted launches", () => {
    const state = LaunchablesState.fromSources({
      _tag: "Ok",
      payload: {
        games: [hostGame.game],
        failures: [{ host: "rpminiv2", code: "LocalRomMissing", message: "The ROM file is missing" }],
      },
    })
    const model = surfaceModelFrom(state)
    expect(model.status).toEqual({
      _tag: "Problem",
      kicker: "Catalog problem",
      reason: "rpminiv2: LocalRomMissing: The ROM file is missing",
      canRetry: false,
    })
    if (model.catalog._tag !== "Ready") throw new Error("expected Ready")
    expect(model.catalog.games[0]?.title).toBe(hostGame.game.title)
  })

  test("stop failures are not labelled as attempted launches", () => {
    const state = LaunchablesState.stopTimedOut({
      _tag: "Stopping", launchId: "L1", entries: [nowPlaying], notice: null,
    })
    const model = surfaceModelFrom(state)
    expect(model.status).toMatchObject({ _tag: "Problem", kicker: "Couldn't end the session" })
  })
  test("labels a source-local catalog route as This device without converting its identity", () => {
    const localCatalog: PortalEntry = {
      kind: "game",
      game: { ...hostGame.game, source: { label: "device-label", isLocal: true } },
      alternatives: [{ kind: "remote", game: { ...hostGame.game, host: "aka", source: source("aka") } }],
    }
    const model = surfaceModelFrom(ready([localCatalog]))
    if (model.catalog._tag !== "Ready") throw new Error("expected Ready")
    const rendered = model.catalog.games[0]!
    expect(rendered.section).toBe("This device")
    expect(rendered.subtitle).toBe("This device · Also on aka")
    expect(rendered.launchLocations?.map(location => location.label)).toEqual(["This device", "aka"])
    expect(entryForLaunchLocation(localCatalog, rendered.launchLocations![0]!.id)).toEqual({ kind: "game", game: localCatalog.game })
  })

  test("publishes only playable things as games", () => {
    const model = surfaceModelFrom(ready([localGame, hostGame]))

    expect(model.catalog._tag).toBe("Ready")
    if (model.catalog._tag !== "Ready") return
    expect(model.catalog.games.map(game => game.title)).toEqual([
      "Wario Land 4",
      "Neverball",
    ])
  })

  test("groups games by where they can be played", () => {
    const model = surfaceModelFrom(ready([nowPlaying, localGame, hostGame]))

    if (model.catalog._tag !== "Ready") throw new Error("expected Ready")
    expect(model.catalog.games.map(game => game.section)).toEqual([
      "Continue",
      "This device",
      "zao",
    ])
  })

  test("never invents art or metadata korrid does not have", () => {
    const model = surfaceModelFrom(ready([localGame]))

    if (model.catalog._tag !== "Ready") throw new Error("expected Ready")
    const game = model.catalog.games[0]!
    expect(game.coverArtUrl).toBeUndefined()
    expect(game.wideArtUrl).toBeUndefined()
    expect(game.subtitle).toBe("GBA")
    expect(game.lastPlayedAt).toBeUndefined()
    expect(game.playCount).toBeUndefined()
    expect(game.totalPlaytimeSeconds).toBeUndefined()
  })

  test("publishes play facts as UTC millis and merges folded copies", () => {
    const model = surfaceModelFrom(
      ready([
        {
          ...localGame,
          game: {
            ...localGame.game,
            playStats: {
              lastPlayed: "2026-08-15T10:00:00Z",
              playCount: 2,
              totalPlaytimeSeconds: 600,
            },
          },
          alternatives: [
            {
              kind: "remote",
              game: {
                id: "wl4",
                title: "Wario Land 4",
                host: "zao",
                supportsRunnerSelection: false,
                source: {
                  devicePublicKey: "zao-key",
                  label: "zao",
                  isLocal: false,
                },
                playStats: {
                  lastPlayed: "2026-09-04T18:00:00Z",
                  playCount: 1,
                  totalPlaytimeSeconds: 1200,
                },
              },
            },
          ],
        },
        hostGame,
      ]),
    )

    if (model.catalog._tag !== "Ready") throw new Error("expected Ready")
    const [wario, neverball] = model.catalog.games
    expect(wario?.lastPlayedAt).toBe(Date.UTC(2026, 8, 4, 18, 0, 0))
    expect(wario?.playCount).toBe(3)
    expect(wario?.totalPlaytimeSeconds).toBe(1800)
    expect(neverball?.lastPlayedAt).toBeUndefined()
  })

  test("publishes trusted local cover art resolved by the host", () => {
    const model = surfaceModelFrom(
      ready([
        {
          kind: "local-game",
          game: {
            id: "wl4",
            title: "Wario Land 4",
            system: "GBA",
            coverArtUrl:
              "https://korri.invalid/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
          },
        },
      ]),
    )

    if (model.catalog._tag !== "Ready") throw new Error("expected Ready")
    expect(model.catalog.games[0]?.coverArtUrl).toBe(
      "https://korri.invalid/game-assets/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png",
    )
  })

  test("reports retained host copies without adding another game", () => {
    const model = surfaceModelFrom(
      ready([
        {
          ...localGame,
          alternatives: [
            {
              kind: "remote",
              game: { id: "wl4", title: "Wario Land 4", host: "zao", supportsRunnerSelection: false, source: source("zao") },
            },
            {
              kind: "remote",
              game: { id: "wl4-aka", title: "Wario Land 4", host: "aka", supportsRunnerSelection: false, source: source("aka") },
            },
          ],
        },
      ]),
    )

    if (model.catalog._tag !== "Ready") throw new Error("expected Ready")
    expect(model.catalog.games).toHaveLength(1)
    expect(model.catalog.games[0]?.subtitle).toBe("GBA · Also on aka, zao")
    expect(model.catalog.games[0]?.launchLocations).toEqual([
      { id: '["local",null,"wl4"]', label: "This device" },
      { id: '["remote","aka","wl4-aka"]', label: "aka" },
      { id: '["remote","zao","wl4"]', label: "zao" },
    ])
  })

  test("a host choice resolves to exactly that copy", () => {
    const folded: PortalEntry = {
      ...localGame,
      alternatives: [
        {
          kind: "remote",
          game: { id: "wl4", title: "Wario Land 4", host: "zao", supportsRunnerSelection: false, source: source("zao") },
        },
      ],
    }
    const model = surfaceModelFrom(ready([folded]))
    if (model.catalog._tag !== "Ready") throw new Error("expected Ready")
    const zao = model.catalog.games[0]?.launchLocations?.find(
      location => location.label === "zao",
    )
    if (zao === undefined) throw new Error("expected zao choice")

    expect(entryForLaunchLocation(folded, zao.id)).toEqual({
      kind: "game",
      game: { id: "wl4", title: "Wario Land 4", host: "zao", supportsRunnerSelection: false, source: source("zao") },
    })
  })

  test("the running session is resumable and leads the catalog", () => {
    const model = surfaceModelFrom(ready([nowPlaying, localGame]))

    if (model.catalog._tag !== "Ready") throw new Error("expected Ready")
    expect(model.catalog.games[0]).toMatchObject({
      title: "Skate 3",
      resumable: true,
    })
  })

  test("a notice becomes a problem the user acknowledges", () => {
    const model = surfaceModelFrom(ready([localGame], "local ROM is missing"))

    expect(model.status).toEqual({
      _tag: "Problem",
      kicker: "Couldn't start",
      reason: "local ROM is missing",
      canRetry: false,
    })
  })

  test("a failure names the game it belongs to, not the focused one", () => {
    const model = surfaceModelFrom(
      ready([localGame], "ActiveSessionConflict: a session must end", {
        id: "wl4",
        title: "Wario Land 4",
      }),
    )

    expect(model.status).toEqual({
      _tag: "Problem",
      kicker: "Couldn't start Wario Land 4",
      reason: "ActiveSessionConflict: a session must end",
      canRetry: false,
      gameId: "local-game:wl4",
      gameTitle: "Wario Land 4",
    })
  })

  test("in-flight work is busy, never an error", () => {
    const preparing = LaunchablesState.beginPreparing(
      ready([hostGame]),
      "Neverball",
    )
    const model = surfaceModelFrom(preparing)

    expect(model.status._tag).toBe("Busy")
    if (model.status._tag !== "Busy") return
    expect(model.status.kicker).toContain("Neverball")
  })

  test("loading reports loading rather than an empty library", () => {
    const model = surfaceModelFrom(LaunchablesState.loading())

    expect(model.catalog._tag).toBe("Loading")
    expect(model.actions).toEqual([])
    expect(model.status._tag).toBe("Browsing")
  })

  test("the clock is only published when the host has one", () => {
    expect(surfaceModelFrom(ready([localGame])).clockLabel).toBeUndefined()
    expect(
      surfaceModelFrom(ready([localGame]), { clockLabel: "4:24 PM" })
        .clockLabel,
    ).toBe("4:24 PM")
  })
})

describe("game actions", () => {
  test("only the running session has actions today", () => {
    expect(gameActionsForEntry(localGame)).toEqual([])
    expect(gameActionsForEntry(hostGame)).toEqual([])
    expect(gameActionsForEntry(nowPlaying).map(action => action.id)).toEqual([
      "resume",
      "stop",
    ])
  })

  test("stopping is marked destructive", () => {
    const stop = gameActionsForEntry(nowPlaying).find(
      action => action.id === "stop",
    )
    expect(stop?.destructive).toBe(true)
  })
})

describe("entryForId", () => {
  test("surface ids round-trip back to the entry they came from", () => {
    const state = ready([localGame, hostGame, nowPlaying])
    const model = surfaceModelFrom(state)

    if (model.catalog._tag !== "Ready") throw new Error("expected Ready")
    for (const game of model.catalog.games) {
      expect(entryForId(state, game.id)).toBeDefined()
    }
  })

  test("an unknown id resolves to nothing rather than the wrong entry", () => {
    expect(entryForId(ready([localGame]), "local-game:missing")).toBeUndefined()
    expect(entryForId(LaunchablesState.loading(), "anything")).toBeUndefined()
  })
})

describe("status entry identity", () => {
  const peer: Extract<PortalEntry, { kind: "game" }> = {
    kind: "game", game: { id: "same", title: "Peer title", host: "peer", supportsRunnerSelection: false, source: { label: "Peer", isLocal: false } },
  }
  const local: Extract<PortalEntry, { kind: "game" }> = {
    kind: "game", game: { ...peer.game, title: "Local title", host: "device-label", source: { label: "This device", isLocal: true } },
  }
  test("Preparing, Launching and Problem retain the selected peer's actual entry key", () => {
    const state = ready([local, peer])
    const subject = launchSubjectForGame(peer.game, state._tag === "Loading" ? [] : state.entries)
    const preparing = LaunchablesState.beginPreparing(state, peer.game.title, subject)
    const launching = LaunchablesState.beginLaunching(state, peer.game.title, subject)
    const problem = LaunchablesState.withPrepareOutcome(preparing, { _tag: "Err", payload: { code: "UpstreamFailure", message: "Peer refused start" } })
    for (const result of [preparing, launching, problem]) {
      expect(surfaceModelFrom(result).status).toHaveProperty("gameId", "game:peer:same")
    }
  })
  test("a removed exact local subject is not renamed to the remaining same-id peer", () => {
    const subject = launchSubjectForGame(local.game, [peer, local])
    const model = surfaceModelFrom(ready([peer], "The local game was removed", subject))
    expect(model.status).not.toHaveProperty("gameId")
    expect(model.status).toMatchObject({ _tag: "Problem", gameTitle: "Local title" })
  })
  test("raw domain identity without exact copy provenance cannot choose between presented peers", () => {
    const state = ready([peer, local])
    const preparing = LaunchablesState.beginPreparing(state, "Known domain title", { id: "same", title: "Known domain title" })
    expect(surfaceModelFrom(preparing).status).not.toHaveProperty("gameId")
  })
  test("a local route folded under a local inventory entry uses the existing primary presentation id", () => {
    const folded: PortalEntry = { ...localGame, alternatives: [{ kind: "remote", game: local.game }] }
    const state = ready([peer, folded])
    const starting = LaunchablesState.beginStartup(state, local.game)
    expect(surfaceModelFrom(starting).status).toHaveProperty("gameId", "local-game:wl4")
  })
})

test("manual choices publish every exact phase, stable same-title labels, retry and no game attribution", () => {
  const phases = [PendingLaunchPhase.Reserved, PendingLaunchPhase.Preparing, PendingLaunchPhase.Committing, "waiting", PendingLaunchPhase.Cancelling] as const
  const state: LaunchablesState = { _tag: "Choosing", entries: [hostGame], notice: null, choices: [
    ...phases.map((phase, index) => ({ launchId: `${index}`, gameId: "neverball", title: index < 2 ? "Same title" : `Title ${index}`, phase, cancel: "idle" as const })),
    { launchId: "5", title: "Retry title", phase: PendingLaunchPhase.Reserved, cancel: "retry" },
  ] }
  const status = surfaceModelFrom(state).status
  expect(status).toEqual({ _tag: "Busy", kicker: "6 launches are starting", detail: "Cancel each launch you do not want.", actions: [
    { id: "cancel-pending:0", label: "Cancel Same title (1)", description: "Waiting to start", enabled: true },
    { id: "cancel-pending:1", label: "Cancel Same title (2)", description: "Preparing", enabled: true },
    { id: "cancel-pending:2", label: "Cancel Title 2", description: "Starting", enabled: true },
    { id: "cancel-pending:3", label: "Cancel Title 3", description: "Waiting for its window", enabled: true },
    { id: "cancel-pending:4", label: "Cancel Title 4", description: "Cancelling", enabled: false },
    { id: "cancel-pending:5", label: "Cancel Retry title", description: "Cancel failed. Try again.", enabled: true },
  ] })
  expect(status).not.toHaveProperty("gameId")
  const sent = surfaceModelFrom(LaunchablesState.beginPendingCancellation(state, "0")).status
  if (sent._tag !== "Busy" || status._tag !== "Busy") throw new Error("expected Busy")
  expect(sent.actions?.map(action => [action.id, action.label])).toEqual(status.actions?.map(action => [action.id, action.label]))
  expect(sent.actions?.[0]?.enabled).toBe(false)
  expect(surfaceModelFrom({ ...state, notice: { _tag: "Launch", message: "Exact cancellation failed" } }).status).toMatchObject({ _tag: "Busy", detail: "Exact cancellation failed" })
})
