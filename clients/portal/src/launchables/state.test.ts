import { describe, expect, it } from "bun:test"
import type {
  CatalogSnapshotOutcome,
  LocalGamesListOutcome,
  SessionStatusOutcome,
  SessionStopOutcome,
} from "@contracts/generated/korrid"
import {
  MoonlightImplementation,
  SessionStopPhase,
} from "@contracts/generated/korrid"
import type { BackgroundNoticeResult } from "@contracts/bridge/korri-native-bridge"
import { entryKey, entryLabel, LaunchablesState } from "./state"
import type { LaunchablesState as State, PortalEntry } from "./state"

/** The banner entry a stop is about. Surfaces name it; this ADT never guesses. */
const nowPlayingEntry = (state: State): PortalEntry => {
  if (state._tag === "Loading") throw new Error("unreachable")
  const entry = state.entries.find(candidate => candidate.kind === "now-playing")
  if (!entry) throw new Error("expected a now-playing entry")
  return entry
}

const source = (label: string) => ({ label, isLocal: false }) as const

const officeHost = { uuid: "h1", name: "Office PC" } as const

const officeApps = {
  host: officeHost,
  apps: {
    _tag: "StreamApps",
    items: [
      { id: 1, name: "Desktop" },
      { id: 2, name: "Steam" },
    ],
  },
} as const

const gamesOk: CatalogSnapshotOutcome = {
  _tag: "Ok",
  payload: {
    games: [
      { id: "skate3", title: "Skate 3", source: source("aka") },
      { id: "neverball", title: "Neverball", source: source("zao") },
    ],
  },
}

const gamesErr: CatalogSnapshotOutcome = {
  _tag: "Err",
  payload: { code: "UpstreamUnreachable", message: "host offline" },
}

const localGamesOk: LocalGamesListOutcome = {
  _tag: "Ok",
  payload: {
    games: [{ id: "wl4", title: "Wario Land 4", system: "Game Boy Advance" }],
  },
}

const ready = LaunchablesState.fromSources([officeApps], gamesOk)

describe("LaunchablesState.fromSources", () => {
  it("folds the local game beside Korri catalog entries", () => {
    const state = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      undefined,
      localGamesOk,
    )
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.map(entry => entry.kind)).toEqual([
      "local-game",
      "game",
      "game",
      "background-notice",
    ])
    expect(state.entries[0]).toMatchObject({
      kind: "local-game",
      game: { id: "wl4", title: "Wario Land 4" },
    })
  })

  it("shows matching local and Zao copies as one local-first game", () => {
    const identity = { kind: "hash" as const, value: "sha256:wario" }
    const state = LaunchablesState.fromSources(
      [officeApps],
      {
        _tag: "Ok",
        payload: {
          games: [
            {
              id: "wl4",
              title: "Wario Land 4",
              host: "zao",
              identity,
              source: source("zao"),
            },
          ],
        },
      },
      undefined,
      undefined,
      {
        _tag: "Ok",
        payload: {
          games: [
            {
              id: "wl4",
              title: "Wario Land 4",
              system: "Game Boy Advance",
              identity,
            },
          ],
        },
      },
    )

    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.filter(entry => entry.kind.includes("game"))).toEqual([
      {
        kind: "local-game",
        game: {
          id: "wl4",
          title: "Wario Land 4",
          system: "Game Boy Advance",
          identity,
        },
        alternatives: [
          {
            kind: "remote",
            game: {
              id: "wl4",
              title: "Wario Land 4",
              host: "zao",
              identity,
              source: source("zao"),
            },
          },
        ],
      },
    ])
  })

  it("does not turn Sunshine's advertised apps into Korri games", () => {
    expect(ready._tag).toBe("Ready")
    if (ready._tag !== "Ready") throw new Error("unreachable")
    expect(ready.entries.map(e => e.kind)).toEqual([
      "game",
      "game",
      "background-notice",
    ])
    expect(ready.notice).toBeNull()
  })

  it("degrades a failed local-game source to a notice while entries remain", () => {
    const state = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      undefined,
      {
        _tag: "Err",
        payload: { code: "LocalStorageUnavailable", message: "storage denied" },
      },
    )
    expect(state).toMatchObject({
      _tag: "Ready",
      notice: { message: "local games: LocalStorageUnavailable" },
    })
  })

  it("surfaces local configuration failures while keeping healthy local games", () => {
    const state = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      undefined,
      {
        _tag: "Ok",
        payload: {
          games: [
            { id: "wl4", title: "Wario Land 4", system: "Game Boy Advance" },
          ],
          failures: [
            {
              code: "LocalConfigReloadFailed",
              message: "library.yaml is malformed",
            },
          ],
        },
      },
    )
    expect(state).toMatchObject({
      _tag: "Ready",
      notice: { message: "local games: LocalConfigReloadFailed" },
    })
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries[0]).toMatchObject({
      kind: "local-game",
      game: { id: "wl4" },
    })
  })

  it("degrades a failed korrid catalog to a notice while entries remain", () => {
    const state = LaunchablesState.fromSources([officeApps], gamesErr)
    expect(state).toMatchObject({
      _tag: "Ready",
      notice: { message: "games: UpstreamUnreachable" },
    })
  })

  it("surfaces partial host catalog failures while keeping healthy games", () => {
    const state = LaunchablesState.fromSources([officeApps], {
      _tag: "Ok",
      payload: {
        games: [
          {
            id: "legacy",
            title: "Legacy game",
            host: "aka",
            source: source("aka"),
          },
        ],
        failures: [
          {
            host: "zao",
            code: "UpstreamUnreachable",
            message: "connection refused",
          },
        ],
      },
    })
    expect(state).toMatchObject({
      _tag: "Ready",
      notice: { message: "zao: UpstreamUnreachable" },
    })
  })

  it("does not surface Sunshine app-query failures as catalog failures", () => {
    const state = LaunchablesState.fromSources(
      [{ host: officeHost, apps: { _tag: "QueryFailed", message: "no cache" } }],
      gamesOk,
    )
    expect(state).toMatchObject({ _tag: "Ready", notice: null })
  })

  it("does not surface Sunshine host-query failures as catalog failures", () => {
    const state = LaunchablesState.fromSources([], gamesOk, "db locked")
    expect(state).toMatchObject({ _tag: "Ready", notice: null })
  })

  it("keeps the background setting reachable when every source failed", () => {
    const state = LaunchablesState.fromSources(
      [
        {
          host: officeHost,
          apps: { _tag: "QueryFailed", message: "no cache" },
        },
      ],
      gamesErr,
    )
    // A fresh install can fail every source. The list stays usable instead of
    // collapsing into an error screen the user cannot act on.
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.map(entry => entry.kind)).toEqual([
      "background-notice",
    ])
    expect(state.notice?.message).toBe("games: UpstreamUnreachable")
  })
})

