import "./BoxbusterNotice.css"
import type { ReactNode } from "react"
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
