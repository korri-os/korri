import "./PicoInviteNotices.css"
import { useEffect, useLayoutEffect, useRef } from "react"
import { type PicoInviteNoticeView, picoNoticeCorner } from "../../pico-invite-view"
import { PicoInviteNotice } from "../molecules/PicoInviteNotice"

/**
 * The invites announced now, newest first, stacked in a corner of the
 * screen over whatever it shows: any screen, question or cover.
 *
 * Nothing here can be pressed or focused, and a pointer goes through it to
 * the screen below, so a notice never blocks the person's way. It never
 * hides the cursor either: the stack stands in the top left corner, and
 * moves to the first other corner that keeps the focused control in sight
 * whenever the cursor moves, or the screen or the stack changes size
 * (`picoNoticeCorner`). Where the stack stands is a measurement, not
 * state: it is written to the DOM as `data-corner`, as `data-cut` and
 * `data-ratio` are, and CSS reads it.
 *
 * The stack may cover a share of the screen and no more: a notice that does
 * not fit whole is not drawn, oldest first. Each invite is still on home,
 * under PLAYING NOW, for as long as it stands.
 */
export function PicoInviteNotices({ notices }: { readonly notices: readonly PicoInviteNoticeView[] }) {
  const stack = useRef<HTMLDivElement>(null)

  const place = () => {
    const root = stack.current
    if (root === null || root.childElementCount === 0) return
    const frame = root.offsetParent ?? root.parentElement
    if (frame === null) return
    const box = root.getBoundingClientRect()
    const screen = frame.getBoundingClientRect()
    const inset = Math.max(0, Math.min(box.left - screen.left, screen.right - box.right, box.top - screen.top, screen.bottom - box.bottom))
    const active = root.ownerDocument.activeElement
    const focused = active instanceof HTMLElement && active !== root.ownerDocument.body && frame.contains(active) && !root.contains(active)
      ? active.getBoundingClientRect()
      : undefined
    const corner = picoNoticeCorner(screen, { width: box.width, height: box.height }, inset, focused)
    if (root.dataset.corner !== corner) root.dataset.corner = corner
  }

  // After every drawing: a notice came or went, or the screen under it changed.
  useLayoutEffect(place)

  // The cursor moves with no drawing of ours, and the screen can change size:
  // the window, or only the box the host gives Pico, or the stack itself.
  // biome-ignore lint/correctness/useExhaustiveDependencies: place reads only the DOM.
  useEffect(() => {
    const root = stack.current
    const view = root?.ownerDocument.defaultView
    const frame = root?.offsetParent ?? root?.parentElement
    const Observer = view?.ResizeObserver
    const sizes = Observer === undefined ? undefined : new Observer(() => place())
    if (root !== null && root !== undefined) sizes?.observe(root)
    if (frame !== null && frame !== undefined) sizes?.observe(frame)
    document.addEventListener("focusin", place)
    view?.addEventListener("resize", place)
    return () => {
      sizes?.disconnect()
      document.removeEventListener("focusin", place)
      view?.removeEventListener("resize", place)
    }
  }, [])

  return (
    <div aria-live="polite" className="pico-invite-notices" ref={stack} role="status">
      {notices.map(notice => (
        <PicoInviteNotice
          deviceLabel={notice.deviceLabel}
          gameTitle={notice.gameTitle}
          key={notice.id}
          personName={notice.personName}
        />
      ))}
    </div>
  )
}
