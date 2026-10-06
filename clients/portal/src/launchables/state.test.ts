import { describe, expect, it, test } from "bun:test"
import type {
  CatalogSnapshotOutcome,
  LocalGamesListOutcome,
  SessionStatusOutcome,
  SessionStopOutcome,
} from "@contracts/generated/korrid"
import {
  FocusOwnership,
  InitialHandoff,
  PendingLaunchPhase,
  SessionStopPhase,
} from "@contracts/generated/korrid"
import { entryKey, entryLabel, isLocalCatalogSession, LaunchablesState } from "./state"
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
      { id: "skate3", title: "Skate 3", supportsRunnerSelection: false, source: source("aka") },
      { id: "neverball", title: "Neverball", supportsRunnerSelection: false, source: source("zao") },
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

const ready = LaunchablesState.fromSources(gamesOk)

describe("catalog session locality", () => {
  const catalogGame = {
    id: "wl4", title: "Wario Land 4", host: "local-label",
    supportsRunnerSelection: false,
    source: { label: "local-label", isLocal: true },
  }
  const session = { launchId: "exact-launch", gameId: "wl4", host: "local-label" }

  it("matches the source-local catalog when Linux status omits the catalog label", () => {
    const entries: PortalEntry[] = [{ kind: "game", game: catalogGame }]
    expect(isLocalCatalogSession(session, entries)).toBe(true)
    expect(isLocalCatalogSession({ ...session, host: "peer" }, entries)).toBe(false)
    expect(isLocalCatalogSession({ launchId: session.launchId, gameId: session.gameId, phase: "running" }, entries)).toBe(true)
    expect(isLocalCatalogSession({ ...session, gameId: "other" }, entries)).toBe(false)
    expect(isLocalCatalogSession(session, [{ kind: "game", game: { ...catalogGame, source: { ...catalogGame.source, isLocal: false } } }])).toBe(false)
  })

  it("keeps same-id remote copies isolated, including folded alternatives", () => {
    const peer = { ...catalogGame, host: "peer", source: source("peer") }
    const entries: PortalEntry[] = [{
      kind: "game", game: peer,
      alternatives: [{ kind: "remote", game: catalogGame }],
    }]
    const linuxSession = { launchId: "linux-launch", gameId: catalogGame.id, phase: "running" }
    expect(isLocalCatalogSession(linuxSession, entries)).toBe(true)
    expect(isLocalCatalogSession({ ...linuxSession, host: "peer" }, entries)).toBe(false)
    expect(isLocalCatalogSession(linuxSession, [{ kind: "game", game: peer }])).toBe(false)
    expect(isLocalCatalogSession(linuxSession, [{ kind: "game", game: { ...peer, host: undefined } }])).toBe(false)
    expect(isLocalCatalogSession({ launchId: "unknown-game" }, entries)).toBe(false)
  })

  it("finds a local catalog route folded under another primary copy", () => {
    expect(isLocalCatalogSession(session, [{
      kind: "local-game",
      game: { id: "peer-copy", title: "Wario Land 4", system: "GBA" },
      alternatives: [{ kind: "remote", game: catalogGame }],
    }])).toBe(true)
  })

  it("publishes prepare identity immediately, then retains it across failed status until idle", () => {
    const ready: State = { _tag: "Ready", entries: [{ kind: "game", game: catalogGame }], notice: null }
    const preparing = LaunchablesState.beginPreparing(ready, catalogGame.title)
    const acknowledged = LaunchablesState.withLocalCatalogPrepareOutcome(preparing, {
      _tag: "Ok", payload: { gameId: catalogGame.id, launchId: session.launchId },
    }, catalogGame)
    expect(acknowledged._tag).toBe("Ready")
    expect(nowPlayingEntry(acknowledged)).toEqual({ kind: "now-playing", session: { ...session, title: catalogGame.title }, localCatalogGame: catalogGame })
    const failedRead = LaunchablesState.withSessionStatus(acknowledged, {
      _tag: "Err", payload: { code: "StatusTimeout", message: "timeout" },
    })
    expect(failedRead).toMatchObject({ ...acknowledged, unavailableSessionStatus: {
      _tag: "Err", payload: { code: "StatusTimeout", message: "timeout" },
    } })
    expect(LaunchablesState.withSessionStatus(failedRead, { _tag: "Ok", payload: {} })).toEqual(ready)
  })

  it.each(["SessionCompleted", "NoActiveSession"])("treats %s as authoritative while observation failures retain the banner", code => {
    const running: SessionStatusOutcome = {
      _tag: "Ok", payload: { active: { launchId: session.launchId, gameId: catalogGame.id, phase: "running" } },
    }
    // services/korrid/src/lib.rs::host_session_status_outcome
    const completed: SessionStatusOutcome = {
      _tag: "Err", payload: { code, message: "no host launch is active" },
    }
    const loaded = LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [catalogGame] } }, running)
    const stopped = LaunchablesState.withSessionStatus(loaded, completed)
    expect(stopped).toMatchObject({ _tag: "Ready", notice: null })
    if (stopped._tag !== "Ready") throw new Error("not ready")
    expect(stopped.entries.some(entry => entry.kind === "now-playing")).toBe(false)
    expect(LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [catalogGame] } }, completed)).toEqual(stopped)
    const stopping = LaunchablesState.beginStopping(loaded, nowPlayingEntry(loaded))
    expect(LaunchablesState.withStatusAfterStop(stopping, completed)).toEqual(stopped)
    const unavailable = { _tag: "Err", payload: { code: "BrainUnreachable", message: "disconnected" } } as const
    expect(LaunchablesState.withSessionStatus(loaded, unavailable)).toMatchObject({ ...loaded, unavailableSessionStatus: unavailable })
  })

  it("retains source evidence only for the same launch without restoring stale catalog games", () => {
    const active = { launchId: session.launchId, gameId: catalogGame.id }
    const running: SessionStatusOutcome = { _tag: "Ok", payload: { active } }
    const loaded = LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [catalogGame] } }, running)
    if (loaded._tag !== "Ready") throw new Error("not ready")
    const failedCatalog = { _tag: "Err", payload: { code: "BrainUnreachable", message: "disconnected" } } as const
    const refreshed = LaunchablesState.fromSources(failedCatalog, running, undefined, loaded.entries)
    if (refreshed._tag !== "Ready") throw new Error("not ready")
    expect(refreshed.entries.some(entry => entry.kind === "game")).toBe(false)
    expect(isLocalCatalogSession(active, refreshed.entries)).toBe(true)
    const polled = LaunchablesState.withSessionStatus(refreshed, running)
    if (polled._tag !== "Ready") throw new Error("not ready")
    expect(isLocalCatalogSession(active, polled.entries)).toBe(true)
    for (const active of [
      { launchId: "replacement", gameId: catalogGame.id },
      { launchId: session.launchId, gameId: catalogGame.id, host: "peer" },
      { launchId: session.launchId, gameId: "other" },
    ]) {
      const replaced = LaunchablesState.withSessionStatus(polled, { _tag: "Ok", payload: { active } })
      if (replaced._tag !== "Ready") throw new Error("not ready")
      expect(isLocalCatalogSession(active, replaced.entries)).toBe(false)
    }
  })

  it("a status observation cannot unlock a command in progress", () => {
    const preparing = LaunchablesState.beginPreparing(ready, "Game")
    expect(LaunchablesState.withSessionStatus(preparing, { _tag: "Ok", payload: {} })).toBe(preparing)
  })
})

