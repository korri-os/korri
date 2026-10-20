/**
 * Shift store — where getting one entry stands (organism).
 *
 * Legacy Shift's store detail put Get, Getting… and Play under the entry, the
 * ungetable guard where Get would be, and the staged notice under the
 * actions. This draws the treaty's `SurfaceStoreGet` the same way: Korri's
 * words, Korri's actions in Korri's order, and a real bar while it downloads.
 * Play is Shift's own verb: it launches the catalog game Korri named.
 */
import type { SurfaceAction, SurfaceStoreGet } from "@contracts/surface/korri-surface"
import { SHIFT_DESIGN_PARTS, shiftDesignPartAttrs } from "../shift-design-parts"
import { ShiftDetailButton } from "./ShiftDetailButton"
import { ShiftStoreProgress } from "./ShiftStoreProgress"

export interface ShiftStoreGetProps {
  readonly get: SurfaceStoreGet
  readonly onAction: (actionId: string) => void
  readonly onPlay: (gameId: string) => void
}

/** The labels the A hint reads: what confirming the first control does. */
export function shiftStoreGetPrimaryLabel(get: SurfaceStoreGet): string | undefined {
  switch (get._tag) {
    case "Offer":
    case "Confirm":
    case "Working":
    case "Problem":
      return get.actions.find(action => action.enabled)?.label
    case "InLibrary":
      return get.gameId === undefined ? undefined : "Play"
    case "Unavailable":
    case "Staged":
      return undefined
  }
}

function Actions({ actions, onAction }: { readonly actions: readonly SurfaceAction[]; readonly onAction: (id: string) => void }) {
  if (actions.length === 0) return null
  return (
    <div className="shift-detail-actions" {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.detailActions)}>
      {actions.map((action, index) => (
        <ShiftDetailButton
          key={action.id}
          primary={index === 0}
          label={action.label}
          {...(action.enabled ? { onClick: () => onAction(action.id) } : {})}
        />
      ))}
    </div>
  )
}

export function ShiftStoreGet({ get, onAction, onPlay }: ShiftStoreGetProps) {
  return (
    <section
      className="shift-store-get"
      data-shift-store-get={get._tag}
      aria-live="polite"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.storeGet)}
    >
      {get._tag === "Offer" ? (
        <>
          <Actions actions={get.actions} onAction={onAction} />
          {get.note === undefined ? null : <p className="shift-store-get-note">{get.note}</p>}
        </>
      ) : get._tag === "Unavailable" ? (
        <p className="shift-store-get-note" data-tone="blocked">{get.reason}</p>
      ) : get._tag === "Confirm" ? (
        <>
          <p className="shift-store-get-kicker">{get.kicker}</p>
          {get.detail === undefined ? null : <p className="shift-store-get-note">{get.detail}</p>}
          <Actions actions={get.actions} onAction={onAction} />
        </>
      ) : get._tag === "Working" ? (
        <>
          <p className="shift-store-get-kicker">{get.kicker}</p>
          {get.progress === undefined ? null : (
            <ShiftStoreProgress
              label={get.kicker}
              done={get.progress.done}
              {...(get.progress.total === undefined ? {} : { total: get.progress.total })}
            />
          )}
          {get.detail === undefined ? null : <p className="shift-store-get-note">{get.detail}</p>}
          <Actions actions={get.actions} onAction={onAction} />
        </>
      ) : get._tag === "InLibrary" ? (
        <>
          {get.gameId === undefined ? null : (
            <div className="shift-detail-actions" {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.detailActions)}>
              <ShiftDetailButton primary label="▶ Play" onClick={() => onPlay(get.gameId ?? "")} />
            </div>
          )}
          <p className="shift-store-get-note" data-tone="done">{get.detail ?? "In your library"}</p>
        </>
      ) : get._tag === "Staged" ? (
        <p className="shift-store-get-note">{get.message}</p>
      ) : (
        <>
          <p className="shift-store-get-kicker" data-tone="failed">{get.kicker}</p>
          <p className="shift-store-get-note">{get.reason}</p>
          <Actions actions={get.actions} onAction={onAction} />
        </>
      )}
    </section>
  )
}