describe("hosted game identity", () => {
  it("qualifies duplicate game ids by host", () => {
    expect(
      [
        entryKey({
          kind: "game",
          game: {
            id: "shared",
            title: "Shared",
            host: "aka",
            source: source("aka"),
          },
        }),
        entryKey({
          kind: "game",
          game: {
            id: "shared",
            title: "Shared",
            host: "zao",
            source: source("zao"),
          },
        }),
      ],
    ).toEqual(["game:aka:shared", "game:zao:shared"])
  })
})

describe("LaunchablesState stream targets", () => {
  const resolvedMoonlight = {
    transportId: "@korri:moonlight/moonlight",
    implementation: MoonlightImplementation.Artemis,
    sunshineApp: "Moonlight-owned Sunshine app",
  }
  const streamSources = [
    {
      host: { uuid: "aka-uuid", name: "aka" },
      apps: {
        _tag: "StreamApps" as const,
        items: [{ id: 10, name: "Moonlight-owned Sunshine app" }],
      },
    },
    {
      host: { uuid: "zao-uuid", name: "zao" },
      apps: {
        _tag: "StreamApps" as const,
        items: [{ id: 20, name: "Moonlight-owned Sunshine app" }],
      },
    },
  ]

  it("selects the provisioned stream host named by the game", () => {
    expect(
      LaunchablesState.korriStreamTarget(
        resolvedMoonlight,
        streamSources,
        "zao",
      ),
    ).toEqual({
      _tag: "Some",
      value: { hostUuid: "zao-uuid", appId: 20 },
    })
  })

  it("preserves first-match behavior for games without a host", () => {
    expect(
      LaunchablesState.korriStreamTarget(resolvedMoonlight, streamSources),
    ).toEqual({
      _tag: "Some",
      value: { hostUuid: "aka-uuid", appId: 10 },
    })
  })

  it("does not attach to another machine when the named host is absent", () => {
    expect(
      LaunchablesState.korriStreamTarget(
        resolvedMoonlight,
        streamSources,
        "sobo",
      ),
    ).toEqual({
      _tag: "None",
    })
  })
})

