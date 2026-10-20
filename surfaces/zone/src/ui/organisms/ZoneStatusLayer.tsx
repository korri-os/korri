import "./ZoneStatusLayer.css"
import "../../zone-motion.css"
import type { SurfaceStatus } from "@contracts/surface/korri-surface"
import { type FocusEvent, type ReactNode, useRef } from "react"
import { useZoneLayerFocus } from "../../use-zone-layer-focus"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZonePanel } from "../atoms/ZonePanel"
import { ZoneTag } from "../atoms/ZoneTag"
import { ZoneBusyStrip } from "../molecules/ZoneBusyStrip"
import { ZoneDialog } from "../molecules/ZoneDialog"

/** The problem dialog's two ways out, left to right. */
export type ZoneStatusProblemButton = "dismiss" | "retry"

/** The index of the button that took DOM focus among the layer's buttons. */
function focusedIndex(event: FocusEvent<HTMLElement>): number | undefined {
  const buttons = [...event.currentTarget.querySelectorAll("button")]
  const at = buttons.indexOf(event.target as HTMLButtonElement)
  return at === -1 ? undefined : at
}

/**
 * Korri's status over any page. The page is drawn under it; the layer
 * decides what covers it.
 *
 *  - Browsing: nothing. The page is the page.
 *  - Busy (C7): the busy strip drops over the page's middle with Korri's
 *    kicker and detail, the game it belongs to in yellow above it, and
 *    Korri's actions (CANCEL) beside it. The page dims and goes inert;
 *    focus moves into the strip when it has an action.
 *  - Running (C8): a small black parallelogram at the top of the page:
 *    the kicker, an OPEN tag and the game's title. It takes no focus and
 *    leaves the page alone.
 *  - Problem (C9): Mania's blue dialog over the dimmed page: the kicker as
 *    headline, the reason under it, the game's title in yellow, DISMISS and
 *    RETRY (when Korri can) with DISMISS lit first.
 *
 * Every word is Korri's except DISMISS, RETRY and OPEN. `focus` is the
 * model's lit button; focus returns to the page when the layer closes.
 * RELOAD is not here: Korri's Problem never asks for it (C10 lives on the
 * catalog's Error state and the SYSTEM group).
 */
export function ZoneStatusLayer({
  status,
  titleOf,
  focus = 0,
  page,
  onFocusButton,
  onBusyAction,
  onProblemButton,
}: {
  readonly status: SurfaceStatus
  /** The catalog's title for a game id; undefined when the catalog has none. */
  readonly titleOf: (gameId: string) => string | undefined
  /** The lit button: a busy action's index, or 0 DISMISS and 1 RETRY. */
  readonly focus?: number
  /** The page the layer covers. */
  readonly page?: ReactNode
  readonly onFocusButton?: (index: number) => void
  readonly onBusyAction?: (actionId: string) => void
  readonly onProblemButton?: (button: ZoneStatusProblemButton) => void
}) {
  const layer = useRef<HTMLDivElement | null>(null)
  const holdsFocus = (status._tag === "Busy" && (status.actions?.length ?? 0) > 0) || status._tag === "Problem"
  useZoneLayerFocus(layer, holdsFocus ? `${status._tag}:${focus}` : undefined, true)

  const covers = status._tag === "Busy" || status._tag === "Problem"
  const gameTitle = status._tag === "Problem"
    ? status.gameTitle ?? (status.gameId === undefined ? undefined : titleOf(status.gameId))
    : status._tag === "Browsing" || status.gameId === undefined ? undefined : titleOf(status.gameId)
  const onFocus = onFocusButton === undefined ? undefined : (event: FocusEvent<HTMLElement>) => {
    const index = focusedIndex(event)
    if (index !== undefined) onFocusButton(index)
  }

  return (
    <div className="zone-status-layer" data-status={status._tag}>
      <div className="zone-status-layer-page" data-covered={covers || undefined} inert={covers}>
        {page}
      </div>
      {status._tag === "Running" ? (
        <div aria-live="polite" className="zone-status-layer-running" role="status">
          <ZonePanel>
            <span className="zone-status-layer-running-line">
              <ZoneCaps>{status.kicker}</ZoneCaps>
              <span className="zone-status-layer-running-tag">
                <ZoneTag kind="open" label="Open" />
              </span>
              {gameTitle === undefined ? null : <span className="zone-status-layer-game">{gameTitle}</span>}
            </span>
          </ZonePanel>
        </div>
      ) : null}
      {status._tag === "Busy" ? (
        <div className="zone-status-layer-busy" data-block-exit onFocus={onFocus} ref={layer}>
          {gameTitle === undefined ? null : (
            <span className="zone-status-layer-busy-game">
              <ZonePanel edge={false}>
                <span className="zone-status-layer-game">{gameTitle}</span>
              </ZonePanel>
            </span>
          )}
          <ZoneBusyStrip
            actions={status.actions}
            detail={status.detail}
            focused={(status.actions?.length ?? 0) > 0 ? focus : undefined}
            kicker={status.kicker}
            onAction={onBusyAction}
          />
        </div>
      ) : null}
      {status._tag === "Problem" ? (
        <div className="zone-status-layer-problem" data-block-exit onFocus={onFocus} ref={layer}>
          <ZoneDialog
            actions={[
              { label: "Dismiss", onPress: onProblemButton === undefined ? undefined : () => onProblemButton("dismiss") },
              ...(status.canRetry
                ? [{ label: "Retry", onPress: onProblemButton === undefined ? undefined : () => onProblemButton("retry") }]
                : []),
            ]}
            focused={focus}
            headline={status.kicker}
            lines={[status.reason]}
            title={gameTitle}
          />
        </div>
      ) : null}
    </div>
  )
}
