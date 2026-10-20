import "./PicoStoreEntry.css"
import type { SurfaceAction, SurfaceStoreDetails, SurfaceStoreGet } from "@contracts/surface/korri-surface"
import { useLayoutEffect, useRef } from "react"
import { PicoButton } from "../atoms/PicoButton"
import { PicoNotice, type PicoNoticeAction } from "../molecules/PicoNotice"
import { PicoGameStage } from "./PicoGameStage"

/**
 * One entry a provider offers, on the same stage as a game's own screen: its
 * cartridge (or initials), its name and where it comes from, the provider's
 * description, and where getting it stands. The words and actions under it
 * are Korri's. Cards say what they are by colour: yellow asks (download),
 * red warns (failed, stopped), blue tells (not available, in your library). Work under way takes the
 * whole screen on the launch stage, so it is not drawn here.
 */
export function PicoStoreEntry({
  details,
  providerTitle,
  onAction,
  onPlay,
}: {
  readonly details: Exclude<SurfaceStoreDetails, { readonly _tag: "Closed" }>
  readonly providerTitle?: string
  readonly onAction: (actionId: string) => void
  readonly onPlay: (gameId: string) => void
}) {
  const root = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => {
    const scope = root.current
    if (scope === null) return
    const active = scope.ownerDocument.activeElement
    if (active !== null && active !== scope.ownerDocument.body && active.isConnected && !active.matches(":disabled")) {
      // Korri's card under the facts grows as the state moves on (a Play
      // arrives, words change): keep the control the cursor is on in view.
      if (scope.contains(active)) active.scrollIntoView?.({ block: "nearest" })
      return
    }
    scope.querySelector<HTMLButtonElement>("button:not([disabled])")?.focus()
  })
  const entry = details.entry
  const subtitle = [entry.system, providerTitle].filter(part => part !== undefined).join(" · ")
  const rows = (actions: readonly SurfaceAction[]): PicoNoticeAction[] =>
    actions.map(action => ({ label: action.label, disabled: !action.enabled, onPress: () => onAction(action.id) }))
  return (
    <div
      className="pico-store-entry"
      ref={root}
      // A card lands with a short step animation; once it has, the control
      // the cursor is on may sit lower than when it was focused.
      onAnimationEnd={event => {
        const active = event.currentTarget.ownerDocument.activeElement
        if (active !== null && event.currentTarget.contains(active)) active.scrollIntoView?.({ block: "nearest" })
      }}
    >
      <PicoGameStage
        id={entry.id}
        stats={[]}
        statsEmptyLabel={details._tag === "Ready" && details.get._tag === "InLibrary"
          ? "ON THIS DEVICE"
          : "NOT ON THIS DEVICE"}
        subtitle={subtitle === "" ? undefined : subtitle}
        title={entry.title}
      >
        {details._tag === "Ready" && details.description !== undefined ? (
          <p className="pico-store-entry-description">{details.description}</p>
        ) : null}
        {details._tag === "Loading" ? (
          <PicoNotice kicker="READING" message={`Asking for ${entry.title}.`} tone="info" />
        ) : details._tag === "Problem" ? (
          <PicoNotice actions={rows(details.actions)} kicker="NO ANSWER" message={details.message} tone="warn" />
        ) : (
          entryGet(details.get, rows, onAction, onPlay)
        )}
      </PicoGameStage>
    </div>
  )

}

/** Where getting the entry stands, drawn in Korri's words. A helper, not a
 * component: it has no part of its own and holds nothing. */
function entryGet(
  get: SurfaceStoreGet,
  rows: (actions: readonly SurfaceAction[]) => PicoNoticeAction[],
  onAction: (actionId: string) => void,
  onPlay: (gameId: string) => void,
) {
    switch (get._tag) {
      case "Offer":
        return (
          <div className="pico-store-entry-get">
            {get.actions.map(action => (
              <PicoButton key={action.id} label={action.label.toUpperCase()} onPress={() => onAction(action.id)} />
            ))}
            {get.note === undefined ? null : <p className="pico-store-entry-note">{get.note}</p>}
          </div>
        )
      case "Unavailable":
        return <PicoNotice kicker="NOT AVAILABLE" message={get.reason} tone="info" />
      case "Confirm":
        return <PicoNotice actions={rows(get.actions)} kicker={get.kicker} message={get.detail} tone="ask" />
      case "Working":
        return <PicoNotice actions={rows(get.actions)} kicker={get.kicker} message={get.detail} tone="info" />
      case "InLibrary":
        return (
          <PicoNotice
            actions={get.gameId === undefined ? [] : [{ label: "▶ PLAY", onPress: () => onPlay(get.gameId ?? "") }]}
            kicker="IN YOUR LIBRARY"
            message={get.gameId === undefined ? "It is on its way to your shelf." : undefined}
            tone="info"
          />
        )
      case "Problem":
        return <PicoNotice actions={rows(get.actions)} kicker={get.kicker} message={get.reason} tone="warn" />
    }
}
