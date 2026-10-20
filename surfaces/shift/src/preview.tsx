import type { SurfaceAction } from "@contracts/surface/korri-surface"
import type { ReactNode } from "react"
import { createRoot } from "react-dom/client"
import { createFixtureHost, fixtureModel } from "./fixtures/fixture-host"
import { SurfaceHostProvider } from "./host/surface-host"
import { ShiftCinematicHome, type ShiftCinematicGame } from "./pages/ShiftCinematicHome"
import { ShiftDetailSplit } from "./pages/ShiftDetailSplit"
import type { ShiftGameDetailView } from "./pages/shift-game-detail-view"
import { ShiftHomeEmptyBody } from "./pages/ShiftHomeEmptyBody"
import { ShiftHomeLoadErrorBody } from "./pages/ShiftHomeLoadErrorBody"
import { ShiftHomeLoadingBody } from "./pages/ShiftHomeLoadingBody"
import { ShiftLibraryCount } from "./pages/ShiftLibraryCount"
import type { ShiftLibraryGame } from "./pages/shift-library-game"
import { ShiftLibraryLens } from "./pages/ShiftLibraryLens"
import { ShiftSettings } from "./pages/ShiftSettings"
import { ShiftStore, type ShiftStorePlace } from "./pages/ShiftStore"
import { STORE_GETS, storeDetails, storeFixture } from "./fixtures/store-fixtures"
import {
  SHIFT_DESIGN_PARTS,
  type ShiftDesignPartLayer,
} from "./shift-design-parts"
import { DEFAULT_SHIFT_NETWORK_READING } from "./shift-network-state"
import "./shift.css"
import { ShiftBattery } from "./ui/atoms/ShiftBattery"
import { ShiftCoverArt } from "./ui/atoms/ShiftCoverArt"
import { ShiftNetworkIcon } from "./ui/atoms/ShiftNetworkIcon"
import { ShiftGameActionsSheet } from "./ui/organisms/ShiftGameActionsSheet"

/** A real Shift part that another tool can mount without knowing Shift uses React. */
export interface ShiftPartPreview {
  readonly id: string
  readonly name: string
  readonly layer: ShiftDesignPartLayer
  readonly mount: (host: HTMLElement) => { readonly unmount: () => void }
}

type PreviewScene =
  | "home"
  | "home-library"
  | "library"
  | "library-genre"
  | "library-empty"
  | "detail"
  | "settings"
  | "sheet"
  | "loading"
  | "error"
  | "empty"
  | "battery"
  | "cover-art"
  | "network"
  | "library-count"
  | "store-providers"
  | "store-results"
  | "store-searching"
  | "store-detail"

const SCENE_BY_PART = {
  backdrop: "home",
  battery: "battery",
  cineLibraryHero: "home-library",
  cineLibraryTile: "home",
  clock: "home",
  coverArt: "cover-art",
  detailActions: "detail",
  detailArt: "detail",
  detailButton: "detail",
  detailFavoriteBadge: "detail",
  detailHint: "detail",
  detailHints: "detail",
  detailStats: "detail",
  detailSynopsis: "detail",
  detailTags: "detail",
  detailTemplate: "detail",
  detailTitle: "detail",
  hero: "home",
  homeEmpty: "empty",
  homeLoadError: "error",
  homeLoading: "loading",
  homeTemplate: "home",
  legend: "home",
  lensRow: "library",
  lensSort: "library",
  lensTab: "library",
  libraryCount: "library-count",
  libraryEmpty: "library-empty",
  libraryGridView: "library",
  libraryHeader: "library",
  libraryHeading: "library",
  libraryShelf: "library-genre",
  libraryShelfStack: "library-genre",
  libraryShelfTitle: "library-genre",
  libraryTile: "library",
  libraryTileBadge: "library",
  libraryTileTitle: "library",
  monogram: "home",
  networkIcon: "network",
  rail: "home",
  settingGroup: "settings",
  settingRow: "settings",
  settingsTemplate: "settings",
  sheet: "sheet",
  sheetAction: "sheet",
  sheetGroup: "sheet",
  sheetPanel: "sheet",
  statusBar: "home",
  store: "store-providers",
  storeBrowseTile: "store-results",
  storeDetail: "store-detail",
  storeEmpty: "store-searching",
  storeFinder: "store-results",
  storeGet: "store-detail",
  storeIndexRow: "store-providers",
  storeProgress: "store-detail",
  tile: "home",
} as const satisfies Partial<
  Record<keyof typeof SHIFT_DESIGN_PARTS, PreviewScene>
>

/**
 * Parts that Shift can currently render as working examples. Entries that only
 * name future or retired layouts are deliberately omitted.
 */
export const shiftPartPreviews: readonly ShiftPartPreview[] = Object.entries(
  SCENE_BY_PART,
).map(([key, scene]) => {
  const part = SHIFT_DESIGN_PARTS[key as keyof typeof SCENE_BY_PART]
  return {
    id: part.id,
    name: part.name,
    layer: part.layer,
    mount: host => mountPartPreview(host, part.id, scene),
  }
})

