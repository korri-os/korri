import "./ZoneStatusLayer.css"
import type { SurfaceStatus } from "@contracts/surface/korri-surface"
import { type ReactNode, useRef } from "react"
import { useZoneLayerFocus } from "../../use-zone-layer-focus"
import { useZoneStatusHud } from "../../use-zone-status-hud"
import { ZoneCaps } from "../atoms/ZoneCaps"
import { ZonePanel } from "../atoms/ZonePanel"
import { ZoneTag } from "../atoms/ZoneTag"
import { ZoneVeil } from "../atoms/ZoneVeil"
import { ZoneWords } from "../atoms/ZoneWords"
import { ZoneBusyStrip } from "../molecules/ZoneBusyStrip"
import { ZoneDialog } from "../molecules/ZoneDialog"

/** The problem dialog's two ways out, left to right. */
export type ZoneStatusProblemButton = "dismiss" | "retry"

/** Where the busy strip stands: across the page's middle, or along its foot (under a title card). */
export type ZoneStatusBusyAt = "middle" | "foot"

/**
 * Korri's status over any page. The page is drawn under it; the layer
 * decides what covers it.
 *
 *  - Browsing: nothing. The page is the page.
 *  - Busy (C7): the page goes inert under the veil, a Mega Drive dither
 *    of black, and the busy strip stands over it with Korri's kicker and
 *    detail, the game it belongs to in the HUD's yellow on a black tab,
 *    and Korri's actions (CANCEL) beside it. With Korri's progress (C32)
 *    the strip's meter shows how far the work has got. Focus moves into
 *    the strip when it has an action.
 *  - Running (C8): a HUD stack at the top left, under the page's header,
 *    as the in-game HUD stands: the kicker in the HUD's yellow, then the
 *    game's title on black with an orange OPEN tag. It takes no pointer and
 *    no focus and leaves the page alone.
 *  - Problem (C9): the sky dialog over the veiled page: the kicker as
 *    headline, the reason in Korri's words, the game's title in yellow,
 *    DISMISS and RETRY (when Korri can) with DISMISS lit first.
 *  - Confirm (C31): the same dialog asks Korri's question: the kicker as
 *    headline, the detail under it, the game's title in yellow when
 *    `gameId` names one, and one button per host action in the host's
 *    order, labels as given, the first lit. No button is read as yes or no
 *    by its label, and Back answers nothing: Korri's actions are the only
 *    ways out.
 *
 * `stage` is a picture that stands over the page and under the status's
 * own panels: the launch title card. While it stands the busy strip goes
 * to its foot (`busyAt`) and no veil lies over it, so the card is seen
 * whole while Korri works; a problem or a question veils it as it veils a
 * page, and covers it.
 *
 * Every word is Korri's except DISMISS, RETRY and OPEN. `focus` is the
 * model's lit button; focus returns to the page when the layer closes.
 * RELOAD is not here: Korri's Problem never asks for it (C10 lives on the
 * catalog's Error dialog on GAMES).
 */
