/**
 * Save sync fixtures (ADR 0014): every screen and state Shift draws for save
 * sync, as whole `SurfaceModel`s, for tests and screenshots. Times are built
 * relative to `now` so their labels stay stable ("Today at …").
 *
 * Shapes follow contracts/surface/korri-surface.ts exactly. Ids are opaque
 * strings in Korri's style; Shift never reads meaning from them.
 */
import type {
  SurfaceAction,
  SurfaceGame,
  SurfaceModel,
  SurfaceSaveHistoryEntry,
  SurfaceStatus,
} from "@contracts/surface/korri-surface"
import { fixtureModel } from "./fixture-host"

const MIN = 60_000
const HOUR = 60 * MIN
const DAY = 24 * HOUR

/** A GBA-sized (240x160) stand-in screenshot, drawn as SVG. */
function shot(sky: string, ground: string, sun: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="240" height="160" viewBox="0 0 240 160" shape-rendering="crispEdges">
<rect width="240" height="160" fill="${sky}"/>
<rect x="180" y="18" width="24" height="24" fill="${sun}"/>
<rect y="112" width="240" height="48" fill="${ground}"/>
<rect x="24" y="96" width="16" height="16" fill="#f8d030"/>
<rect x="28" y="84" width="8" height="12" fill="#e83838"/>
<rect x="96" y="80" width="48" height="8" fill="${ground}"/>
<rect x="160" y="64" width="40" height="8" fill="${ground}"/>
<rect x="8" y="8" width="64" height="8" fill="#ffffff" opacity="0.8"/>
</svg>`
  return `data:image/svg+xml;base64,${btoa(svg)}`
}

/** Wide art for the backdrop, so the scene reads as it does with real art. */
const WIDE_ART = (() => {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 16 9">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#5a2c86"/><stop offset="0.55" stop-color="#c2477a"/><stop offset="1" stop-color="#f2a33a"/></linearGradient></defs>
<rect width="16" height="9" fill="url(#g)"/><circle cx="12" cy="3" r="1.6" fill="#ffe28a" opacity="0.8"/>
<path d="M0 7 L4 5 L7 6.5 L11 4.5 L16 6.5 L16 9 L0 9Z" fill="#2a1240" opacity="0.85"/></svg>`
  // Base64: the backdrop puts this inside an unquoted CSS url().
  return `data:image/svg+xml;base64,${btoa(svg)}`
})()

export const SAVE_SHOTS = {
  usu: shot("#78b8f8", "#58a838", "#f8f878"),
  zao: shot("#283868", "#704828", "#e8e8f8"),
} as const

const WL4 = "local-game:wl4"

const choose = (gameId: string): SurfaceAction => ({
  id: `saves-choose:${gameId}`,
  label: "Choose a save",
  enabled: true,
})

/** Every catalog game gets a mark; Wario Land 4 is split. */
export function markedGames(): readonly SurfaceGame[] {
  return [
    {
      id: WL4,
      title: "Wario Land 4",
      section: "Continue",
      subtitle: "GBA",
      wideArtUrl: WIDE_ART,
      lastPlayedAt: Date.now() - 2 * HOUR,
      totalPlaytimeSeconds: 11 * 3600 + 40 * 60,
      playCount: 14,
      saves: { _tag: "Split", choose: choose(WL4) },
    },
    {
      id: "local-game:minish",
      title: "The Legend of Zelda: The Minish Cap",
      section: "This device",
      subtitle: "GBA",
      saves: { _tag: "Synced" },
    },
    {
      id: "local-game:advance-wars",
      title: "Advance Wars",
      section: "This device",
      subtitle: "GBA",
      saves: { _tag: "NotChecked" },
    },
    {
      id: "game:zao:neverball",
      title: "Neverball",
      section: "zao",
      subtitle: "zao",
      saves: { _tag: "NotSynced" },
    },
    {
      id: "steam:hades",
      title: "Hades",
      section: "zao",
      subtitle: "Steam · zao",
      saves: { _tag: "SyncedBySteam" },
    },
  ]
}