describe("LaunchablesState action results", () => {
  it("surfaces local brain and native launch failures as notices", () => {
    const launching = LaunchablesState.beginLaunching(ready, "Wario Land 4")
    expect(
      LaunchablesState.withLocalLaunchOutcome(launching, {
        _tag: "Err",
        payload: { code: "LocalRomMissing", message: "ROM absent" },
      }),
    ).toMatchObject({ _tag: "Ready", notice: { message: "LocalRomMissing: ROM absent" } })
    expect(
      LaunchablesState.withLocalLaunchResult(launching, {
        _tag: "LaunchFailed",
        reason: "NotInstalled",
        message: "RetroArch absent",
      }),
    ).toMatchObject({
      _tag: "Ready",
      notice: { message: "NotInstalled: RetroArch absent" },
    })
  })

  it("surfaces stream and prepare failures as notices", () => {
    const streamFailed = LaunchablesState.withStartStreamResult(
      LaunchablesState.beginLaunching(ready, "Desktop"),
      {
        _tag: "StreamFailed",
        reason: "HostCertificateRejected",
        message: "host rejected certificate",
      },
    )
    expect(streamFailed).toMatchObject({
      notice: { message: "HostCertificateRejected: host rejected certificate" },
    })

    const prepareFailed = LaunchablesState.withPrepareOutcome(
      LaunchablesState.beginPreparing(ready, "Skate 3"),
      {
      _tag: "Err",
      payload: { code: "UpstreamFailure", message: "no such game" },
      },
    )
    expect(prepareFailed).toMatchObject({
      notice: { message: "UpstreamFailure: no such game" },
    })
  })

  it("clears notices on success and on movement", () => {
    const failed = LaunchablesState.withStartStreamResult(
      LaunchablesState.beginLaunching(ready, "Desktop"),
      {
      _tag: "StreamFailed",
      reason: "HostUnreachable",
      message: "offline",
      },
    )
    const launching = LaunchablesState.beginLaunching(ready, "Desktop")
    expect(
      LaunchablesState.withStartStreamResult(launching, {
        _tag: "StreamStarted",
      }),
    ).toMatchObject({ _tag: "Launching", notice: null })
    const preparing = LaunchablesState.beginPreparing(ready, "Skate 3")
    expect(
      LaunchablesState.withPrepareOutcome(preparing, {
        _tag: "Ok",
        payload: { gameId: "skate3", launchId: "launch-1" },
      }),
    ).toMatchObject({ _tag: "Preparing", notice: null })
  })
})

const sessionActive: SessionStatusOutcome = {
  _tag: "Ok",
  payload: {
    active: { launchId: "l1", gameId: "skate3", title: "Skate 3", phase: "running" },
  },
}

const sessionIdle: SessionStatusOutcome = { _tag: "Ok", payload: {} }

const sessionErr: SessionStatusOutcome = {
  _tag: "Err",
  payload: { code: "HostUnavailable", message: "host is unavailable" },
}

describe("LaunchablesState now playing", () => {
  it("renders an active session as a selectable banner entry first", () => {
    const state = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      sessionActive,
    )
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries[0]).toEqual({
      kind: "now-playing",
      session: {
        launchId: "l1",
        gameId: "skate3",
        title: "Skate 3",
        phase: "running",
      },
    })
  })

  it("shows no banner when nothing is playing", () => {
    const state = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      sessionIdle,
    )
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.every(entry => entry.kind !== "now-playing")).toBe(true)
  })

  it("degrades a status failure silently: no banner, no notice", () => {
    const state = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      sessionErr,
    )
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.every(entry => entry.kind !== "now-playing")).toBe(true)
    expect(state.notice).toBeNull()
  })

  it("stop Ok enters an input-locked stopping case until status is idle", () => {
    const withBanner = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      sessionActive,
    )
    const ok: SessionStopOutcome = {
      _tag: "Ok",
      payload: { phase: SessionStopPhase.Stopped },
    }
    const stopRequested = LaunchablesState.beginStopping(
      withBanner,
      nowPlayingEntry(withBanner),
    )
    expect(stopRequested._tag).toBe("Stopping")
    const stopping = LaunchablesState.withStopOutcome(stopRequested, ok)
    expect(stopping._tag).toBe("Stopping")

    const stillActive = LaunchablesState.withStatusAfterStop(
      stopping,
      sessionActive,
    )
    expect(stillActive._tag).toBe("Stopping")

    const stopped = LaunchablesState.withStatusAfterStop(stillActive, sessionIdle)
    if (stopped._tag !== "Ready") throw new Error("unreachable")
    expect(stopped.entries.every(entry => entry.kind !== "now-playing")).toBe(true)

    const replacementSession: SessionStatusOutcome = {
      _tag: "Ok",
      payload: {
        active: { launchId: "l2", title: "Different game" },
      },
    }
    expect(
      LaunchablesState.withStatusAfterStop(stopping, replacementSession)._tag,
    ).toBe("Ready")

    const failed = LaunchablesState.withStopOutcome(stopRequested, {
      _tag: "Err",
      payload: { code: "HostUnavailable", message: "host is unavailable" },
    })
    if (failed._tag !== "Ready") throw new Error("unreachable")
    expect(failed.notice?.message).toBe("HostUnavailable: host is unavailable")
  })

  it("returns to the list with a truthful notice when pending stop times out", () => {
    const withBanner = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      sessionActive,
    )
    const stopping = LaunchablesState.withStopOutcome(
      LaunchablesState.beginStopping(withBanner, nowPlayingEntry(withBanner)),
      {
      _tag: "Ok",
      payload: { phase: SessionStopPhase.Pending },
      },
    )
    const timedOut = LaunchablesState.stopTimedOut(stopping)
    expect(timedOut).toMatchObject({
      _tag: "Ready",
      notice: { message: "StopPending: session is still stopping" },
    })
  })
})

