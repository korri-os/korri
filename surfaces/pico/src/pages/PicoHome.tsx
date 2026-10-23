import "./PicoHome.css"
import { picoStatsFor } from "../pico-detail-view"
import type { PicoInviteNoticeView, PicoInviteView } from "../pico-invite-view"
import { picoCollectionsFrom, picoHeroPick } from "../pico-library-view"
import type { PicoScreenView } from "../pico-screen-view"
import type { PicoShelfGame } from "../pico-shelf-game"
import { PicoNotice } from "../ui/molecules/PicoNotice"
import { PicoCartGrid } from "../ui/organisms/PicoCartGrid"
import { PicoCartShelf } from "../ui/organisms/PicoCartShelf"
import { PicoGameHero } from "../ui/organisms/PicoGameHero"
import { PicoInviteNotices } from "../ui/organisms/PicoInviteNotices"
import { PicoPlayingNow } from "../ui/organisms/PicoPlayingNow"
import { PicoResumeList } from "../ui/organisms/PicoResumeList"
import { PicoLaunchStage } from "../ui/organisms/PicoLaunchStage"
import { PicoLocationPicker } from "../ui/organisms/PicoLocationPicker"
import { PicoMenu } from "../ui/organisms/PicoMenu"
import { PicoScreenShell } from "../ui/templates/PicoScreenShell"

/** Hints for a shelf the user can act on. */
const SHELF_HINTS = [
  { hintKey: "a", label: "PLAY" },
  { hintKey: "b", label: "BACK" },
] as const

/** With nothing to act on, the only honest hint left is the way out. */
const QUIET_HINTS = [{ hintKey: "b", label: "BACK" }] as const

/** Nothing to press and no way out until Korri is done. */
const NO_HINTS = [] as const

const MENU_LABEL = "MENU"

/* The layout's own word, for the VIEW line in the menu. */
const VIEW_WORDS: Record<PicoHomeMode, string> = {
  shelf: "SHELF",
  grid: "GRID",
  hero: "HERO",
}

/* The mode is in the header rather than a badge of its own: the user is
 * already reading that line to know where they are. */
const MODE_LABELS: Record<PicoHomeMode, string> = {
  shelf: "LIBRARY",
  grid: "LIBRARY · GRID",
  hero: "LIBRARY · HERO",
}

/**
 * How home lays the library out. Cycled by the treaty's `menu` button and by
 * VIEW in the MENU list.
 */
export type PicoHomeMode = "shelf" | "grid" | "hero"

/**
 * Home's MENU key and its list: FIND, GET GAMES (where Korri offers a store),
 * SETTINGS and VIEW. FIND, SETTINGS and VIEW are also reached by the treaty's
 * `options`, `system` and `menu` buttons also reach. Whether the list is open
 * belongs to the owner, because Back closes it and Back arrives through the
 * host.
 */
export interface PicoHomeMenu {
  readonly open: boolean
  /** Put the cursor on MENU as home appears: Back from a screen MENU opened. */
  readonly returning: boolean
  readonly onToggle: () => void
  readonly onFind: () => void
  /** GET GAMES: present only where Korri can get games from providers. */
  readonly onStore?: () => void
  readonly onSettings: () => void
  readonly onView: () => void
  readonly onReturned: () => void
  /** The name of the focused MENU key or menu line, for the A hint. */
  readonly aim: string | undefined
  readonly onAim: (label: string | undefined) => void
}

/** "9 carts · 2 resumable": what the shelf holds, in the footer's quiet ink. */
function shelfReadout(games: readonly PicoShelfGame[]): string {
  const resumable = games.filter((game) => game.resumable === true).length
  const carts = `${games.length} ${games.length === 1 ? "CART" : "CARTS"}`
  return resumable === 0 ? carts : `${carts} · ${resumable} RESUMABLE`
}

/**
 * Pico's home screen.
 *
 * Every state it can be in — reading the library, empty, failed to read,
 * showing the shelf, asking where to play, starting a game, running one,
 * failing to start one, paused or ended on another device's stream — shares
 * one frame and differs only in the body, so the header and footer never
 * jump. Which state is showing was decided upstream; this file renders the
 * answer and does not re-derive it.
 *
 * Over the library, PLAYING NOW lists the invites Korri publishes, above
 * whichever layout is chosen; with none it is not there. The library sits
 * in the same element either way, so an invite that comes or goes never
 * redraws the shelf or moves its cursor. Invite notices float over the
 * whole frame and take nothing from it.
 *
 * Failure copy is Korri's own, passed through untouched: the surface cannot
 * tell a missing file from an unreachable host, and guessing would put a wrong
 * explanation in front of the user.
 */
