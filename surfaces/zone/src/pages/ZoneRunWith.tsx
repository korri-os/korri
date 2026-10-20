import "./ZoneRunWith.css"
import type { SurfaceRunnerChoice } from "@contracts/surface/korri-surface"
import { useEffect, useRef } from "react"
import { runWithPlan, type ZoneRunWithState } from "../plan/options-run-with"
import { ZoneBanner } from "../ui/atoms/ZoneBanner"
import { ZoneCaps } from "../ui/atoms/ZoneCaps"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import { ZonePanel } from "../ui/atoms/ZonePanel"
import { ZoneTag, type ZoneTagKind } from "../ui/atoms/ZoneTag"
import { ZoneActionRow } from "../ui/molecules/ZoneActionRow"
import { ZoneFoot } from "../ui/molecules/ZoneFoot"
import { ZoneOptionRow } from "../ui/molecules/ZoneOptionRow"
import { ZoneRunWithRoute } from "../ui/organisms/ZoneRunWithRoute"
import { ZoneWarningLines } from "../ui/organisms/ZoneWarningLines"
import { ZoneShell, type ZoneBand, type ZoneStrips } from "../ui/templates/ZoneShell"

/** Each state of Korri's runner choice wears a tag colour Zone already uses
 * for that kind of news: mint for settled, yellow for at work, orange for
 * needs a look, red for stopped, white for not yet. */
const STATE_TAGS: Record<ZoneRunWithState, ZoneTagKind> = {
  Loading: "continue",
  Ready: "saved",
  Stale: "open",
  Warnings: "open",
  Busy: "here",
  Conflict: "remote",
  Error: "remote",
}

/**
 * RUN WITH, reached from GAME OPTS. for one game: which installed runner
 * plays it. The banner names the page, the game's title stands beside it,
 * and Korri's state for the choice is a tag at the end of that row. Under
 * them: Korri's warnings for the whole choice in red, its message as one
 * caps line, one row per route (program, then the exact build) with SAVED
 * on the route a saved choice picks, and Korri's actions as buttons: each
 * route's under it, the choice's own at the bottom.
 *
 * Every id goes back unchanged through `onAction`; the host bound it to
 * this read. Closed renders nothing. The page traps the cursor in its rows
 * (`data-block-exit`) and the way out is the host's own action.
 *
 * A shape: Korri's runner choice and the focused id come in as props;
 * focus and presses go out through callbacks. When Korri's state changes
 * and the cursor is not on the page, it lands on the first control.
 */
export function ZoneRunWith({
  choice,
  focused,
  strips,
  band,
  clock,
  build,
  onFocus,
  onAction,
}: {
  readonly choice: SurfaceRunnerChoice
  /** The route row id or action id the cursor is on. */
  readonly focused?: string
  readonly strips?: ZoneStrips
  readonly band?: ZoneBand
  readonly clock?: string
  readonly build?: string
  readonly onFocus?: (id: string) => void
  readonly onAction?: (actionId: string) => void
}) {
  const list = useRef<HTMLDivElement | null>(null)
  const tag = choice._tag
  useEffect(() => {
    const box = list.current
    if (box === null || box.contains(document.activeElement)) return
    box.querySelector<HTMLButtonElement>("button")?.focus({ preventScroll: true })
  }, [tag])

  if (choice._tag === "Closed") return null
  const plan = runWithPlan(choice)
  const run = onAction ?? (() => undefined)

  return (
    <ZoneShell
      band={band}
      foot={clock === undefined && build === undefined ? undefined : <ZoneFoot build={build} clock={clock} />}
      hintBack={<ZoneKeycap button="B" word="Back" />}
      hintConfirm={<ZoneKeycap button="A" word="Confirm" />}
      strips={strips}
    >
      <div className="zone-run-with" data-state={choice._tag}>
        <div className="zone-run-with-head">
          <ZoneBanner>Run with</ZoneBanner>
          <span className="zone-run-with-title">
            <ZonePanel>
              <ZoneCaps>{choice.gameTitle}</ZoneCaps>
            </ZonePanel>
          </span>
          <span className="zone-run-with-state">
            <ZoneTag kind={STATE_TAGS[choice._tag]} label={choice._tag} />
          </span>
        </div>
        <div className="zone-run-with-scroll" data-block-exit="true" ref={list}>
          <div className="zone-run-with-rows">
            <ZoneWarningLines lines={choice.warnings} />
            {choice.message === "" ? null : <p className="zone-run-with-message">{choice.message}</p>}
            {plan.orphans.map(orphan => (
              <ZoneOptionRow
                key={`${orphan.label}:${orphan.runnerId}`}
                label={`Saved: ${orphan.label}`}
                value={orphan.runnerId}
              />
            ))}
            {plan.routes.map(entry => (
              <ZoneRunWithRoute
                focused={focused}
                key={entry.id}
                onAction={run}
                onFocus={onFocus}
                route={entry.route}
                rowId={entry.id}
                saved={entry.saved}
              />
            ))}
            {choice.actions.length === 0 ? null : (
              <div className="zone-run-with-actions">
                <ZoneActionRow
                  actions={choice.actions.map(action => ({
                    id: action.id,
                    label: action.label,
                    ...(action.enabled ? {} : { disabled: true }),
                    ...(action.description === undefined ? {} : { reason: action.description }),
                  }))}
                  focused={focused}
                  onFocus={onFocus}
                  onPress={run}
                />
              </div>
            )}
          </div>
        </div>
      </div>
    </ZoneShell>
  )
}
