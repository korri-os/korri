import "./ZoneRunWithRoute.css"
import type { SurfaceRunnerRoute } from "@contracts/surface/korri-surface"
import { ZoneActionRow } from "../molecules/ZoneActionRow"
import { ZoneOptionRow } from "../molecules/ZoneOptionRow"
import { ZoneTagRow } from "../molecules/ZoneTagRow"
import { ZoneWarningLines } from "./ZoneWarningLines"

/**
 * One installed route on the RUN WITH page: the program as the row's label
 * and the exact runner build as its value, SAVED and the saved choice's name
 * over it when a saved choice picks it, Korri's warnings for it in red, and
 * its actions as buttons under it.
 *
 * The row and every action are native buttons; the host moves focus and the
 * route reports it (`onFocus`) by id: the row's own id, or an action's id.
 * A disabled action dims and stays reachable, as ZoneButton draws it.
 */
export function ZoneRunWithRoute({
  rowId,
  route,
  saved,
  focused,
  onFocus,
  onAction,
}: {
  /** This route's row id, reported when the row itself takes focus. */
  readonly rowId: string
  readonly route: SurfaceRunnerRoute
  /** The saved choices that pick this route, by their names. */
  readonly saved: readonly string[]
  /** The row id or action id the cursor is on. */
  readonly focused?: string
  readonly onFocus?: (id: string) => void
  readonly onAction: (actionId: string) => void
}) {
  return (
    <section aria-label={route.program} className="zone-run-with-route" data-saved={saved.length > 0 || undefined}>
      {saved.length === 0 ? null : (
        <span className="zone-run-with-route-tags">
          <ZoneTagRow
            tags={[{ kind: "saved", label: "Saved" }, ...saved.map(label => ({ kind: "system" as const, label }))]}
          />
        </span>
      )}
      <ZoneOptionRow
        focused={focused === rowId}
        label={route.program}
        onFocus={onFocus === undefined ? undefined : () => onFocus(rowId)}
        value={route.runnerBuild}
      />
      <ZoneWarningLines lines={route.warnings} />
      {route.actions.length === 0 ? null : (
        <ZoneActionRow
          actions={route.actions.map(action => ({
            id: action.id,
            label: action.label,
            ...(action.enabled ? {} : { disabled: true }),
            ...(action.description === undefined ? {} : { reason: action.description }),
          }))}
          focused={focused}
          onFocus={onFocus}
          onPress={onAction}
        />
      )}
    </section>
  )
}