const HOME_GAMES: readonly ShiftCinematicGame[] = [
  {
    id: "now-playing:L1",
    title: "Skate 3",
    tileArtUrl: "",
    wideArtUrl: "",
    section: "Continue",
    subtitle: "RPCS3 · This device",
    resumable: true,
  },
  {
    id: "local-game:wl4",
    title: "Wario Land 4",
    tileArtUrl: "",
    wideArtUrl: "",
    section: "This device",
    subtitle: "GBA",
  },
]

const LIBRARY_GAMES: readonly ShiftLibraryGame[] = [
  {
    id: "now-playing:L1",
    title: "Skate 3",
    artUrl: "",
    favorite: true,
    genre: "Sports",
    lastPlayedAt: Date.parse("2026-08-04T20:00:00.000Z"),
    playtimeMinutes: 184,
  },
  {
    id: "local-game:wl4",
    title: "Wario Land 4",
    artUrl: "",
    genre: "Platformer",
    lastPlayedAt: Date.parse("2026-08-01T20:00:00.000Z"),
    playtimeMinutes: 72,
  },
]

const DETAIL_GAME: ShiftGameDetailView = {
  id: "now-playing:L1",
  title: "Skate 3",
  artUrl: "",
  genre: "Sports",
  developer: "EA Black Box",
  lastPlayedLabel: "today",
  playtimeLabel: "3h 4m played",
  favorite: true,
}

const GAME_ACTIONS: readonly SurfaceAction[] = [
  { id: "continue", label: "Continue", enabled: true },
  { id: "remove", label: "Remove from device", enabled: true, destructive: true },
]

function mountPartPreview(
  host: HTMLElement,
  partId: string,
  scene: PreviewScene,
): { readonly unmount: () => void } {
  host.replaceChildren()
  host.dataset.shiftPartPreview = partId
  const root = createRoot(host)
  root.render(<PreviewFrame scene={scene} />)
  let disposed = false
  void isolatePart(host, partId, scene, () => disposed)
  return {
    unmount() {
      disposed = true
      root.unmount()
      host.replaceChildren()
      delete host.dataset.shiftPartPreview
      delete host.dataset.shiftPartPreviewReady
      delete host.dataset.shiftPartPreviewError
    },
  }
}

function PreviewFrame({ scene }: { readonly scene: PreviewScene }) {
  const fixtureHost = createFixtureHost({ "now-playing:L1": GAME_ACTIONS })
  return (
    <>
      <style>{PREVIEW_STYLE}</style>
      <div
        data-shift-part-preview-scene
        data-shift-surface
        data-shift-home-frame
        className="shift-part-preview-scene shift-sheet-host intrinsic"
      >
        <SurfaceHostProvider host={fixtureHost}>
          {sceneNode(scene)}
        </SurfaceHostProvider>
      </div>
    </>
  )
}

function sceneNode(scene: PreviewScene): ReactNode {
  switch (scene) {
    case "home":
    case "home-library":
      return (
        <ShiftCinematicHome
          games={HOME_GAMES}
          time="4:24 PM"
          network={DEFAULT_SHIFT_NETWORK_READING}
          battery={{ level: "medium", percent: 68, charging: true }}
          onOpenLibrary={() => undefined}
          actions={fixtureModel.actions}
          onAction={() => undefined}
          onOptions={() => undefined}
        />
      )
    case "library":
      return <ShiftLibraryLens games={LIBRARY_GAMES} />
    case "library-genre":
      return <ShiftLibraryLens games={LIBRARY_GAMES} lens="genre" />
    case "library-empty":
      return (
        <ShiftLibraryLens
          games={LIBRARY_GAMES.map(game => ({ ...game, favorite: false }))}
          lens="favorites"
        />
      )
    case "detail":
      return (
        <ShiftDetailSplit
          game={DETAIL_GAME}
          onPlay={() => undefined}
          onNewGame={() => undefined}
          onFavorite={() => undefined}
        />
      )
    case "settings":
      return (
        <ShiftSettings
          groups={fixtureModel.settings}
          status={fixtureModel.settingsStatus}
          time={fixtureModel.clockLabel}
          onChange={() => undefined}
          onAction={() => undefined}
          onDismissProblem={() => undefined}
          onClose={() => undefined}
        />
      )
    case "sheet":
      return (
        <ShiftGameActionsSheet
          open
          gameTitle="Skate 3"
          actions={GAME_ACTIONS}
          onSelect={() => undefined}
          onClose={() => undefined}
        />
      )
    case "loading":
      return <ShiftHomeLoadingBody />
    case "error":
      return (
        <ShiftHomeLoadErrorBody
          message="The game list could not be loaded."
          onRetry={() => undefined}
        />
      )
    case "empty":
      return <ShiftHomeEmptyBody />
    case "battery":
      return (
        <div data-shift-home className="shift-cine">
          <ShiftBattery level="medium" percent={68} charging />
        </div>
      )
    case "cover-art":
      return (
        <div data-shift-home className="shift-cine">
          <div className="shift-cine-tile">
            <ShiftCoverArt
              src="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='320' height='480'%3E%3Crect width='320' height='480' fill='%231c2440'/%3E%3C/svg%3E"
              alt="Skate 3 cover"
              title="Skate 3"
            />
          </div>
        </div>
      )
    case "network":
      return (
        <div data-shift-home className="shift-cine">
          <ShiftNetworkIcon network={DEFAULT_SHIFT_NETWORK_READING} />
        </div>
      )
    case "store-providers":
    case "store-results":
    case "store-searching":
    case "store-detail": {
      const place: ShiftStorePlace = scene === "store-providers"
        ? { view: "providers", text: "" }
        : { view: scene === "store-detail" ? "detail" : "results", providerId: "@korri:homebrewhub", text: "2048" }
      const store = storeFixture({
        ...(scene === "store-searching" ? { search: { _tag: "Searching", providerId: "@korri:homebrewhub", query: "2048" } } : {}),
        ...(scene === "store-detail" ? { details: storeDetails(STORE_GETS.downloading) } : {}),
      })
      return (
        <ShiftStore
          store={store}
          place={place}
          onPlace={() => undefined}
          onSearch={() => undefined}
          onOpenEntry={() => undefined}
          onAction={() => undefined}
          onPlay={() => undefined}
          onClose={() => undefined}
        />
      )
    }
    case "library-count":
      return (
        <div data-shift-library>
          <ShiftLibraryCount count={12} />
        </div>
      )
  }
}