export function ZoneStatusLayer({
  status,
  titleOf,
  focus = 0,
  page,
  stage,
  busyAt,
  runningHud = true,
  onFocusButton,
  onBusyAction,
  onProblemButton,
  onConfirmAction,
}: {
  readonly status: SurfaceStatus
  /** The catalog's title for a game id; undefined when the catalog has none. */
  readonly titleOf: (gameId: string) => string | undefined
  /** The lit button: a busy action's index, or 0 DISMISS and 1 RETRY. */
  readonly focus?: number
  /** The page the layer covers. */
  readonly page?: ReactNode
  /** A picture over the page and under the status's panels: the launch title card. */
  readonly stage?: ReactNode
  /** Where the busy strip stands. Default: the foot under a stage, else the middle. */
  readonly busyAt?: ZoneStatusBusyAt
  /** Draw the running HUD. Off where the page already names the game (PAUSED). */
  readonly runningHud?: boolean
  readonly onFocusButton?: (index: number) => void
  readonly onBusyAction?: (actionId: string) => void
  readonly onProblemButton?: (button: ZoneStatusProblemButton) => void
  /** A on one of Korri's answers to its question: the action's id. */
  readonly onConfirmAction?: (actionId: string) => void
}) {
  const root = useRef<HTMLDivElement | null>(null)
  const layer = useRef<HTMLDivElement | null>(null)
  const holdsFocus = (status._tag === "Busy" && (status.actions?.length ?? 0) > 0) || status._tag === "Problem"
    || (status._tag === "Confirm" && status.actions.length > 0)
  useZoneLayerFocus(layer, holdsFocus ? `${status._tag}:${focus}` : undefined, true)
  const hud = status._tag === "Running" && runningHud
  useZoneStatusHud(root, hud)

  const covers = status._tag === "Busy" || status._tag === "Problem" || status._tag === "Confirm"
  const gameTitle = status._tag === "Problem"
    ? status.gameTitle ?? (status.gameId === undefined ? undefined : titleOf(status.gameId))
    : status._tag === "Browsing" || status.gameId === undefined ? undefined : titleOf(status.gameId)

  return (
    <div className="zone-status-layer" data-status={status._tag} ref={root}>
      <div className="zone-status-layer-page" data-covered={covers || undefined} inert={covers}>
        <div className="zone-status-layer-content">{page}</div>
        {/* Busy veils the page, never the stage; a problem's or a question's
            dialog lays its own veil over both. */}
        {status._tag === "Busy" && stage === undefined ? <ZoneVeil /> : null}
      </div>
      {stage === undefined ? null : <div className="zone-status-layer-stage" inert={covers}>{stage}</div>}
      {status._tag === "Running" && hud ? (
        <div aria-live="polite" className="zone-status-layer-running" role="status">
          <span className="zone-status-layer-hud-kicker">
            <ZoneCaps ink="hud" size="caps-1">{status.kicker}</ZoneCaps>
          </span>
          <span className="zone-status-layer-hud-line">
            {gameTitle === undefined ? null : (
              <span className="zone-status-layer-hud-title">
                <ZonePanel edge={false}>
                  <ZoneCaps size="caps-1">{gameTitle}</ZoneCaps>
                </ZonePanel>
              </span>
            )}
            <span className="zone-status-layer-running-tag">
              <ZoneTag kind="open" label="Open" />
            </span>
          </span>
        </div>
      ) : null}
      {status._tag === "Busy" ? (
        <div className="zone-status-layer-busy" data-at={busyAt ?? (stage === undefined ? "middle" : "foot")} data-block-exit ref={layer}>
          {gameTitle === undefined ? null : (
            <span className="zone-status-layer-busy-game">
              <ZonePanel edge={false}>
                <ZoneWords ink="hud">{gameTitle}</ZoneWords>
              </ZonePanel>
            </span>
          )}
          <ZoneBusyStrip
            actions={status.actions}
            detail={status.detail}
            focused={(status.actions?.length ?? 0) > 0 ? focus : undefined}
            kicker={status.kicker}
            onAction={onBusyAction}
            progress={status.progress}
            onFocusAction={onFocusButton}
          />
        </div>
      ) : null}
      {status._tag === "Problem" ? (
        <div className="zone-status-layer-problem" data-block-exit ref={layer}>
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
            onFocusAction={onFocusButton}
            title={gameTitle}
          />
        </div>
      ) : null}
      {status._tag === "Confirm" ? (
        <div className="zone-status-layer-confirm" data-block-exit ref={layer}>
          <ZoneDialog
            actions={status.actions.map(action => ({
              id: action.id,
              label: action.label,
              disabled: !action.enabled,
              destructive: action.destructive,
              onPress: onConfirmAction === undefined ? undefined : () => onConfirmAction(action.id),
            }))}
            focused={focus}
            headline={status.kicker}
            lines={status.detail === undefined ? [] : [status.detail]}
            onFocusAction={onFocusButton}
            title={gameTitle}
          />
        </div>
      ) : null}
    </div>
  )
}
