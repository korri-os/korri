import type { SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import { type ComponentProps, useRef } from "react"
import { fixtureTitleOf, statusBusyDetail } from "../fixtures/fixture-layers"
import { useFixtureHold } from "../fixtures/fixture-moments"
import { ZonePartFrame } from "../fixtures/ZonePartFrame"
import {
  createGamesFixtureHost,
  fixtureDownloadModel,
  fixtureEmptyModel,
  fixtureErrorModel,
  fixtureFortyModel,
  fixtureLoadingModel,
  fixtureOneGameModel,
  fixtureReadyModel,
  fixtureSectionsModel,
  fixtureWideArtIndex,
  fixtureWideArtModel,
} from "../fixtures/zone-games"
import { ZoneGround } from "../ui/organisms/ZoneGround"
import { ZoneStatusLayer } from "../ui/organisms/ZoneStatusLayer"
import { useZoneGames } from "../use-zone-games"
import { ZoneGames } from "./ZoneGames"

export const name = "Games"
export const note = "Zone's home: the zone select, the focused game centred, its places under it, GAME OPTS. and FIND over it"

export const composition = {
  default: [
    { part: "src/ui/templates/ZoneShell.template.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneTabStrip.molecule.part.tsx", state: "GamesActive" },
    { part: "src/ui/molecules/ZoneCardBand.molecule.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneActionRow.molecule.part.tsx", state: "OneButton" },
    { part: "src/ui/molecules/ZoneFoot.molecule.part.tsx", state: "Wifi" },
  ],
  Loading: [{ part: "src/ui/molecules/ZoneBusyStrip.molecule.part.tsx", state: "default" }],
  Error: [{ part: "src/ui/molecules/ZoneDialog.molecule.part.tsx", state: "OneButton" }],
  Empty: [{ part: "src/ui/molecules/ZoneCardBand.molecule.part.tsx", state: "NoCards" }],
  Sections: [{ part: "src/ui/molecules/ZoneCardBand.molecule.part.tsx", state: "Sections" }],
  OneGame: [{ part: "src/ui/molecules/ZoneCardBand.molecule.part.tsx", state: "OneCard" }],
  WideArtOnly: [{ part: "src/ui/atoms/ZoneCoverWell.atom.part.tsx", state: "WideArt" }],
  FortyGames: [{ part: "src/ui/molecules/ZoneCardBand.molecule.part.tsx", state: "FortyCards" }],
  TwoPlaceChoice: [{ part: "src/ui/molecules/ZoneActionRow.molecule.part.tsx", state: "TwoButtons" }],
  BusyOverHome: [{ part: "src/ui/organisms/ZoneStatusLayer.organism.part.tsx", state: "OverPage" }],
  GameOpts: [
    { part: "src/ui/molecules/ZoneOptionGroup.molecule.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneDialog.molecule.part.tsx", state: "FocusedNo" },
  ],
  GameOptsAsking: [{ part: "src/ui/molecules/ZoneDialog.molecule.part.tsx", state: "FocusedNo" }],
  FindOpen: [{ part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "default" }],
  Idle: [{ part: "src/ui/molecules/ZoneTitleCard.molecule.part.tsx", state: "default" }],
  Narrow: [{ part: "src/ui/molecules/ZoneCardBand.molecule.part.tsx", state: "Narrow" }],
  DownloadFocused: [{ part: "src/ui/molecules/ZoneCard.molecule.part.tsx", state: "Download" }],
} as const

type Opening = NonNullable<Parameters<typeof useZoneGames>[3]>

/* The surface's own home program, run against the fixture host and started
 * by `opening`, so the band follows focus, A launches or drops into the row,
 * the keyboard filters and GAME OPTS. asks, as on the device. */
function Games({
  model,
  opening = [],
  status,
}: {
  readonly model: SurfaceModel
  readonly opening?: Opening
  /** Korri's status over the page, as the status layer draws it. */
  readonly status?: SurfaceStatus
}) {
  const root = useRef<HTMLDivElement | null>(null)
  const fixture = useRef<ReturnType<typeof createGamesFixtureHost> | null>(null)
  if (fixture.current === null) fixture.current = createGamesFixtureHost()
  const { page } = useZoneGames(model, fixture.current, root, opening)
  const home = <ZoneGames {...(page satisfies ComponentProps<typeof ZoneGames>)} />
  return (
    <div className="zone-theme zone-screen" ref={root}>
      <ZoneGround />
      {status === undefined ? home : <ZoneStatusLayer page={home} status={status} titleOf={fixtureTitleOf} />}
    </div>
  )
}

/** The twelve games, Hollow Knight focused: one place, PLAY lit, its facts under the band. */
export default function Ready() {
  return <Games model={fixtureReadyModel} opening={[{ _tag: "FocusedCard", index: 1 }]} />
}

/** Korri is still reading the library: three dim stripe wells and LOADING. */
export function Loading() {
  return <Games model={fixtureLoadingModel} />
}

/** Korri could not read the library: its message and RELOAD. */
export function Error() {
  return <Games model={fixtureErrorModel} />
}

/** No games: the band says so; the tabs stay reachable. */
export function Empty() {
  return <Games model={fixtureEmptyModel} />
}

/** Korri's sections: a spine opens each one; the cursor passes over it. */
export function Sections() {
  return <Games model={fixtureSectionsModel} opening={[{ _tag: "FocusedCard", index: 1 }]} />
}

/** One game, centred alone. */
export function OneGame() {
  return <Games model={fixtureOneGameModel} />
}

/** Wario Land 4 has wide art and no cover: the wide art, letterboxed in the cover well, not the stripes. */
export function WideArtOnly() {
  return <Games model={fixtureWideArtModel} opening={[{ _tag: "FocusedCard", index: fixtureWideArtIndex }]} />
}

/** Forty games, the twentieth focused. */
export function FortyGames() {
  return <Games model={fixtureFortyModel} opening={[{ _tag: "FocusedCard", index: 19 }]} />
}

/** Spelunky plays here or on zao: two buttons, none lit until the cursor is on one. */
export function TwoPlaceChoice() {
  return <Games model={fixtureReadyModel} opening={[{ _tag: "FocusedCard", index: 3 }]} />
}

/** Celeste can be continued: CONTINUE, and the CONTINUE tag. */
export function Resumable() {
  return <Games model={fixtureReadyModel} />
}

/**
 * Korri busy over home: the whole page dims under the sheet, FIND and the
 * tabs with it, and the busy strip stands clear above.
 */
export function BusyOverHome() {
  return <Games model={fixtureReadyModel} status={statusBusyDetail} />
}

/**
 * Hollow Knight needs a 1.5 GB download first: the orange DOWNLOAD tag on its
 * card and DOWNLOAD 1.5 GB at the end of its facts. Spelunky, two cards on,
 * needs one of a size Korri does not know: DOWNLOAD alone.
 */
export function DownloadFocused() {
  return <Games model={fixtureDownloadModel} opening={[{ _tag: "FocusedCard", index: 1 }]} />
}

/** options on Spelunky: Korri's actions, the disabled one dimmed with its reason. */
export function GameOpts() {
  return <Games model={fixtureReadyModel} opening={[{ _tag: "FocusedCard", index: 3 }, { _tag: "PressedOptions" }]} />
}

/** DELETE SAVE chosen on GAME OPTS.: the NO / YES question over the sheet, the veil between. */
export function GameOptsAsking() {
  return (
    <Games
      model={fixtureReadyModel}
      opening={[
        { _tag: "FocusedCard", index: 3 },
        { _tag: "PressedOptions" },
        { _tag: "FocusedGameOpt", index: 1 },
        { _tag: "ChoseGameAction", gameId: "spelunky", actionId: "delete-save", enabled: true, destructive: true },
      ]}
    />
  )
}

/** FIND with "o" typed: the keyboard up, the band filtered live behind it. */
export function FindOpen() {
  return (
    <Games
      model={fixtureReadyModel}
      opening={[{ _tag: "ChoseFind" }, { _tag: "PressedKey", key: { kind: "char", char: "o" } }]}
    />
  )
}

/** DONE on "la": the filter kept, FIND: LA at the band's start. */
export function Filtered() {
  return (
    <Games
      model={fixtureReadyModel}
      opening={[
        { _tag: "ChoseFind" },
        { _tag: "PressedKey", key: { kind: "char", char: "l" } },
        { _tag: "PressedKey", key: { kind: "char", char: "a" } },
        { _tag: "PressedKey", key: { kind: "done" } },
      ]}
    />
  )
}

/** Thirty seconds without input: the attract, Spelunky's title card at rest (1.5 s into its 6 s turn). */
export function Idle() {
  useFixtureHold(1500)
  return <Games model={fixtureReadyModel} opening={[{ _tag: "FocusedCard", index: 3 }, { _tag: "IdleElapsed" }]} />
}

/** A 640 by 480 screen: three cards, the short strips. */
export function Narrow() {
  return (
    <ZonePartFrame shape="handheld">
      <Games model={fixtureReadyModel} opening={[{ _tag: "FocusedCard", index: 3 }]} />
    </ZonePartFrame>
  )
}