export function PicoHome({
  view,
  mode,
  menu,
  placing,
  selectedGameId,
  returnToCart = false,
  onReturnedToCart,
  onSelectGame,
  onOpenGame,
  onChooseLocation,
  onRetry,
  onDismiss,
  onAction,
  invites = [],
  onInviteAction,
  onLostInviteFocus,
  notices = [],
  clockLabel,
}: {
  readonly view: PicoScreenView
  readonly mode: PicoHomeMode
  /** With no menu, home offers no MENU key. */
  readonly menu?: PicoHomeMenu
  /** The game whose launch location is being chosen, when one is. */
  readonly placing?: PicoShelfGame
  /** The shelf's chosen cart, kept by the owner so it survives home leaving. */
  readonly selectedGameId: string | undefined
  /** Put the cursor back on the chosen cart when the shelf appears. */
  readonly returnToCart?: boolean
  readonly onReturnedToCart?: () => void
  readonly onSelectGame: (gameId: string) => void
  /** Selecting a cart opens the game's own screen; launching happens there. */
  readonly onOpenGame: (gameId: string) => void
  readonly onChooseLocation: (locationId: string) => void
  readonly onRetry: () => void
  readonly onDismiss: () => void
  readonly onAction?: (actionId: string) => void
  /** Games playing now on the person's other devices, as PLAYING NOW lists them. */
  readonly invites?: readonly PicoInviteView[]
  readonly onInviteAction?: (actionId: string) => void
  /** The cursor was in PLAYING NOW and its last row went. */
  readonly onLostInviteFocus?: () => void
  /** Invites announced now, newest first. */
  readonly notices?: readonly PicoInviteNoticeView[]
  readonly clockLabel?: string
}) {
  const asking = placing !== undefined && view._tag === "Shelf"
  const aim = menu?.aim
  /* MENU stands wherever home is the library: on the shelf in every layout,
   * and while the library is read, empty or unreadable, when Settings is most
   * needed. Not over a launch, a running game or a question. */
  const library = view._tag === "Shelf" || view._tag === "Loading" || view._tag === "Empty" || view._tag === "Failed"
  const offered = menu !== undefined && !asking && library
  const open = offered && menu.open
  /* Work with nothing to press (unpacking, stopping) takes no Back either:
   * the host delivers Back only as an action Korri published. No hint, then.
   * A paused stream is the same: with Korri's actions (Leave) the host takes
   * Back as its own Leave, as it takes Back as Cancel on work; Pico runs
   * nothing on Back and shows the hint. */
  const hints = open
    ? [
        ...(aim === undefined || aim === MENU_LABEL ? [] : [{ hintKey: "a" as const, label: aim }]),
        { hintKey: "b" as const, label: "CLOSE" },
      ]
    : offered && aim === MENU_LABEL
      ? [{ hintKey: "a" as const, label: MENU_LABEL }, { hintKey: "b" as const, label: "BACK" }]
      : view._tag === "Shelf" && !asking ? SHELF_HINTS
        : (view._tag === "Busy" && (view.actions?.length ?? 0) === 0) || (view._tag === "Paused" && view.actions.length === 0)
          ? NO_HINTS : QUIET_HINTS

  return (
    <PicoScreenShell
      clockLabel={clockLabel}
      hints={hints}
      label={MODE_LABELS[mode]}
      lead={offered ? (
        <PicoMenu
          claimFocus={menu.returning}
          entries={[
            { label: "FIND", onPress: menu.onFind },
            ...(menu.onStore === undefined ? [] : [{ label: "GET GAMES", onPress: menu.onStore }]),
            { label: "SETTINGS", onPress: menu.onSettings },
            { detail: `◀ ${VIEW_WORDS[mode]} ▶`, label: "VIEW", onPress: menu.onView },
          ]}
          label={MENU_LABEL}
          onAim={menu.onAim}
          onFocusClaimed={menu.onReturned}
          onToggle={menu.onToggle}
          open={open}
        />
      ) : undefined}
      over={<PicoInviteNotices notices={notices} />}
      readout={view._tag === "Shelf" ? shelfReadout(view.games) : undefined}
    >
      {asking && placing !== undefined ? (
        <PicoLocationPicker
          locations={placing.locations ?? []}
          onChoose={onChooseLocation}
          title={placing.title}
        />
      ) : null}

      {library && !asking ? (
        <div className="pico-home-library">
          <div className="pico-home-invites">
            <PicoPlayingNow
              invites={invites}
              onAction={actionId => onInviteAction?.(actionId)}
              onLostFocus={onLostInviteFocus}
            />
          </div>
          <div className="pico-home-view">
            {view._tag === "Shelf" && mode === "shelf" ? (
              <PicoCartShelf
                claimFocus={returnToCart}
                games={view.games}
                onFocusClaimed={onReturnedToCart}
                onOpen={onOpenGame}
                onSelect={onSelectGame}
                selectedId={selectedGameId}
              />
            ) : null}

            {view._tag === "Shelf" && mode === "hero" ? (
              <div className="pico-home-hero">
                {(() => {
                  const pick = picoHeroPick(view.games)
                  if (pick === undefined) return null
                  return (
                    <PicoGameHero
                      game={pick.game}
                      onOpen={() => onOpenGame(pick.game.id)}
                      reason={pick.reason}
                      stats={picoStatsFor(pick.game)}
                    />
                  )
                })()}
                <PicoResumeList
                  games={view.games.filter((game) => game.resumable === true)}
                  onOpen={onOpenGame}
                />
              </div>
            ) : null}

            {view._tag === "Shelf" && mode === "grid" ? (
              <PicoCartGrid
                collections={picoCollectionsFrom(view.games)}
                onOpen={onOpenGame}
              />
            ) : null}

            {view._tag === "Loading" ? (
              <PicoNotice
                kicker="READING CARTS"
                message="Korri is looking through your library."
                tone="info"
              />
            ) : null}

            {view._tag === "Empty" ? (
              <PicoNotice
                kicker="NO CARTS"
                message="Nothing to play yet. Add games to your library and they appear here."
                tone="info"
              />
            ) : null}

            {view._tag === "Failed" ? (
              <PicoNotice
                actions={[{ label: "TRY AGAIN", onPress: onRetry }]}
                kicker="SHELF JAMMED"
                message={view.message}
                tone="warn"
              />
            ) : null}
          </div>
        </div>
      ) : null}

      {view._tag === "Confirm" ? (
        <PicoNotice
          actions={view.actions.map(action => ({
            label: action.label,
            disabled: !action.enabled,
            onPress: () => onAction?.(action.id),
          }))}
          kicker={view.kicker}
          message={view.detail}
          title={view.cart?.title}
          tone="ask"
        />
      ) : null}

      {view._tag === "Busy" ? (
        <PicoLaunchStage
          cart={view.cart}
          detail={view.detail}
          kicker={view.kicker}
          progress={view.progress}
          actions={view.actions?.map(action => ({
            id: action.id,
            label: action.label,
            ...(action.description === undefined ? {} : { detail: action.description }),
            disabled: !action.enabled,
            onPress: () => onAction?.(action.id),
          }))}
        />
      ) : null}

      {view._tag === "Running" ? (
        <PicoLaunchStage
          cart={view.cart}
          gameTitle={view.gameTitle}
          kicker={view.kicker}
          phase="running"
        />
      ) : null}

      {/* Retry and Cancel, as the treaty names them: Cancel leaves the failed
        * thing, and the host stops whatever it was still doing for it. With
        * nothing to retry there is nothing to cancel, only to acknowledge. */}
      {view._tag === "Problem" ? (
        <PicoNotice
          actions={
            view.canRetry
              ? [
                  { label: "RETRY", onPress: onRetry },
                  { label: "CANCEL", onPress: onDismiss },
                ]
              : [{ label: "OK", onPress: onDismiss }]
          }
          kicker={view.kicker}
          message={view.reason}
          title={view.gameTitle}
          tone="warn"
        />
      ) : null}

      {/* Stopped on the device that holds the session: the cart rests in its
        * slot, with Korri's words and Korri's actions. */}
      {view._tag === "Paused" ? (
        <PicoLaunchStage
          actions={view.actions.map(action => ({
            id: action.id,
            label: action.label,
            ...(action.description === undefined ? {} : { detail: action.description }),
            disabled: !action.enabled,
            onPress: () => onAction?.(action.id),
          }))}
          cart={view.cart}
          detail={view.detail}
          kicker={view.kicker}
          phase="paused"
        />
      ) : null}

      {/* A session this device played on is over. It tells, in blue, with
        * Korri's reason; OK acknowledges it and home comes back. */}
      {view._tag === "Ended" ? (
        <PicoNotice
          actions={[{ label: "OK", onPress: onDismiss }]}
          message={view.reason}
          kicker={view.kicker}
          tone="info"
        />
      ) : null}
    </PicoScreenShell>
  )
}
