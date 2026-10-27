import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { type ComponentProps, useState } from "react"
import { createFixtureHost, fixtureModel } from "../fixtures/fixture-host"
import {
  fixtureInvite,
  fixtureInvites,
  fixtureShelfGame,
  inviteModel,
  inviteNoticeViews,
  longInvite,
  manyInvites,
  noArtworkGame,
  streamConnectingStatus,
  streamEndedStatus,
  streamFailedStatus,
  streamPausedStatus,
  netplayCheckingStatus,
  netplayConnectingStatus,
  netplayCopyFailedStatus,
  netplayCopyInvite,
  netplayCopyingLongStatus,
  netplayCopyingStatus,
  netplayEndedStatus,
  netplayFindingStatus,
  netplayInstallFailedStatus,
  netplayInstallingStatus,
  netplayInstallInvite,
  netplayLongInvites,
  netplayLostStatus,
  netplayMixedInvites,
  netplayNoRouteStatus,
  netplayNoSpaceStatus,
  netplayPausedStatus,
  netplayReadyInvite,
  netplayReconnectingStatus,
  netplayReviewLongStatus,
  netplayReviewStatus,
  netplayVersionStatus,
} from "../fixtures/named-states"
import { picoInvitesFrom } from "../pico-invite-view"
import { picoScreenViewFromModel } from "../pico-screen-view"
import { PicoInviteNotices } from "../ui/organisms/PicoInviteNotices"
import { PicoHome, type PicoHomeMenu, type PicoHomeMode } from "./PicoHome"

export const name = "Home"
export const note = "The shelf, with Korri's catalog ready"
export const surface = true

// Empty removes the shelf; it is deliberately not a shelf composition.
export const composition = {
  NoArtwork: [{ part: "src/ui/organisms/PicoCartShelf.organism.part.tsx", state: "NoArtwork" }],
} as const

const closedMenu: PicoHomeMenu = {
  open: false,
  returning: false,
  onToggle: () => undefined,
  onFind: () => undefined,
  onSettings: () => undefined,
  onView: () => undefined,
  onReturned: () => undefined,
  aim: undefined,
  onAim: () => undefined,
}

/* The surface keeps the chosen cart; here the part does, so the shelf still
 * follows the d-pad in a preview. */
function Home(props: Omit<ComponentProps<typeof PicoHome>, "selectedGameId" | "onSelectGame">) {
  const [selected, setSelected] = useState<string | undefined>(undefined)
  const [aim, setAim] = useState<string | undefined>(undefined)
  const menu = props.menu === undefined ? undefined : { ...props.menu, aim, onAim: setAim }
  return <PicoHome {...props} menu={menu} onSelectGame={setSelected} selectedGameId={selected} />
}

function home(
  model: SurfaceModel,
  mode: PicoHomeMode = "shelf",
  placing?: ReturnType<typeof fixtureShelfGame>,
  menu: PicoHomeMenu = closedMenu,
  notices?: ComponentProps<typeof PicoInviteNotices>["notices"],
) {
  const host = createFixtureHost()
  const page = <Home clockLabel={model.clockLabel} menu={menu} mode={mode} placing={placing}
    onOpenGame={() => undefined} onChooseLocation={id => host.launchGame("tetris", id)}
    onRetry={host.retry} onDismiss={host.dismiss} view={picoScreenViewFromModel(model)}
    invites={picoInvitesFrom(model)} onInviteAction={host.runAction} />
  // The notices are the composition root's, over every screen; laid over home as it lays them.
  return notices === undefined ? page : <>{page}<PicoInviteNotices notices={notices} /></>
}

