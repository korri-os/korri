/**
 * Shift's composition root — the only component a host renders.
 *
 * It takes one `SurfaceModel` (what Korri knows) plus one `SurfaceHost` (what
 * Shift may ask for) and owns everything else: which catalog body shows, which
 * game the command sheet is about, and how the model's shape becomes the
 * cinematic scene's props. Nothing below this file knows a host exists beyond
 * the semantic-input hook.
 */
import type {
  SurfaceAction,
  SurfaceGame,
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { useCallback, useEffect, useMemo, useState } from "react"
import { SurfaceHostProvider } from "./host/surface-host"
import {
  type ShiftCinematicGame,
  ShiftCinematicHome,
} from "./pages/ShiftCinematicHome"
import { ShiftHomeEmptyBody } from "./pages/ShiftHomeEmptyBody"
import { ShiftHomeLoadErrorBody } from "./pages/ShiftHomeLoadErrorBody"
import { ShiftHomeLoadingBody } from "./pages/ShiftHomeLoadingBody"
import { ShiftDetailSplit } from "./pages/ShiftDetailSplit"
import { ShiftLibraryLens } from "./pages/ShiftLibraryLens"
import { playtimeLabel, relativeLastPlayed } from "./play-labels"
import type { ShiftLibraryGame } from "./pages/shift-library-game"
import type { ShiftGameDetailView } from "./pages/shift-game-detail-view"
import { ShiftSettings } from "./pages/ShiftSettings"
import { ShiftGameActionsSheet } from "./ui/organisms/ShiftGameActionsSheet"
import { ShiftGameplayOverlaySheet } from "./ui/organisms/ShiftGameplayOverlaySheet"
import { ShiftLaunchLocationSheet } from "./ui/organisms/ShiftLaunchLocationSheet"

export interface ShiftSurfaceProps {
  readonly model: SurfaceModel
  readonly host: SurfaceHost
}

/**
 * Map one catalog entry onto the scene's game shape. Absent art becomes an
 * empty string, which is how Shift's cover-art atom self-selects the title
 * monogram — the gap is presented, never filled in with a placeholder asset.
 */
export function shiftGameFromSurfaceGame(
  game: SurfaceGame,
  now: Date = new Date(),
): ShiftCinematicGame {
  const labels = playLabels(game, now)
  return {
    id: game.id,
    title: game.title,
    tileArtUrl: game.coverArtUrl ?? "",
    wideArtUrl: game.wideArtUrl ?? "",
    ...(game.section === undefined ? {} : { section: game.section }),
    ...(game.subtitle === undefined ? {} : { subtitle: game.subtitle }),
    ...(game.resumable === undefined ? {} : { resumable: game.resumable }),
    ...(game.lastPlayedAt === undefined
      ? {}
      : { lastPlayedAt: game.lastPlayedAt }),
    ...labels,
  }
}

export function shiftLibraryGameFromSurfaceGame(
  game: SurfaceGame,
): ShiftLibraryGame {
  const minutes =
    game.totalPlaytimeSeconds === undefined
      ? undefined
      : Math.round(game.totalPlaytimeSeconds / 60)
  return {
    id: game.id,
    title: game.title,
    artUrl: game.coverArtUrl ?? "",
    ...(game.lastPlayedAt === undefined
      ? {}
      : { lastPlayedAt: game.lastPlayedAt }),
    ...(minutes === undefined ? {} : { playtimeMinutes: minutes }),
  }
}

export function shiftDetailGameFromSurfaceGame(
  game: SurfaceGame,
  now: Date = new Date(),
): ShiftGameDetailView {
  const labels = playLabels(game, now)
  return {
    id: game.id,
    title: game.title,
    artUrl: game.coverArtUrl ?? "",
    ...labels,
    // A running session outranks history: it is the thing you continue.
    ...(game.resumable ? { lastPlayedLabel: "Ready to continue" } : {}),
  }
}

/**
 * Format Korri's raw play facts into Shift's words. Shift owns the phrasing;
 * Korri only ever sends a timestamp and a count of seconds.
 */
function playLabels(
  game: SurfaceGame,
  now: Date,
): { readonly lastPlayedLabel?: string; readonly playtimeLabel?: string } {
  const lastPlayedLabel =
    game.lastPlayedAt === undefined
      ? undefined
      : relativeLastPlayed(new Date(game.lastPlayedAt), now)
  const playtime =
    game.totalPlaytimeSeconds === undefined
      ? undefined
      : playtimeLabel(Math.round(game.totalPlaytimeSeconds / 60))
  return {
    ...(lastPlayedLabel === undefined ? {} : { lastPlayedLabel }),
    ...(playtime === undefined ? {} : { playtimeLabel: playtime }),
  }
}

const HOME_CONTINUE_CAP = 8

/**
 * Keep Home curated, as the legacy Shift route did: active/recent work first,
 * then one rotating pick. The complete catalog belongs behind Library.
 *
 * Continue is the running session first, then games with a recorded
 * lastPlayedAt newest-first, capped. A game Korri has never seen played does
 * not appear here. `randomIndex` is injected to keep the pick deterministic
 * in tests.
 */
export function shiftHomeGamesFromCatalog(
  games: readonly ShiftCinematicGame[],
  randomIndex: (count: number) => number = count =>
    Math.floor(Math.random() * count),
): readonly ShiftCinematicGame[] {
  const running = games.filter(game => game.resumable)
  const runningIds = new Set(running.map(game => game.id))
  const recent = games
    .filter(game => !runningIds.has(game.id) && game.lastPlayedAt !== undefined)
    .sort((a, b) => (b.lastPlayedAt ?? 0) - (a.lastPlayedAt ?? 0))
  const continuing = [...running, ...recent].slice(0, HOME_CONTINUE_CAP)
  const continuingIds = new Set(continuing.map(game => game.id))
  const candidates = games.filter(game => !continuingIds.has(game.id))
  const random =
    candidates.length > 0
      ? candidates[
          Math.min(
            Math.max(randomIndex(candidates.length), 0),
            candidates.length - 1,
          )
        ]
      : undefined

  return [
    ...continuing.map(game => ({ ...game, section: "Continue" })),
    ...(random ? [{ ...random, section: "Random" }] : []),
  ]
}

/**
 * The rail's Settings destination. Shift owns it, not Korri: which screens a
 * surface has, and how you reach them, is the surface's business — Korri only
 * says what the settings themselves are. It joins the host's own rail actions
 * as an ordinary affordance, so the rail learns nothing new.
 */
export const SHIFT_SETTINGS_ACTION_ID = "shift:settings"

const SETTINGS_AFFORDANCE: SurfaceAction = {
  id: SHIFT_SETTINGS_ACTION_ID,
  label: "Settings",
  description: "What this device is, and what it can currently reach.",
  enabled: true,
}

export function ShiftSurface({ model, host }: ShiftSurfaceProps) {
  const [sheetGameId, setSheetGameId] = useState<string | null>(null)
  const [launchGameId, setLaunchGameId] = useState<string | null>(null)
  const [detailGameId, setDetailGameId] = useState<string | null>(null)
  const [screen, setScreen] = useState<
    "home" | "library" | "detail" | "settings"
  >("home")

  const surfaceGames = model.catalog._tag === "Ready" ? model.catalog.games : []
  const allGames = useMemo(
    () => surfaceGames.map(game => shiftGameFromSurfaceGame(game)),
    [surfaceGames],
  )
  // Stable per Home visit: ordinary re-renders must not reshuffle the pick.
  const homeRandomSeed = useMemo(() => Math.random(), [])
  const games = useMemo(
    () =>
      shiftHomeGamesFromCatalog(allGames, count =>
        Math.floor(homeRandomSeed * count),
      ),
    [allGames, homeRandomSeed],
  )
  const libraryGames = useMemo(
    () => surfaceGames.map(game => shiftLibraryGameFromSurfaceGame(game)),
    [surfaceGames],
  )

  const closeSheet = useCallback(() => setSheetGameId(null), [])
  const requestLaunch = useCallback(
    (gameId: string) => {
      const game = surfaceGames.find(candidate => candidate.id === gameId)
      if ((game?.launchLocations?.length ?? 0) > 1) {
        setLaunchGameId(gameId)
        return
      }
      setScreen("home")
      host.launchGame(gameId)
    },
    [host, surfaceGames],
  )

  // Library is Shift's dedicated destination. Generic rail actions are reserved
  // for newer host-backed destinations such as Settings.
  const railActions = useMemo<readonly SurfaceAction[]>(
    () => (model.settings.length > 0 ? [SETTINGS_AFFORDANCE] : []),
    [model.settings],
  )

  const runRailAction = useCallback((actionId: string) => {
    if (actionId === SHIFT_SETTINGS_ACTION_ID) setScreen("settings")
  }, [])

  const sheetGame = games.find(game => game.id === sheetGameId)
  const launchSurfaceGame = surfaceGames.find(game => game.id === launchGameId)
  const launchChooserOpen =
    (launchSurfaceGame?.launchLocations?.length ?? 0) > 1
  useEffect(() => {
    if (launchGameId !== null && !launchChooserOpen) setLaunchGameId(null)
  }, [launchChooserOpen, launchGameId])
  const detailSurfaceGame = surfaceGames.find(game => game.id === detailGameId)
  const detailGame = detailSurfaceGame
    ? shiftDetailGameFromSurfaceGame(detailSurfaceGame)
    : null
  // Ask the host only while the sheet is actually about a game, so a host that
  // computes actions lazily is not polled on every render.
  const sheetActions = sheetGameId ? host.gameActions(sheetGameId) : []
  // A game with no host-declared actions has no command sheet to show; the
  // Options affordance disappears with it rather than opening an empty panel.
  const hasGameActions =
    model.catalog._tag === "Ready" &&
    model.catalog.games.some(game => host.gameActions(game.id).length > 0)

  const body =
    model.presentation.kind === "gameplay-overlay" ? null
    : screen === "settings" ? (
      <ShiftSettings
        groups={model.settings}
        status={model.settingsStatus}
        onChange={(settingId, value) => host.changeSetting(settingId, value)}
        onAction={actionId => host.runAction(actionId)}
        onDismissProblem={() => host.dismissSettingsProblem()}
        {...(model.clockLabel === undefined ? {} : { time: model.clockLabel })}
        onClose={() => setScreen("home")}
      />
    ) : screen !== "home" && model.catalog._tag === "Loading" ? (
      <ShiftHomeLoadingBody />
    ) : screen !== "home" && model.catalog._tag === "Error" ? (
      <ShiftHomeLoadErrorBody
        message={model.catalog.message}
        onRetry={() => host.reload()}
      />
    ) : screen !== "home" && model.catalog._tag === "Empty" ? (
      <ShiftHomeEmptyBody />
    ) : screen === "library" ? (
      <ShiftLibraryLens
        games={libraryGames}
        onSelect={gameId => {
          setDetailGameId(gameId)
          setScreen("detail")
        }}
        onBack={() => setScreen("home")}
      />
    ) : screen === "detail" && detailGame ? (
      <ShiftDetailSplit
        game={detailGame}
        onPlay={() => requestLaunch(detailGame.id)}
        onBack={() => {
          if (!launchChooserOpen) setScreen("home")
        }}
      />
    ) : screen === "detail" ? (
      <main
        data-shift-home
        className="intrinsic relative flex h-full w-full flex-col items-center justify-center text-[color:var(--shift-ink)]"
      >
        <p className="opacity-70">Game not found.</p>
      </main>
    ) : model.catalog._tag === "Loading" ? (
      <ShiftHomeLoadingBody />
    ) : model.catalog._tag === "Error" ? (
      <ShiftHomeLoadErrorBody
        message={model.catalog.message}
        onRetry={() => host.reload()}
      />
    ) : model.catalog._tag === "Empty" && railActions.length > 0 ? (
      // A plugin can make the playable catalog empty. Settings must remain
      // reachable so the user can turn it back on rather than trapping the
      // device on an empty page.
      <ShiftCinematicHome
        games={[]}
        {...(model.clockLabel === undefined ? {} : { time: model.clockLabel })}
        status={model.status}
        statusGames={allGames}
        actions={railActions}
        onAction={runRailAction}
        onRetry={() => host.retry()}
        onDismiss={() => host.dismiss()}
      />
    ) : model.catalog._tag === "Empty" ? (
      <ShiftHomeEmptyBody />
    ) : (
      <ShiftCinematicHome
        games={games}
        {...(model.clockLabel === undefined ? {} : { time: model.clockLabel })}
        status={model.status}
        statusGames={allGames}
        onOpenLibrary={() => setScreen("library")}
        actions={railActions}
        onLaunch={requestLaunch}
        onAction={runRailAction}
        onRetry={() => host.retry()}
        onDismiss={() => host.dismiss()}
        {...(hasGameActions ? { onOptions: setSheetGameId } : {})}
      />
    )

  return (
    <SurfaceHostProvider host={host}>
      {/* The surface root is Shift's token scope, its size container, and the
          positioned/clipping box the sheet anchors to. */}
      <div
        data-shift-surface
        data-shift-home-frame
        data-shift-gameplay-overlay={
          model.presentation.kind === "gameplay-overlay" ? "" : undefined
        }
        className="shift-sheet-host intrinsic"
      >
        {body}
        {model.presentation.kind === "gameplay-overlay" ? (
          <ShiftGameplayOverlaySheet
            presentation={model.presentation}
            status={model.status}
          />
        ) : null}
        {sheetGame ? (
          <ShiftGameActionsSheet
            open
            gameTitle={sheetGame.title}
            actions={sheetActions}
            onSelect={actionId => {
              closeSheet()
              host.runGameAction(sheetGame.id, actionId)
            }}
            onClose={closeSheet}
          />
        ) : null}
        {launchChooserOpen && launchSurfaceGame?.launchLocations ? (
          <ShiftLaunchLocationSheet
            open
            gameTitle={launchSurfaceGame.title}
            locations={launchSurfaceGame.launchLocations}
            onSelect={launchLocationId => {
              setLaunchGameId(null)
              setScreen("home")
              host.launchGame(launchSurfaceGame.id, launchLocationId)
            }}
            onClose={() => setLaunchGameId(null)}
          />
        ) : null}
      </div>
    </SurfaceHostProvider>
  )
}
