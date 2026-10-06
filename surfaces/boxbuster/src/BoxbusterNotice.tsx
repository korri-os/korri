import "./BoxbusterNotice.css"
import { type ReactNode, useLayoutEffect, useRef } from "react"
import type { BoxbusterStoreView } from "./boxbuster-store-view"

/** The store cannot be drawn here: this browser has no WebGL. */
export const NO_STORE = { _tag: "NoStore" } as const

/**
 * What the door says when there is no store to enter. Renders nothing once the
 * store is open.
 */
export function BoxbusterNotice({
  view,
  onReload,
}: {
  view: BoxbusterStoreView | typeof NO_STORE
  onReload: () => void
}) {
  switch (view._tag) {
    case "Open":
      return null
    case "NoStore":
      // You play a game by carrying its tape to the deck. With no store to
      // walk, there is no way to play here, and the sign says so plainly.
      return (
        <BoxbusterNoticeBoard
          sign="The store cannot open on this device"
          detail="Boxbuster draws the store in 3D, and this device's browser has no 3D graphics."
        />
      )
    case "Loading":
      return <BoxbusterNoticeBoard sign="Opening the store…" />
    case "Empty":
      return (
        <BoxbusterNoticeBoard
          sign="The shelves are empty"
          detail="Games Korri finds on this device or its peers are shelved here."
        />
      )
    case "Error":
      return (
        <BoxbusterNoticeBoard sign="The store could not open" detail={view.message}>
          <button
            type="button"
            className="boxbuster-notice-action"
            // The only control on screen: focus it so one confirm retries.
            autoFocus
            onClick={onReload}
          >
            Try again
          </button>
        </BoxbusterNoticeBoard>
      )
  }
}

/**
 * What the door says while Korri works and there is no store to walk into.
 * Korri's actions for that work (such as Cancel) are still on the door: the
 * deck is out of reach, so the door holds them, in Korri's words.
 */
/** One of Korri's actions as the door shows it: its words and whether it acts. */
export interface DoorAction {
  readonly id: string
  readonly label: string
  readonly description?: string
  readonly enabled: boolean
}

export function BoxbusterWorkNotice({
  kicker,
  detail,
  actions,
  onAction,
}: {
  kicker: string
  detail?: string
  actions: readonly DoorAction[]
  onAction: (actionId: string) => void
}) {
  const list = useRef<HTMLDivElement>(null)
  // Focus is where the host's confirm lands. Seed it, and move it to a control
  // that still acts when the focused one goes away or goes inert.
  useLayoutEffect(() => {
    const root = list.current
    if (root === null) return
    const active = root.ownerDocument.activeElement
    const lost = active === null || active === root.ownerDocument.body ||
      !active.isConnected || (root.contains(active) && active.matches(":disabled"))
    if (lost) root.querySelector<HTMLButtonElement>("button:not([disabled])")?.focus()
  })
  return (
    <BoxbusterNoticeBoard sign={kicker} {...(detail === undefined ? {} : { detail })}>
      <div className="boxbuster-notice-actions" ref={list}>
        {actions.map(action => (
          <div className="boxbuster-notice-work" key={action.id}>
            <button
              type="button"
              className="boxbuster-notice-action"
              disabled={!action.enabled}
              onClick={() => onAction(action.id)}
            >
              {action.label}
            </button>
            {action.description === undefined ? null : (
              <p className="boxbuster-notice-detail">{action.description}</p>
            )}
          </div>
        ))}
      </div>
    </BoxbusterNoticeBoard>
  )
}

function BoxbusterNoticeBoard({
  sign,
  detail,
  children,
}: {
  sign: string
  detail?: string
  children?: ReactNode
}) {
  return (
    <div className="boxbuster-notice" role="status">
      <div className="boxbuster-notice-board">
        <h1 className="boxbuster-notice-sign">{sign}</h1>
        {detail === undefined ? null : (
          <p className="boxbuster-notice-detail">{detail}</p>
        )}
        {children}
      </div>
    </div>
  )
}