describe("LaunchablesState.fromSources", () => {

  it("shows matching local and Zao copies as one local-first game", () => {
    const identity = { kind: "hash" as const, value: "sha256:wario" }
    const state = LaunchablesState.fromSources({
        _tag: "Ok",
        payload: {
          games: [
            {
              id: "wl4",
              title: "Wario Land 4",
              host: "zao",
              identity,
              supportsRunnerSelection: false,
              source: source("zao"),
            },
          ],
        },
      }, undefined, {
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
      })

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
              supportsRunnerSelection: false,
              source: source("zao"),
            },
          },
        ],
      },
    ])
  })

  it("degrades a failed local-game source to a notice while entries remain", () => {
    const state = LaunchablesState.fromSources(gamesOk, undefined, {
        _tag: "Err",
        payload: { code: "LocalStorageUnavailable", message: "storage denied" },
      })
    expect(state).toMatchObject({
      _tag: "Ready",
      notice: { _tag: "Catalog", message: "local games: LocalStorageUnavailable: storage denied" },
    })
  })

  it("surfaces local configuration failures while keeping healthy local games", () => {
    const state = LaunchablesState.fromSources(gamesOk, undefined, {
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
      })
    expect(state).toMatchObject({
      _tag: "Ready",
      notice: { _tag: "Catalog", message: "local games: LocalConfigReloadFailed: library.yaml is malformed" },
    })
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries[0]).toMatchObject({
      kind: "local-game",
      game: { id: "wl4" },
    })
  })

  it("degrades a failed korrid catalog to a notice while entries remain", () => {
    const state = LaunchablesState.fromSources(gamesErr)
    expect(state).toMatchObject({
      _tag: "Ready",
      notice: { _tag: "Catalog", message: "games: UpstreamUnreachable: host offline" },
    })
  })

  it("surfaces partial host catalog failures while keeping healthy games", () => {
    const state = LaunchablesState.fromSources({
      _tag: "Ok",
      payload: {
        games: [
          {
            id: "legacy",
            title: "Legacy game",
            host: "aka",
            supportsRunnerSelection: false,
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
      notice: { _tag: "Catalog", message: "zao: UpstreamUnreachable: connection refused" },
    })
  })

  it("does not surface Sunshine app-query failures as catalog failures", () => {
    const state = LaunchablesState.fromSources(gamesOk)
    expect(state).toMatchObject({ _tag: "Ready", notice: null })
  })

  it("does not surface Sunshine host-query failures as catalog failures", () => {
    const state = LaunchablesState.fromSources(gamesOk)
    expect(state).toMatchObject({ _tag: "Ready", notice: null })
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
            supportsRunnerSelection: false,
            source: source("aka"),
          },
        }),
        entryKey({
          kind: "game",
          game: {
            id: "shared",
            title: "Shared",
            host: "zao",
            supportsRunnerSelection: false,
            source: source("zao"),
          },
        }),
      ],
    ).toEqual(["game:aka:shared", "game:zao:shared"])
  })
})