export function savesModel(overrides: Partial<SurfaceModel> = {}): SurfaceModel {
  return {
    ...fixtureModel,
    catalog: { _tag: "Ready", games: markedGames() },
    ...overrides,
  }
}

/** The split choice for Wario Land 4. */
export function chooseSaveStatus(
  now: number = Date.now(),
  overrides: Partial<Extract<SurfaceStatus, { _tag: "ChooseSave" }>> = {},
): Extract<SurfaceStatus, { _tag: "ChooseSave" }> {
  return {
    _tag: "ChooseSave",
    gameId: WL4,
    kicker: "Saves split between two devices",
    actions: [{ id: "saves-choose-cancel", label: "Not now", enabled: true }],
    sides: [
      {
        actionId: "saves-pick:sha256:aa11",
        device: "usu",
        endedAt: now - 25 * MIN,
        durationSeconds: 72 * 60,
        imageUrl: SAVE_SHOTS.usu,
      },
      {
        actionId: "saves-pick:sha256:bb22",
        device: "zao",
        endedAt: now - DAY - 3 * HOUR,
        durationSeconds: 25 * 60 + 12,
        imageUrl: SAVE_SHOTS.zao,
      },
    ],
    ...overrides,
  }
}

/** Stress: no picture, a picture that fails, a long unbroken device name. */
export function chooseSaveStressStatus(now: number = Date.now()) {
  return chooseSaveStatus(now, {
    detail: "No other device answered. Your pick syncs when your devices meet again.",
    actions: [
      { id: "saves-choose-cancel", label: "Not now", enabled: true },
      { id: "saves-choose-history", label: "Open save history", enabled: true },
    ],
    sides: [
      {
        actionId: "saves-pick:sha256:cc33",
        device: "Retroid-Pocket-Mini-V2-living-room-handheld",
        endedAt: now - 400 * DAY,
        durationSeconds: 1204 * 3600 + 5 * 60,
      },
      {
        actionId: "saves-pick:sha256:dd44",
        device: "zao",
        endedAt: now - 30 * 1000,
        durationSeconds: 12,
        imageUrl: "data:image/png;base64,broken",
      },
    ],
  })
}

export const checkingStatus: SurfaceStatus = {
  _tag: "Busy",
  kicker: "Checking other devices…",
  gameId: WL4,
  actions: [{ id: "saves-check-skip", label: "Skip", enabled: true }],
}

export const inUseStatus: SurfaceStatus = {
  _tag: "Confirm",
  kicker: "In use on zao since 4:02 PM",
  detail: "Wario Land 4 may still be running there. If you play here too, Korri asks which save to keep later.",
  gameId: WL4,
  actions: [
    { id: "saves-inuse-launch", label: "Launch anyway", enabled: true },
    { id: "saves-inuse-cancel", label: "Cancel", enabled: true },
  ],
}

export const releaseMismatchStatus: SurfaceStatus = {
  _tag: "Confirm",
  kicker: "Different release",
  detail: "The latest save came from Wario Land 4 (Europe). You are starting Wario Land 4 (USA). The save may not load.",
  gameId: WL4,
  actions: [
    { id: "saves-mismatch-launch", label: "Launch anyway", enabled: true },
    { id: "saves-mismatch-cancel", label: "Cancel", enabled: true },
  ],
}

export const runnerMismatchStatus: SurfaceStatus = {
  _tag: "Confirm",
  kicker: "These savestates came from mGBA",
  detail: "You are starting Wario Land 4 with VBA-M. Its savestates may not load. In-game saves are not affected.",
  gameId: WL4,
  actions: [
    { id: "saves-runner-launch", label: "Launch anyway", enabled: true },
    { id: "saves-runner-cancel", label: "Cancel", enabled: true },
  ],
}

export const chooseSaveProblem: SurfaceStatus = {
  _tag: "Problem",
  kicker: "Couldn't use that save",
  reason: "Korri could not read the save files from zao. Try again when zao is on.",
  canRetry: true,
  gameId: WL4,
  gameTitle: "Wario Land 4",
}

