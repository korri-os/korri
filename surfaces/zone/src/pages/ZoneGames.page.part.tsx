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
  fixtureGetGamesEmptyModel,
  fixtureGetGamesModel,
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
  FindOpenNarrow: [{ part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "default" }],
  FindOpenStrip: [{ part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "default" }],
  Idle: [{ part: "src/ui/molecules/ZoneTitleCard.molecule.part.tsx", state: "default" }],
  Narrow: [{ part: "src/ui/molecules/ZoneCardBand.molecule.part.tsx", state: "Narrow" }],
  DownloadFocused: [{ part: "src/ui/molecules/ZoneCard.molecule.part.tsx", state: "Download" }],
  GetGames: [{ part: "src/ui/atoms/ZoneButton.atom.part.tsx", state: "default" }],
  GetGamesEmpty: [{ part: "src/ui/molecules/ZoneCardBand.molecule.part.tsx", state: "NoCards" }],
} as const

/* The helpers Caliper hands an authored check, typed locally: Zone imports
 * nothing from Caliper (test/authoring-gate.test.ts). */
interface CheckTools {
  readonly canvas: { getByRole(role: string, options: { name: RegExp }): HTMLElement }
  readonly expect: (value: unknown) => { toBe(expected: unknown): void }
  readonly waitFor: (assertion: () => void) => Promise<void>
}

/** The art pixel an element stands on, in CSS px. */
const artPxOf = (element: Element): number =>
  Number.parseFloat(getComputedStyle(element).getPropertyValue("--zone-px")) || 1

/** The boxes of an element's own words (its text nodes' line boxes). */
function wordBoxes(element: Element): DOMRect[] {
  const boxes: DOMRect[] = []
  const walk = document.createTreeWalker(element, NodeFilter.SHOW_TEXT)
  for (let node = walk.nextNode(); node !== null; node = walk.nextNode()) {
    if ((node.textContent ?? "").trim() === "") continue
    const range = document.createRange()
    range.selectNodeContents(node)
    boxes.push(...Array.from(range.getClientRects()).filter(box => box.width > 0))
  }
  return boxes
}

/** Where a key's baseline stands: a zero-size probe on it, put back at once. */
function baselineOf(key: HTMLElement): number {
  const probe = document.createElement("i")
  probe.style.cssText = "display:inline-block;width:0;height:0;vertical-align:baseline"
  key.append(probe)
  const at = probe.getBoundingClientRect().bottom
  probe.remove()
  return at
}

/**
 * Every key whose label is not whole, read off the real keyboard in
 * Chromium, and why. A label is whole when its letters (the cap above the
 * baseline, a descender under it, and the 1-art-px drop) stand inside its
 * own key and the key stands inside the panel, unscrolled, on the screen.
 */
function keysNotWhole(keyboard: HTMLElement): string {
  const px = artPxOf(keyboard)
  const panel = keyboard.getBoundingClientRect()
  const screen = { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight }
  const problems: string[] = []
  if (keyboard.scrollHeight > keyboard.clientHeight + 1) problems.push(`panel scrolls ${keyboard.scrollHeight}/${keyboard.clientHeight}`)
  const keys = Array.from(keyboard.querySelectorAll<HTMLElement>(".zone-keyboard-key"))
  for (const key of keys) {
    const name = key.textContent ?? ""
    const box = key.getBoundingClientRect()
    const words = wordBoxes(key)
    if (words.length === 0) {
      problems.push(`${name}: no label`)
      continue
    }
    const fact = key.dataset["face"] === "fact"
    const descends = fact ? /[gjpqy]/.test(name) : /[,;Q]/.test(name)
    const baseline = baselineOf(key)
    const ink = {
      left: Math.min(...words.map(word => word.left)),
      right: Math.max(...words.map(word => word.right)),
      top: baseline - 10 * px,
      bottom: baseline + (descends ? 3 * px : 0) + px,
    }
    const inside = (outer: { left: number; top: number; right: number; bottom: number }) =>
      ink.left >= outer.left - 0.5 && ink.right <= outer.right + 0.5 && ink.top >= outer.top - 0.5 && ink.bottom <= outer.bottom + 0.5
    if (!inside(box)) problems.push(`${name}: past its key (${Math.round(ink.left - box.left)},${Math.round(ink.top - box.top)} to ${Math.round(box.right - ink.right)},${Math.round(box.bottom - ink.bottom)})`)
    else if (!inside(panel)) problems.push(`${name}: past the panel`)
    else if (!inside(screen)) problems.push(`${name}: off the screen`)
  }
  return problems.join("; ")
}

/**
 * The focused card's title stands inside its card, or (when a word or its
 * lines have no room) is not drawn on it and stands whole on the facts
 * line instead.
 */