async function isolatePart(
  host: HTMLElement,
  partId: string,
  scene: PreviewScene,
  cancelled: () => boolean,
): Promise<void> {
  try {
    const frame = await waitForElement(
      host,
      "[data-shift-part-preview-scene]",
      cancelled,
    )
    if (scene === "home-library") {
      const library = await waitForElement(
        frame,
        'button[aria-label="Library"]',
        cancelled,
      )
      library.focus()
    }
    const target = await waitForElement(
      frame,
      `[data-korri-part="${CSS.escape(partId)}"]`,
      cancelled,
    )
    const targetRect = await waitForVisibleBox(target, partId, cancelled)
    if (cancelled()) return
    const frameRect = frame.getBoundingClientRect()
    target.dataset.shiftPartPreviewTarget = ""
    frame.dataset.shiftPartPreviewIsolated = ""
    frame.style.transform = `translate(${Math.round(frameRect.left - targetRect.left)}px, ${Math.round(frameRect.top - targetRect.top)}px)`
    host.style.width = `${Math.ceil(targetRect.width)}px`
    host.style.height = `${Math.ceil(targetRect.height)}px`
    host.dataset.shiftPartPreviewReady = partId
  } catch (cause) {
    if (cancelled()) return
    host.dataset.shiftPartPreviewError =
      cause instanceof Error ? cause.message : String(cause)
  }
}

function waitForElement(
  root: ParentNode,
  selector: string,
  cancelled: () => boolean,
): Promise<HTMLElement> {
  return new Promise((resolve, reject) => {
    let attempts = 0
    const inspect = () => {
      if (cancelled()) return reject(new Error("Shift part preview cancelled"))
      const element = root.querySelector<HTMLElement>(selector)
      if (element) return resolve(element)
      attempts += 1
      if (attempts >= 120) {
        return reject(new Error(`Shift part preview did not render ${selector}`))
      }
      requestAnimationFrame(inspect)
    }
    inspect()
  })
}

function waitForVisibleBox(
  element: HTMLElement,
  partId: string,
  cancelled: () => boolean,
): Promise<DOMRect> {
  return new Promise((resolve, reject) => {
    let attempts = 0
    const inspect = () => {
      if (cancelled()) return reject(new Error("Shift part preview cancelled"))
      const rect = element.getBoundingClientRect()
      if (rect.width > 0 && rect.height > 0) return resolve(rect)
      attempts += 1
      if (attempts >= 120) {
        return reject(new Error(`Shift part ${partId} rendered without a visible box`))
      }
      requestAnimationFrame(inspect)
    }
    inspect()
  })
}

const PREVIEW_STYLE = `
  [data-surface-harness-part] { display: block; overflow: hidden; }
  .shift-part-preview-scene {
    position: relative;
    width: 960px;
    height: 540px;
    overflow: hidden;
    transform-origin: top left;
  }
  .shift-part-preview-scene > [data-shift-home],
  .shift-part-preview-scene > [data-shift-settings],
  .shift-part-preview-scene > [data-shift-detail],
  .shift-part-preview-scene > [data-shift-library],
  .shift-part-preview-scene > [data-shift-store] {
    width: 100%;
    height: 100%;
    min-height: 540px;
  }
  [data-shift-part-preview-isolated] * {
    visibility: hidden !important;
  }
  [data-shift-part-preview-target],
  [data-shift-part-preview-target] * {
    visibility: visible !important;
  }
`

/** Tool-neutral preview composition for hosts that render Shift without Korri. */
export {
  createFixtureHost,
  fixtureModel,
  type FixtureHost,
} from "./fixtures/fixture-host"