const closeHistory: SurfaceAction = { id: "saves-history-close", label: "Close", enabled: true }

function entry(
  id: string,
  device: string,
  endedAt: number,
  durationSeconds: number,
  extra: Partial<SurfaceSaveHistoryEntry> = {},
): SurfaceSaveHistoryEntry {
  return {
    snapshotId: `sha256:${id}`,
    device,
    endedAt,
    durationSeconds,
    interrupted: false,
    current: false,
    restore: { id: `saves-restore:sha256:${id}`, label: "Restore", enabled: true },
    ...extra,
  }
}

export function historyEntries(now: number = Date.now(), count = 5): readonly SurfaceSaveHistoryEntry[] {
  const base: SurfaceSaveHistoryEntry[] = [
    entry("e01", "usu", now - 25 * MIN, 72 * 60, { current: true, restore: undefined }),
    entry("e02", "zao", now - DAY - 3 * HOUR, 25 * 60, { interrupted: true }),
    entry("e03", "usu", now - 2 * DAY, 40),
    entry("e04", "Retroid-Pocket-Mini-V2-living-room-handheld", now - 5 * DAY, 3 * 3600),
    entry("e05", "zao", now - 40 * DAY, 54 * 60, {
      restore: {
        id: "saves-restore:sha256:e05",
        label: "Restore",
        enabled: false,
        description: "Its files are on zao, which is not reachable.",
      },
    }),
  ]
  for (let i = base.length; i < count; i++) {
    base.push(entry(`e${String(i + 1).padStart(2, "0")}`, i % 2 ? "zao" : "usu", now - (i * 9 + 50) * DAY, (i * 17) * 60))
  }
  return base.slice(0, count)
}

export function historyModel(
  history: SurfaceModel["saveHistory"],
  overrides: Partial<SurfaceModel> = {},
): SurfaceModel {
  return savesModel({ ...(history === undefined ? {} : { saveHistory: history }), ...overrides })
}

export const historyStates = (now: number = Date.now()) => ({
  loading: { _tag: "Loading", gameId: WL4, actions: [closeHistory] },
  error: {
    _tag: "Error",
    gameId: WL4,
    message: "The save store on this device could not be read. Restart the device and try again.",
    actions: [closeHistory],
  },
  empty: { _tag: "Ready", gameId: WL4, entries: [], actions: [closeHistory] },
  ready: { _tag: "Ready", gameId: WL4, entries: historyEntries(now, 5), actions: [closeHistory] },
  many: { _tag: "Ready", gameId: WL4, entries: historyEntries(now, 14), actions: [closeHistory] },
}) satisfies Record<string, NonNullable<SurfaceModel["saveHistory"]>>

/** Review stress: many split games, long titles. */
export function manySplitGames(): readonly SurfaceGame[] {
  const titles = [
    "Wario Land 4",
    "The Legend of Zelda: The Minish Cap",
    "Pokémon Mystery Dungeon: Red Rescue Team",
    "Advance Wars 2: Black Hole Rising",
    "Golden Sun: The Lost Age",
    "Metroid Fusion",
    "Mother 3",
    "Castlevania: Aria of Sorrow",
    "Final Fantasy Tactics Advance",
    "Kirby & The Amazing Mirror",
    "Fire Emblem: The Sacred Stones",
    "Superlongtitlewithnobreakpointsatallforthestresscase",
  ]
  return titles.map((title, i) => ({
    id: `local-game:split-${i}`,
    title,
    subtitle: "GBA",
    saves: { _tag: "Split", choose: choose(`local-game:split-${i}`) },
  }))
}

export function noSplitGames(): readonly SurfaceGame[] {
  return markedGames().map(game =>
    game.saves?._tag === "Split" ? { ...game, saves: { _tag: "Synced" } } : game,
  )
}

export { WL4 as SAVE_FIXTURE_GAME_ID }
