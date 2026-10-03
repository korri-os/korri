/**
 * Boxbuster's root, and the only component that reads the treaty.
 *
 * It turns Korri's model into two views once: the store (the place, rebuilt
 * only when a tape changes place or a drawn fact changes) and the counter
 * (every tape and where a launch stands). It owns the hand, answers Back, and
 * asks Korri to launch, retry, dismiss, and reload. Everything below it speaks
 * in tapes and rooms.
 */
import "./BoxbusterSurface.css"
import type {
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import {
  type CSSProperties,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { BoxbusterCounter } from "./BoxbusterCounter"
import { BoxbusterNotice } from "./BoxbusterNotice"
import { BoxbusterStore } from "./BoxbusterStore"
import { backFrom, type BoxbusterHand, handAfterCatalog } from "./boxbuster-hand"
import { layoutFor } from "./boxbuster-layout"
import {
  counterStatusFrom,
  counterTapesFrom,
  storeSignature,
  storeViewFrom,
} from "./boxbuster-store-view"
import { canDrawStore } from "./boxbuster-webgl"
import { useBoxbusterSize } from "./use-boxbuster-size"

export function BoxbusterSurface({
  model,
  host,
}: {
  model: SurfaceModel
  host: SurfaceHost
}) {
  // Korri republishes the whole model on any change, a clock tick included,
  // so reading the time here re-checks the 14-day window at least once a
  // minute. The signature covers every fact the store view reads, so the
  // store, its textures, and its cover requests rebuild only when a tape
  // changes place or a drawn fact changes.
  const now = Date.now()
  const signature = storeSignature(model, now)
  // `model` and `now` are deliberately absent from the dependencies: the
  // signature stands in for both.
  const view = useMemo(() => storeViewFrom(model, now), [signature])
  // The counter also reads facts the store ignores (launch locations, play
  // counts), so it follows the catalog itself as well as tape placement.
  const tapes = useMemo(
    () => counterTapesFrom(model, now),
    [model.catalog, signature],
  )
  const status = counterStatusFrom(model)

  const [held, setHand] = useState<BoxbusterHand>({ _tag: "NoTape" })
  // A republished catalog may have taken the tape away, or stocked an empty
  // store; settle the hand before anything renders with a stale one.
  const hand = handAfterCatalog(held, tapes)
  if (hand !== held) setHand(hand)

  // The Back handler is registered once; it reads the hand and status as they
  // are when the button is pressed, not as they were when it was registered.
  const latest = useRef({ hand, status })
  latest.current = { hand, status }
  useEffect(
    () =>
      host.input.on("back", () => {
        const outcome = backFrom(latest.current.hand, latest.current.status)
        if (outcome._tag === "Dismiss") host.dismiss()
        else if (outcome._tag === "PutDown") setHand(outcome.hand)
      }),
    [host],
  )

  const surface = useRef<HTMLDivElement>(null)
  const size = useBoxbusterSize(surface)
  const storeDrawable = useMemo(canDrawStore, [])
  const layout = layoutFor(size.width, size.height, storeDrawable)
  const counterSize =
    layout._tag === "Side"
      ? layout.counterWidth
      : layout._tag === "Below"
        ? layout.counterHeight
        : undefined
  // The counter's share is computed from the measured container, so it reaches
  // the stylesheet as a custom property; BoxbusterSurface.css owns the grid.
  const style =
    counterSize === undefined
      ? undefined
      : ({ "--bb-counter-size": `${counterSize}px` } as CSSProperties)

  if (view._tag !== "Open") {
    return (
      <div className="boxbuster-surface" data-boxbuster-surface="" ref={surface}>
        <BoxbusterNotice view={view} onReload={() => host.reload()} />
      </div>
    )
  }

  return (
    <div
      className="boxbuster-surface"
      data-boxbuster-surface=""
      data-layout={layout._tag}
      ref={surface}
      style={style}
    >
      {layout._tag === "CounterOnly" ? null : (
        <div className="boxbuster-surface-store">
          <BoxbusterStore view={view} />
        </div>
      )}
      <BoxbusterCounter
        tapes={tapes}
        hand={hand}
        status={status}
        onLook={tapeId => setHand({ _tag: "Browsing", tapeId })}
        onPickUp={tapeId => setHand({ _tag: "Holding", tapeId })}
        onPutDown={() =>
          setHand(current =>
            current._tag === "Holding"
              ? { _tag: "Browsing", tapeId: current.tapeId }
              : current,
          )
        }
        onPlay={(tapeId, locationId) =>
          locationId === undefined
            ? host.launchGame(tapeId)
            : host.launchGame(tapeId, locationId)
        }
        onRetry={() => host.retry()}
        onDismiss={() => host.dismiss()}
      />
    </div>
  )
}
