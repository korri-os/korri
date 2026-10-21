import { motion } from "framer-motion"
import type { ReactNode, Ref } from "react"
import {
  SHIFT_DESIGN_PARTS,
  shiftDesignPartAttrs,
} from "../../shift-design-parts"
import { ShiftCineTile } from "../molecules/ShiftCineTile"

const SPRING = { type: "spring", stiffness: 260, damping: 32 } as const

/** One game in the rail — the minimum the rail needs to render a tile. */
export interface ShiftCineRailGame {
  readonly id: string
  readonly title: string
  readonly tileArtUrl: string
  readonly tileArtAspectRatio?: string
  /** Discovery/recommended pick — surfaces a "Fresh" marker on the tile. */
  readonly fresh?: boolean
  /** Save mark; a split draws a corner marker, since it blocks the launch. */
  readonly saves?: string
  /** Section this game belongs to; consecutive games sharing a section are
   * grouped under one caption below the group. */
  readonly section?: string
}

interface RailTileRef {
  readonly game: ShiftCineRailGame
  readonly index: number
}
interface RailGroup {
  readonly key: string
  readonly label?: string
  readonly tiles: readonly RailTileRef[]
}

/** Group consecutive games by their `section`, preserving each game's absolute
 * rail index (so focus/centering/image-windowing stay index-driven). Games with
 * no section coalesce into unlabeled groups. */
export function groupRailGames(
  games: readonly ShiftCineRailGame[],
): readonly RailGroup[] {
  const groups: {
    key: string
    label?: string
    tiles: RailTileRef[]
  }[] = []
  games.forEach((game, index) => {
    const last = groups.at(-1)
    if (last && last.label === game.section) {
      last.tiles.push({ game, index })
    } else {
      groups.push({
        key: `${game.section ?? "_"}-${index}`,
        ...(game.section !== undefined ? { label: game.section } : {}),
        tiles: [{ game, index }],
      })
    }
  })
  return groups
}

/**
 * The Switch-style rail: tiles laid in a track that the owner shifts so the
 * focused tile stays centered (`trackX`, spring-animated). The rail is
 * presentational — focus and centering live in the owning screen; it forwards
 * `trackRef` for the centering math and reports tile focus/activation up. An
 * optional `cap` renders after the game tiles for a trailing non-game entry
 * (e.g. the Library affordance); it is a self-contained focusable node, so the
 * rail stays purely presentational.
 *
 * When any game carries a `section`, the rail groups consecutive same-section
 * games under a caption (rendered below the group); otherwise it lays the tiles
 * out flat exactly as before. The `cap` is aligned to the game tiles via a
 * hidden caption-height spacer in sectioned mode.
 */
export function ShiftCineRail({
  games,
  index,
  trackX,
  trackRef,
  imageWindow,
  onTileFocus,
  onTileActivate,
  cap,
}: {
  readonly games: readonly ShiftCineRailGame[]
  readonly index: number
  readonly trackX: number
  readonly trackRef: Ref<HTMLDivElement>
  readonly imageWindow?: { readonly start: number; readonly end: number }
  readonly onTileFocus: (index: number) => void
  readonly onTileActivate: (index: number) => void
  readonly cap?: ReactNode
}) {
  const renderTile = (game: ShiftCineRailGame, i: number) => (
    <ShiftCineTile
      key={game.id}
      index={i}
      gameId={game.id}
      title={game.title}
      artUrl={game.tileArtUrl}
      aspectRatio={game.tileArtAspectRatio}
      fresh={game.fresh}
      split={game.saves === "Split"}
      focused={i === index}
      renderImage={
        !imageWindow || (i >= imageWindow.start && i <= imageWindow.end)
      }
      onFocus={() => onTileFocus(i)}
      onActivate={() => onTileActivate(i)}
    />
  )

  const groups = groupRailGames(games)
  const sectioned = groups.some(group => group.label !== undefined)

  return (
    <div
      className="shift-cine-rail"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.rail)}
    >
      <motion.div
        className="shift-cine-track"
        data-sectioned={sectioned || undefined}
        ref={trackRef}
        animate={{ x: trackX }}
        transition={SPRING}
      >
        {sectioned ? (
          <>
            {groups.map(group => (
              <div
                key={group.key}
                className="shift-cine-rail-group"
                data-active={
                  group.tiles.some(tile => tile.index === index) || undefined
                }
              >
                <div className="shift-cine-rail-group-tiles">
                  {group.tiles.map(tile => renderTile(tile.game, tile.index))}
                </div>
                {group.label ? (
                  <div className="shift-cine-rail-group-label">
                    {group.label}
                  </div>
                ) : (
                  <div
                    className="shift-cine-rail-group-label"
                    data-spacer
                    aria-hidden
                  >
                    {"\u00A0"}
                  </div>
                )}
              </div>
            ))}
            {cap ? (
              <div className="shift-cine-rail-group">
                <div className="shift-cine-rail-group-tiles">{cap}</div>
                <div
                  className="shift-cine-rail-group-label"
                  data-spacer
                  aria-hidden
                >
                  {"\u00A0"}
                </div>
              </div>
            ) : null}
          </>
        ) : (
          <>
            {games.map((game, i) => renderTile(game, i))}
            {cap}
          </>
        )}
      </motion.div>
    </div>
  )
}