function focusedTitleSpill(): string {
  const card = document.querySelector<HTMLElement>(".zone-card[data-focused=\"true\"]")
  if (card === null) return ""
  const title = card.querySelector<HTMLElement>(".zone-card-title")
  if (title === null) return ""
  const box = card.getBoundingClientRect()
  const words = wordBoxes(title)
  // A title the card does not draw (no room, or the band hidden under the keys) cannot spill.
  if (words.length === 0 || title.getBoundingClientRect().height === 0 || !title.checkVisibility({ visibilityProperty: true })) return ""
  return words.some(word => word.left < box.left - 0.5 || word.right > box.right + 0.5 || word.top < box.top - 0.5 || word.bottom > box.bottom + 0.5)
    ? `${title.textContent ?? ""} spills out of its card`
    : ""
}

/**
 * GET GAMES' key and its cursor's triangles stand clear of the focused
 * card, and both triangles stand on the screen.
 */
function getGamesClear(): string {
  const face = document.querySelector(".zone-games-get .zone-button-face")
  const card = document.querySelector(".zone-card[data-focused=\"true\"]")
  if (face === null) return "no GET GAMES"
  const parts = [face, ...Array.from(document.querySelectorAll(".zone-games-get [class^='zone-button-cursor']"))]
  const problems: string[] = []
  for (const part of parts) {
    const box = part.getBoundingClientRect()
    if (box.left < -0.5 || box.right > window.innerWidth + 0.5) problems.push(`${part.className} off the screen`)
    if (card === null) continue
    const at = card.getBoundingClientRect()
    const meets = box.left < at.right && box.right > at.left && box.top < at.bottom && box.bottom > at.top
    if (meets) problems.push(`${part.className} stands on the focused card`)
  }
  return problems.join("; ")
}

/** The keys' check: every label whole, the panel unscrolled. */
async function keysWhole({ canvas, expect, waitFor }: CheckTools): Promise<void> {
  await waitFor(() => {
    expect(keysNotWhole(canvas.getByRole("group", { name: /Keyboard/ }))).toBe("")
  })
}

/** The card's check: the focused title inside its card, or not drawn there. */
async function titleInCard({ expect, waitFor }: CheckTools): Promise<void> {
  await waitFor(() => {
    expect(focusedTitleSpill()).toBe("")
  })
}

/* FIND's keyboard is planned from the body, so at every size its keys
 * stand whole and unscrolled; the band yields first, then the keys' room.
 * The focused card's title never spills out of its card, at rest or with
 * the band shrunk under the keys. */
export const checks = {
  GetGames: {
    "GET GAMES and its cursor stand clear of the focused card, on the screen": async ({ expect, waitFor }: CheckTools) => {
      await waitFor(() => {
        expect(getGamesClear()).toBe("")
      })
    },
  },
  default: {
    "the focused card's title stands inside its card": (tools: CheckTools) => titleInCard(tools),
  },
  Narrow: {
    "the focused card's title stands inside its card": (tools: CheckTools) => titleInCard(tools),
  },
  FindOpen: {
    "every key's label is whole: no key cut, none scrolled away": (tools: CheckTools) => keysWhole(tools),
    "the focused card's title stands inside its card": (tools: CheckTools) => titleInCard(tools),
  },
  FindOpenNarrow: {
    "every key's label is whole: no key cut, none scrolled away": (tools: CheckTools) => keysWhole(tools),
    "the focused card's title stands inside its card": (tools: CheckTools) => titleInCard(tools),
  },
  FindOpenStrip: {
    "every key's label is whole: no key cut, none scrolled away": (tools: CheckTools) => keysWhole(tools),
    "the focused card's title stands inside its card": (tools: CheckTools) => titleInCard(tools),
  },
}

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

/** FIND in a column 480 wide: the rows wrap, the band yields to the keys. */
export function FindOpenNarrow() {
  return (
    <ZonePartFrame shape="w480">
      <FindOpen />
    </ZonePartFrame>
  )
}

/** FIND on a strip 300 tall: one CSS px an art px, the band kept over the keys. */
export function FindOpenStrip() {
  return (
    <ZonePartFrame shape="strip-300">
      <FindOpen />
    </ZonePartFrame>
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

/** Korri can get games: GET GAMES in the body's top left, the cursor on it. */
export function GetGames() {
  return <Games model={fixtureGetGamesModel} opening={[{ _tag: "FocusedCard", index: 1 }, { _tag: "FocusedGetGames" }]} />
}

/** An empty library where Korri can get games: GET GAMES has the cursor. */
export function GetGamesEmpty() {
  return <Games model={fixtureGetGamesEmptyModel} />
}

/** GET GAMES with a kept filter: FIND: LA stands under it. */
export function GetGamesFiltered() {
  return (
    <Games
      model={fixtureGetGamesModel}
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