export function Loading() { return home({ ...fixtureModel, catalog: { _tag: "Loading" } }) }
export function Empty() { return home({ ...fixtureModel, catalog: { _tag: "Empty" } }) }
export function Error() {
  return home({ ...fixtureModel, catalog: { _tag: "Error", message: "The library could not be read." } })
}
export function Ready() { return home(fixtureModel) }
export function NoArtwork() {
  return home({ ...fixtureModel, catalog: { _tag: "Ready", games: [noArtworkGame] } })
}
export function Grid() { return home(fixtureModel, "grid") }
export function Hero() { return home(fixtureModel, "hero") }
export function MenuOpen() { return home(fixtureModel, "shelf", undefined, { ...closedMenu, open: true }) }
export function ChooseLocation() { return home(fixtureModel, "shelf", fixtureShelfGame("tetris")) }
export function Starting() {
  return home({ ...fixtureModel, status: { _tag: "Busy", kicker: "STARTING", detail: "Preparing the game.", gameId: "hollow" } })
}
/** A launch the host can still cancel, as the RP Mini V2 showed on 2026-10-18. */
export function StartingWithCancel() {
  return home({
    ...fixtureModel,
    status: {
      _tag: "Busy",
      kicker: "Starting Hollow…",
      detail: "Opening your session",
      gameId: "hollow",
      actions: [{ id: "cancel", label: "Cancel", enabled: true }],
    },
  })
}
export function Running() {
  return home({ ...fixtureModel, status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" } })
}
/** The question before a first launch, as the host words it (ADR 0012). */
export function NeedsDownload() {
  return home({
    ...fixtureModel,
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
}
export function NeedsDownloadUnknownSize() {
  return home({
    ...fixtureModel,
    status: {
      _tag: "Confirm",
      kicker: "Needs a download",
      detail: "FEX rootfs, Proton-CachyOS, Steam Runtime, Vulkan layers, Mesa, lib32-mesa and libdisplay-info, size unknown",
      gameId: "petal",
      actions: [
        { id: "acquisition:1:download", label: "Download", enabled: true },
        { id: "acquisition:1:cancel", label: "Cancel", enabled: true },
      ],
    },
  })
}
export function Downloading() {
  return home({
    ...fixtureModel,
    status: {
      _tag: "Busy",
      kicker: "Downloading FEX rootfs",
      detail: "116 MB of 1.2 GB · 6.4 MB/s · 2:43 left",
      progress: { done: 116_000_000, total: 1_160_000_000 },
      gameId: "hollow",
      actions: [{ id: "acquisition:2:cancel", label: "Cancel", enabled: true }],
    },
  })
}
export function DownloadingNearlyDone() {
  return home({
    ...fixtureModel,
    status: {
      _tag: "Busy",
      kicker: "Downloading FEX rootfs",
      detail: "1 GB of 1.2 GB · 6.4 MB/s · 0:18 left",
      progress: { done: 1_044_000_000, total: 1_160_000_000 },
      gameId: "hollow",
      actions: [{ id: "acquisition:2:cancel", label: "Cancel", enabled: true }],
    },
  })
}
/** Unpacking: a position of nothing and no known end, so the band walks. */
export function Unpacking() {
  return home({ ...fixtureModel, status: { _tag: "Busy", kicker: "Unpacking FEX rootfs", progress: { done: 0 }, gameId: "hollow" } })
}
export function DownloadFailed() {
  return home({
    ...fixtureModel,
    status: {
      _tag: "Problem",
      kicker: "Couldn't download FEX rootfs",
      reason: "The connection stalled. Check the network and try again.",
      canRetry: true,
      gameId: "hollow",
      gameTitle: "Hollow Knight",
    },
  })
}
/** A shelf where the chosen cart still has something to fetch. */
export function ShelfWithDownload() {
  return home({
    ...fixtureModel,
    catalog: fixtureModel.catalog._tag !== "Ready" ? fixtureModel.catalog : {
      _tag: "Ready",
      games: fixtureModel.catalog.games.map(game =>
        game.id === "celeste" ? { ...game, download: { bytes: 1_500_000_000 } }
          : game.id === "petal" ? { ...game, download: {} } : game),
    },
  })
}
/** A shelf where the chosen cart downloads in the background, another waits its turn. */
export function ShelfDownloading() {
  return home({
    ...fixtureModel,
    catalog: fixtureModel.catalog._tag !== "Ready" ? fixtureModel.catalog : {
      _tag: "Ready",
      games: fixtureModel.catalog.games.map(game =>
        game.id === "celeste" ? { ...game, download: { bytes: 1_500_000_000, progress: { done: 615_000_000, total: 1_500_000_000 } } }
          : game.id === "petal" ? { ...game, download: { bytes: 20_000_000, queued: true } } : game),
    },
  })
}
export function RetryableProblem() {
  return home({ ...fixtureModel, status: { _tag: "Problem", kicker: "COULD NOT START", reason: "The device did not respond.", canRetry: true, gameTitle: "Tetris" } })
}
export function Problem() {
  return home({ ...fixtureModel, status: { _tag: "Problem", kicker: "COULD NOT START", reason: "No installed runner can play this game.", canRetry: false, gameTitle: "Tetris" } })
}

/* ---- Stream play ---- */

/** An invite arrives on the idle shelf: its notice in the corner, and its
 * row under PLAYING NOW. */
export function InviteNotice() {
  return home(inviteModel([fixtureInvite]), "shelf", undefined, closedMenu, inviteNoticeViews([fixtureInvite]))
}
/** Three arrive together: one notice each, newest on top. */
export function InviteNotices() {
  return home(inviteModel(fixtureInvites), "shelf", undefined, closedMenu, inviteNoticeViews(fixtureInvites))
}
/** After the notice: PLAYING NOW over the shelf, one game. */
export function PlayingNowOne() {
  return home(inviteModel([fixtureInvite]))
}
export function PlayingNowThree() {
  return home(inviteModel(fixtureInvites))
}
/** More than its share holds: the rows scroll above the shelf. */
/** The longest names, and a game with no cover art. */
export function PlayingNowLongNames() {
  return home(inviteModel([fixtureInvite, longInvite]), "shelf", undefined, closedMenu, inviteNoticeViews([longInvite]))
}
export function PlayingNowMany() {
  return home(inviteModel(manyInvites))
}
/** Nothing in this device's library, and a game to play on another's stream. */
export function PlayingNowEmptyLibrary() {
  return home({ ...inviteModel([fixtureInvite]), catalog: { _tag: "Empty" } })
}
export function PlayingNowGrid() {
  return home(inviteModel(fixtureInvites), "grid")
}
export function PlayingNowHero() {
  return home(inviteModel([fixtureInvite]), "hero")
}
/** Play on stream pressed: the host connects, with its Cancel. */
export function StreamConnecting() {
  return home({ ...fixtureModel, status: streamConnectingStatus })
}
export function StreamFailed() {
  return home({ ...fixtureModel, status: streamFailedStatus })
}
export function StreamPaused() {
  return home({ ...fixtureModel, status: streamPausedStatus })
}
export function StreamEnded() {
  return home({ ...fixtureModel, status: streamEndedStatus })
}

/* ---- Netplay, on the receiving device ---- */

/** Netplay is possible: Play here beside Play on stream. */
export function NetplayPlayHere() { return home(inviteModel([netplayReadyInvite])) }
/** The release is copied first: its size is on the button. */
export function NetplayCopyAndPlay() { return home(inviteModel([netplayCopyInvite])) }
/** The core is missing: the plugin is installed first. */
export function NetplayInstallAndPlay() { return home(inviteModel([netplayInstallInvite])) }
/** Every route, and an invite with stream play only. */
export function NetplayRoutes() { return home(inviteModel(netplayMixedInvites)) }
/** The longest words: a long plugin title, a size in GB, names with no break. */
export function NetplayLongNames() { return home(inviteModel(netplayLongInvites)) }
export function NetplayCopying() { return home({ ...fixtureModel, status: netplayCopyingStatus }) }
export function NetplayCopyingLong() { return home({ ...fixtureModel, status: netplayCopyingLongStatus }) }
export function NetplayConnecting() { return home({ ...fixtureModel, status: netplayConnectingStatus }) }
/** The holder left the session: this device waits, and can Leave. */
export function NetplayPaused() { return home({ ...fixtureModel, status: netplayPausedStatus }) }
/** A drop: korrid retries for about 10 s, with nothing to press. */
export function NetplayReconnecting() { return home({ ...fixtureModel, status: netplayReconnectingStatus }) }
export function NetplayEnded() { return home({ ...fixtureModel, status: netplayEndedStatus }) }
export function NetplayLost() { return home({ ...fixtureModel, status: netplayLostStatus }) }
export function NetplayNoRoute() { return home({ ...fixtureModel, status: netplayNoRouteStatus }) }
export function NetplayVersionMismatch() { return home({ ...fixtureModel, status: netplayVersionStatus }) }
export function NetplayCopyFailed() { return home({ ...fixtureModel, status: netplayCopyFailedStatus }) }
export function NetplayNoSpace() { return home({ ...fixtureModel, status: netplayNoSpaceStatus }) }
export function NetplayFinding() { return home({ ...fixtureModel, status: netplayFindingStatus }) }
export function NetplayChecking() { return home({ ...fixtureModel, status: netplayCheckingStatus }) }
/** Settings' review, before anything installs. */
export function NetplayInstallReview() { return home({ ...fixtureModel, status: netplayReviewStatus }) }
export function NetplayInstallReviewLong() { return home({ ...fixtureModel, status: netplayReviewLongStatus }) }
/** Installing has no Cancel: the plugin is half in place until it ends. */
export function NetplayInstalling() { return home({ ...fixtureModel, status: netplayInstallingStatus }) }
export function NetplayInstallFailed() { return home({ ...fixtureModel, status: netplayInstallFailedStatus }) }

export default function PicoHomePart() {
  return (
    <Home
      clockLabel="10:24"
      menu={closedMenu}
      onChooseLocation={() => undefined}
      onDismiss={() => undefined}
      mode="shelf"
      onOpenGame={() => undefined}
      onRetry={() => undefined}
      view={{
        _tag: "Shelf",
        games: [
          { id: "celeste", subtitle: "PICO-8 · This device", title: "Celeste Classic" },
          { id: "hollow", resumable: true, subtitle: "GBA · This device", title: "Hollow Knight" },
          { id: "tetris", playCount: 12, subtitle: "GB · zao", title: "Tetris" },
          { id: "zelda-la", resumable: true, subtitle: "GBC · This device", title: "Link's Awakening DX" },
          { id: "smb3", playCount: 3, subtitle: "NES · zao", title: "Super Mario Bros. 3" },
          { id: "earthbound", subtitle: "SNES · This device", title: "EarthBound" },
          { id: "sonic2", subtitle: "MD · zao", title: "Sonic 2" },
        ],
      }}
    />
  )
}
