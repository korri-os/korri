import "./BoxbusterNotice.css"
import type { ReactNode } from "react"
import type { BoxbusterStoreView } from "./boxbuster-store-view"

/**
 * What the door says when there is no store to enter. Renders nothing once the
 * store is open.
 */
export function BoxbusterNotice({
  view,
  onReload,
}: {
  view: BoxbusterStoreView
  onReload: () => void
}) {
  switch (view._tag) {
    case "Open":
      return null
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