describe("LaunchablesState action results", () => {
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
    const state = LaunchablesState.fromSources(gamesOk, sessionActive)
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
    const state = LaunchablesState.fromSources(gamesOk, sessionIdle)
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.every(entry => entry.kind !== "now-playing")).toBe(true)
  })

  it("degrades a status failure silently: no banner, no notice", () => {
    const state = LaunchablesState.fromSources(gamesOk, sessionErr)
    if (state._tag !== "Ready") throw new Error("unreachable")
    expect(state.entries.every(entry => entry.kind !== "now-playing")).toBe(true)
    expect(state.notice).toBeNull()
  })

  it("stop Ok enters an input-locked stopping case until status is idle", () => {
    const withBanner = LaunchablesState.fromSources(gamesOk, sessionActive)
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
    const withBanner = LaunchablesState.fromSources(gamesOk, sessionActive)
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
      notice: { _tag: "Stop", message: "StopPending: session is still stopping" },
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
})

describe("storage access prompt", () => {
  const denied = { _tag: "Denied" } as const
})


describe("owned local startup observation", () => {
  const game = {
    id: "owned", title: "Owned game", supportsRunnerSelection: false,
    source: { label: "This device", isLocal: true },
  }
  const reservation = { gameId: game.id, launchId: "exact-owned" }
  const reserved = () => LaunchablesState.withStartupReservation(
    LaunchablesState.beginStartup(LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [game] } }), game), reservation, game,
  )
  const active = { ...reservation, phase: "running", initialHandoff: InitialHandoff.Waiting }
  const observed = (focusOwnership?: FocusOwnership) => ({
    _tag: "Ok" as const, payload: { active: { ...active, initialHandoff: focusOwnership === FocusOwnership.Launch ? InitialHandoff.Observed : InitialHandoff.Waiting,
      ...(focusOwnership === undefined ? {} : { focusOwnership }) } },
  })
  test.each([undefined, FocusOwnership.Excluded, FocusOwnership.Other])("ACK and focus %s do not prove readiness", focus => {
    const state = reserved()
    expect(LaunchablesState.withSessionStatus(state, observed(focus))._tag).toBe("Starting")
  })
  test("pending daemon preparation retains startup; exact initial handoff resolves without an ACK", () => {
    const state = reserved()
    const focused = observed(FocusOwnership.Launch)
    expect(LaunchablesState.withSessionStatus(state, { _tag: "Ok", payload: { pendingLaunches: [{ session: reservation, phase: PendingLaunchPhase.Preparing }] } })._tag).toBe("Starting")
    expect(LaunchablesState.withSessionStatus(state, focused)._tag).toBe("Ready")
    const handedOff = LaunchablesState.withSessionStatus(state, focused)
    expect(handedOff._tag).toBe("Ready")
    expect(LaunchablesState.withSessionStatus(handedOff, observed())._tag).toBe("Ready")
  })
  test("observed exact exit retires a still-unacknowledged start", () => {
    const pending = LaunchablesState.withSessionStatus(reserved(), observed())
    expect(pending._tag).toBe("Starting")
    expect(LaunchablesState.withSessionStatus(pending, { _tag: "Ok", payload: {} })._tag).toBe("Ready")
  })
  test.each(["frozen", "focus-failed"])("a missed final focus reply recovers on %s", phase => {
    const state = reserved()
    expect(LaunchablesState.withSessionStatus(state, { _tag: "Ok", payload: { active: { ...active, phase } } })._tag).toBe("Ready")
  })
  test("Pending cancellation ignores focus and observation failure until exact exit", () => {
    const state = LaunchablesState.cancelStartup(reserved())
    const focused = observed(FocusOwnership.Launch)
    expect(LaunchablesState.withSessionStatus(state, focused)._tag).toBe("Starting")
    expect(LaunchablesState.withSessionStatus(state, { _tag: "Err", payload: { code: "HostUnavailable", message: "offline" } })).toBe(state)
    expect(LaunchablesState.withSessionStatus(state, { _tag: "Err", payload: { code: "SessionCompleted", message: "ended" } })._tag).toBe("Ready")
    const replacement = LaunchablesState.withSessionStatus(state, { _tag: "Ok", payload: { active: { ...active, launchId: "replacement" } } })
    expect(replacement).toMatchObject({ _tag: "Starting", launchId: "replacement", cancelling: false, entries: expect.arrayContaining([expect.objectContaining({ session: expect.objectContaining({ launchId: "replacement" }) })]) })
  })
})