describe("LaunchablesState preparing", () => {
  it("confirm on a game enters an explicit input-locked Preparing case", () => {
    const preparing = LaunchablesState.beginPreparing(ready, "Skate 3")
    if (preparing._tag !== "Preparing") throw new Error("unreachable")
    expect(preparing.title).toBe("Skate 3")
  })

  it("prepare Err restores the list with a notice", () => {
    const preparing = LaunchablesState.beginPreparing(ready, "Skate 3")
    const failed = LaunchablesState.withPrepareOutcome(preparing, {
      _tag: "Err",
      payload: { code: "UpstreamUnreachable", message: "host offline" },
    })
    if (failed._tag !== "Ready") throw new Error("unreachable")
    expect(failed.notice?.message).toBe("UpstreamUnreachable: host offline")
  })

  it("prepare Ok keeps preparing visible until the activity swap", () => {
    const preparing = LaunchablesState.beginPreparing(ready, "Skate 3")
    const prepared = LaunchablesState.withPrepareOutcome(preparing, {
      _tag: "Ok",
      payload: { gameId: "skate3", launchId: "launch-2" },
    })
    if (prepared._tag !== "Preparing") throw new Error("unreachable")
    expect(prepared.title).toBe("Skate 3")
  })

  it("a stream start failure clears preparing with a notice", () => {
    const preparing = LaunchablesState.beginPreparing(ready, "Skate 3")
    const failed = LaunchablesState.withStartStreamResult(preparing, {
      _tag: "StreamFailed",
      reason: "HostUnreachable",
      message: "no route",
    })
    if (failed._tag !== "Ready") throw new Error("unreachable")
    expect(failed.notice?.message).toBe("HostUnreachable: no route")
  })
})

describe("storage access prompt", () => {
  const denied = { _tag: "Denied" } as const

  it("puts a focusable prompt first when file access is denied", () => {
    const state = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      undefined,
      undefined,
      denied,
    )
    if (state._tag !== "Ready") throw new Error("unreachable")
    // It leads the list so a surface cannot bury it below things to play.
    expect(state.entries[0]).toEqual({ kind: "storage-access" })
  })

  it("shows nothing when access is granted", () => {
    const state = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      undefined,
      undefined,
      { _tag: "Granted" },
    )
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.some(entry => entry.kind === "storage-access")).toBe(false)
  })

  it("shows nothing on a platform where access is not a concept", () => {
    const state = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      undefined,
      undefined,
      { _tag: "NotRequired" },
    )
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.some(entry => entry.kind === "storage-access")).toBe(false)
  })

  it("does not nag when the check itself failed", () => {
    // An inconclusive answer is not a denial: prompting on a failed query
    // would badger users whose permission is actually fine.
    const state = LaunchablesState.fromSources(
      [officeApps],
      gamesOk,
      undefined,
      undefined,
      undefined,
      { _tag: "QueryFailed", message: "boom" },
    )
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.some(entry => entry.kind === "storage-access")).toBe(false)
  })

  it("keeps a stable key so the prompt does not remount on refresh", () => {
    expect(entryKey({ kind: "storage-access" })).toBe("storage-access")
  })
})
describe("background notice setting", () => {
  const build = (notice?: BackgroundNoticeResult) =>
    LaunchablesState.fromSources(
      [],
      { _tag: "Ok", payload: { games: [] } },
      undefined,
      undefined,
      undefined,
      undefined,
      notice,
    )

  it("is always offered, so the user can always find the switch", () => {
    const state = build({ _tag: "Visible" })
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.map(e => e.kind)).toContain("background-notice")
  })

  it("says which way it is set rather than what to do about it", () => {
    const on = build({ _tag: "Visible" })
    const off = build({ _tag: "Hidden" })
    if (on._tag !== "Ready" || off._tag !== "Ready") throw new Error("unreachable")
    expect(entryLabel(on.entries.at(-1)!)).toContain("on")
    expect(entryLabel(off.entries.at(-1)!)).toContain("off")
  })

  it("reads as off when the shell is too old to answer", () => {
    // An unanswered question is not a promise that the user can see anything.
    const state = build(undefined)
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.at(-1)).toMatchObject({
      kind: "background-notice",
      visible: false,
    })
  })
})