test("failed native status retains the actual pending/error payload, never synthesized healthy truth", () => {
  const game = { id: "local", title: "Local game", supportsRunnerSelection: false, source: { label: "This device", isLocal: true } }
  const catalog = { _tag: "Ok" as const, payload: { games: [game] } }
  const known = LaunchablesState.fromSources(catalog, { _tag: "Ok", payload: { active: {
    launchId: "A", gameId: game.id, phase: "frozen", initialHandoff: InitialHandoff.Observed,
  } } })
  if (known._tag !== "Ready") throw new Error("expected Ready")
  const unavailable: SessionStatusOutcome = { _tag: "Ok", payload: {
    pendingLaunches: [{ session: { gameId: game.id, launchId: "B" }, phase: PendingLaunchPhase.Preparing }],
    observationFailure: { code: "HostRecoveryBlocked", message: "native observation failed" },
  } }
  const refreshed = LaunchablesState.fromSources(catalog, unavailable, undefined, known.entries)
  expect(refreshed).toMatchObject({ _tag: "Ready", unavailableSessionStatus: unavailable })
  expect(nowPlayingEntry(refreshed)).toEqual(nowPlayingEntry(known))
  const polled = LaunchablesState.withSessionStatus(known, unavailable)
  expect(polled).toMatchObject({ _tag: "Ready", unavailableSessionStatus: unavailable })
  expect(nowPlayingEntry(polled)).toEqual(nowPlayingEntry(known))
})

describe("manual pending launch choices", () => {
  const local = { id: "wl4", title: "Local title", supportsRunnerSelection: false, source: { label: "This device", isLocal: true } }
  const ready = LaunchablesState.fromSources({ _tag: "Ok", payload: { games: [local] } })
  if (ready._tag !== "Ready") throw new Error("expected Ready")
  const status = (phase = PendingLaunchPhase.Reserved): SessionStatusOutcome => ({ _tag: "Ok", payload: { pendingLaunches: [
    { session: { gameId: local.id, launchId: "B" }, phase },
    { session: { gameId: local.id, launchId: "A" }, phase: PendingLaunchPhase.Preparing },
  ] } })
  test("candidate sorting and per-launch cancel intent survive reordered authoritative facts", () => {
    const first = LaunchablesState.recoverStartup(ready, status())
    expect(first._tag).toBe("Choosing")
    if (first._tag !== "Choosing") throw new Error("expected Choosing")
    expect(first.choices).toEqual([
      { launchId: "A", gameId: local.id, title: "Local title", phase: PendingLaunchPhase.Preparing, cancel: "idle" },
      { launchId: "B", gameId: local.id, title: "Local title", phase: PendingLaunchPhase.Reserved, cancel: "idle" },
    ])
    const sent = LaunchablesState.beginPendingCancellation(LaunchablesState.beginPendingCancellation(first, "A"), "B")
    const failed = LaunchablesState.withPendingCancellationOutcome(sent, "B", { _tag: "Err", payload: { code: "BrainUnreachable", message: "Exact cancel failed" } })
    const snapshot = status()
    if (snapshot._tag !== "Ok") throw new Error("expected Ok")
    const reordered = LaunchablesState.withSessionStatus(failed, { ...snapshot, payload: { pendingLaunches: [...snapshot.payload.pendingLaunches!].reverse() } })
    expect(reordered).toEqual(failed)
    expect(LaunchablesState.beginPendingCancellation(reordered, "A")).toBe(reordered)
    expect(LaunchablesState.beginPendingCancellation(reordered, "unknown")).toBe(reordered)
    const cancelling = LaunchablesState.withSessionStatus(reordered, status(PendingLaunchPhase.Cancelling))
    if (cancelling._tag !== "Choosing") throw new Error("expected Choosing")
    expect(cancelling.choices.map(choice => [choice.launchId, choice.cancel])).toEqual([["A", "sent"], ["B", "sent"]])
    expect(LaunchablesState.beginPendingCancellation(cancelling, "B")).toBe(cancelling)
  })
  test("unknown observation retains every choice and actual failure; positive absence retires them", () => {
    const choosing = LaunchablesState.recoverStartup(ready, status())
    const failedStatus: SessionStatusOutcome = { _tag: "Ok", payload: { observationFailure: { code: "NativeQueryFailed", message: "Window unknown" } } }
    const unknown = LaunchablesState.withSessionStatus(choosing, failedStatus)
    if (choosing._tag !== "Choosing" || unknown._tag !== "Choosing") throw new Error("expected Choosing")
    expect(unknown.choices).toBe(choosing.choices)
    expect(unknown.unavailableSessionStatus).toEqual(failedStatus)
    expect(unknown.notice).toEqual({ _tag: "Launch", message: "Window unknown" })
    expect(LaunchablesState.withSessionStatus(unknown, { _tag: "Ok", payload: {} })).toMatchObject({ _tag: "Ready", notice: null })
    expect(LaunchablesState.recoverStartup(ready, failedStatus)._tag).toBe("Recovery")
  })
  test("a sent surviving choice enters single cancelling and accepts only its exact late reply", () => {
    const sent = LaunchablesState.beginPendingCancellation(LaunchablesState.recoverStartup(ready, status()), "A")
    const single = LaunchablesState.withSessionStatus(sent, { _tag: "Ok", payload: { pendingLaunches: [
      { session: { gameId: local.id, launchId: "A" }, phase: PendingLaunchPhase.Preparing },
    ] } })
    expect(single).toMatchObject({ _tag: "Starting", launchId: "A", cancelling: true })
    const failure = { _tag: "Err", payload: { code: "BrainUnreachable", message: "Exact retry" } } as const
    expect(LaunchablesState.withPendingCancellationOutcome(single, "B", failure)).toBe(single)
    expect(LaunchablesState.withPendingCancellationOutcome(single, "A", failure)).toMatchObject({ _tag: "Starting", launchId: "A", cancelling: true, cancelRetryAvailable: true, notice: { message: "Exact retry" } })
  })
  test("a waiting session without game metadata keeps it absent and uses its real title", () => {
    const result = LaunchablesState.recoverStartup(ready, { _tag: "Ok", payload: {
      active: { launchId: "A", title: "Real live title", phase: "running", initialHandoff: InitialHandoff.Waiting },
      pendingLaunches: [{ session: { launchId: "B", gameId: "removed-game" }, phase: PendingLaunchPhase.Committing }],
    } })
    if (result._tag !== "Choosing") throw new Error("expected Choosing")
    expect(result.choices).toEqual([
      { launchId: "A", title: "Real live title", phase: "waiting", cancel: "idle" },
      { launchId: "B", gameId: "removed-game", title: "removed-game", phase: PendingLaunchPhase.Committing, cancel: "idle" },
    ])
    expect(result.choices[0]).not.toHaveProperty("gameId")
  })
  test("HostRecoveryBlocked enables only sent exact choices, preserves phases and healthy cancelling wins", () => {
    const choosing: State = { _tag: "Choosing", entries: [], notice: null, choices: [
      { launchId: "A", gameId: "wl4", title: "Wario", phase: PendingLaunchPhase.Cancelling, cancel: "sent" },
      { launchId: "B", gameId: "wl4", title: "Wario", phase: PendingLaunchPhase.Reserved, cancel: "idle" },
      { launchId: "C", gameId: "wl4", title: "Wario", phase: PendingLaunchPhase.Committing, cancel: "sent" },
    ] }
    const failed = LaunchablesState.withSessionStatus(choosing, { _tag: "Ok", payload: {
      observationFailure: { code: "HostRecoveryBlocked", message: "Exact cleanup failed" },
    } })
    if (failed._tag !== "Choosing") throw new Error("expected Choosing")
    expect(failed.choices.map(choice => [choice.launchId, choice.cancel, choice.phase])).toEqual([
      ["A", "retry", PendingLaunchPhase.Cancelling], ["B", "idle", PendingLaunchPhase.Reserved], ["C", "retry", PendingLaunchPhase.Committing],
    ])
    expect(failed.choices[1]).toBe(choosing.choices[1])
    expect(failed.notice).toEqual({ _tag: "Launch", message: "Exact cleanup failed" })
    const retry = LaunchablesState.beginPendingCancellation(failed, "A")
    if (retry._tag !== "Choosing") throw new Error("expected Choosing")
    expect(retry.choices.map(choice => choice.cancel)).toEqual(["sent", "idle", "retry"])
    expect(LaunchablesState.beginPendingCancellation(retry, "A")).toBe(retry)
    const healthy = LaunchablesState.withSessionStatus(failed, { _tag: "Ok", payload: { pendingLaunches: choosing.choices.map(choice => ({
      session: { launchId: choice.launchId, gameId: "wl4" },
      phase: choice.launchId === "B" ? PendingLaunchPhase.Reserved : PendingLaunchPhase.Cancelling,
    })) } })
    if (healthy._tag !== "Choosing") throw new Error("expected Choosing")
    expect(healthy.choices.map(choice => choice.cancel)).toEqual(["sent", "idle", "sent"])
  })
  test("stopping with waiting history is neither an active nor matching pending startup candidate", () => {
    const active = { launchId: "A", gameId: "wl4", phase: "stopping", initialHandoff: InitialHandoff.Waiting }
    const initial: Extract<State, { _tag: "Ready" }> = { _tag: "Ready", entries: [], notice: null }
    expect(LaunchablesState.recoverStartup(initial, { _tag: "Ok", payload: { active } })).toBe(initial)
    const pendingLaunches = [{ session: { launchId: "A", gameId: "wl4" }, phase: PendingLaunchPhase.Cancelling }]
    expect(LaunchablesState.recoverStartup(initial, { _tag: "Ok", payload: { active, pendingLaunches } })).toBe(initial)
    expect(LaunchablesState.recoverStartup(initial, { _tag: "Ok", payload: { active, pendingLaunches: [
      ...pendingLaunches, { session: { launchId: "B", gameId: "wl4" }, phase: PendingLaunchPhase.Reserved },
    ] } })).toMatchObject({ _tag: "Starting", launchId: "B", cancelling: false })
  })
  test("candidate exclusions remain unchanged for returned or already-handed-off sessions", () => {
    for (const active of [
      { phase: "frozen", initialHandoff: InitialHandoff.Waiting },
      { phase: "focus-failed", initialHandoff: InitialHandoff.Waiting },
      { phase: "running", initialHandoff: InitialHandoff.Observed },
      { phase: "running", initialHandoff: InitialHandoff.Recovered },
    ]) {
      const snapshot = status()
      if (snapshot._tag !== "Ok") throw new Error("expected Ok")
      expect(LaunchablesState.recoverStartup(ready, { _tag: "Ok", payload: {
        ...snapshot.payload, active: { ...active, launchId: "A", gameId: local.id },
      } })).toMatchObject({ _tag: "Starting", launchId: "B", cancelling: false })
    }
  })
})
