/**
 * Boxbuster's root, and the only component that reads the treaty.
 *
 * It turns Korri's model into the store (rebuilt only when a tape changes
 * place or a drawn fact changes), what the visit needs to know about each
 * tape, and what the TV says about a launch. Nothing is laid over the room:
 * the room shows focus, the box carries its own words, the TV prints Korri's. It owns the visit (boxbuster-visit.ts): where you
 * stand, what you hold, what is in the deck. You play a game by carrying its
 * tape to the viewing room and putting it in the deck; that is the only call
 * to `launchGame`. Everything below this file speaks in tapes and rooms.
 */
import "./BoxbusterSurface.css"
import type {
  SurfaceHost,
  SurfaceModel,
} from "@contracts/surface/korri-surface"
import { useEffect, useMemo, useRef, useState } from "react"
import { BoxbusterNotice, BoxbusterWorkNotice, NO_STORE } from "./BoxbusterNotice"
import { type BoxbusterStoreDrawing, webglStore } from "./BoxbusterStore"
import { BoxbusterTargets } from "./BoxbusterTargets"
import { DOOR_SPOT, spotsFrom } from "./boxbuster-spots"
import {
  storeSignature,
  storeViewFrom,
  tapeFactsFrom,
  tvStatusFrom,
} from "./boxbuster-store-view"
import { landingFor, targetsFor } from "./boxbuster-targets"
import {
  closedVisit,
  step,
  type Visit,
  type VisitEvent,
  visitAfterStore,
} from "./boxbuster-visit"
import { placeTapes } from "./tape-placement"
import { useBoxbusterSize } from "./use-boxbuster-size"

export function BoxbusterSurface({
  model,
  host,
  drawing = webglStore,
}: {
  model: SurfaceModel
  host: SurfaceHost
  /** How the store is drawn. Tests draw nothing, so the walk runs without
   * WebGL; a host never passes this. */
  drawing?: BoxbusterStoreDrawing
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
  const store = useMemo(() => {
    if (view._tag !== "Open") return undefined
    const placed = placeTapes(view.map)
    return { map: view.map, placed, spots: spotsFrom(view.map, placed) }
  }, [view])
  // Tape facts also read what the store ignores (launch locations, play
  // counts), so they follow the catalog as well as tape placement.
  const tapes = useMemo(
    () => tapeFactsFrom(model, now),
    [model.catalog, signature],
  )
  const status = tvStatusFrom(model)
  // While Korri works, the decks run its actions. Several actions are a
  // choice between launches, so Korri calls you to the TV to make it.
  const work = model.status._tag === "Busy" ? (model.status.actions ?? []) : undefined
  const choosing = (work?.length ?? 0) > 1

  const [held, setVisit] = useState<Visit>(() => closedVisit({ door: DOOR_SPOT }))
  // A republished catalog may have taken a tape away, rebuilt the store, or
  // stocked it for the first time; settle the visit before anything renders
  // with a stale one.
  const visit =
    store === undefined ? held : visitAfterStore(held, tapes, store.spots)
  if (visit !== held) setVisit(visit)

  // Input handlers are registered once, so they read the visit as it is
  // when the button is pressed, not as it was when they were registered.
  const latest = useRef({ visit, status, store })
  latest.current = { visit, status, store }
  const dispatch = (event: VisitEvent) => {
    const { visit: current, store: built } = latest.current
    if (built === undefined) return
    const stepped = step(current, event, built.spots)
    latest.current = { ...latest.current, visit: stepped.visit }
    setVisit(stepped.visit)
    const command = stepped.command
    if (command === undefined) return
    if (command._tag === "Dismiss") host.dismiss()
    else if (command.locationId === undefined) host.launchGame(command.tapeId)
    else host.launchGame(command.tapeId, command.locationId)
  }
  const dispatchRef = useRef(dispatch)
  dispatchRef.current = dispatch

  useEffect(() => {
    const offBack = host.input.on("back", () =>
      dispatchRef.current({
        _tag: "Back",
        problem: latest.current.status._tag === "Problem",
      }),
    )
    const offOptions = host.input.on("options", () =>
      dispatchRef.current({ _tag: "Turn" }),
    )
    return () => {
      offBack()
      offOptions()
    }
  }, [host])

  // A problem is told on the TV, so it brings you to the TV.
  useEffect(() => {
    if (status._tag === "Problem") dispatchRef.current({ _tag: "ProblemShown" })
  }, [status._tag])

  // A choice is made at the TV too.
  useEffect(() => {
    if (choosing) dispatchRef.current({ _tag: "ChoiceShown" })
  }, [choosing])

  // Which unseen target has focus, so the room can show it.
  const [focused, setFocused] = useState<string | undefined>(undefined)

  const surface = useRef<HTMLDivElement>(null)
  const size = useBoxbusterSize(surface)
  const drawable = useMemo(() => drawing.available(), [drawing])

  // Without a store, Korri's work and its actions still need a place: the door.
  const doorWork =
    model.status._tag === "Busy" && (work?.length ?? 0) > 0 ? (
      <BoxbusterWorkNotice
        kicker={model.status.kicker}
        {...(model.status.detail === undefined ? {} : { detail: model.status.detail })}
        actions={work ?? []}
        onAction={actionId => host.runAction(actionId)}
      />
    ) : null
  if (view._tag !== "Open" || store === undefined) {
    return (
      <div className="boxbuster-surface" data-boxbuster-surface="" ref={surface}>
        {doorWork ?? <BoxbusterNotice view={view} onReload={() => host.reload()} />}
      </div>
    )
  }
  if (!drawable) {
    return (
      <div className="boxbuster-surface" data-boxbuster-surface="" ref={surface}>
        {doorWork ?? <BoxbusterNotice view={NO_STORE} onReload={() => host.reload()} />}
      </div>
    )
  }

  const spot = store.spots.byId.get(visit.spot)
  const targets = targetsFor({
    spots: store.spots,
    visit,
    tapes,
    placed: store.placed,
    width: size.width,
    height: size.height,
    retry: status._tag === "Problem" && status.canRetry,
    ...(work === undefined ? {} : { work }),
  })
  const hand = visit.hand
  const heldId = hand._tag === "Holding" ? hand.tapeId : undefined
  const heldTape =
    heldId === undefined ? undefined : tapes.find(tape => tape.id === heldId)
  const inDeck = store.placed.find(tape => tape.game.id === visit.deck)?.game
  const Draw = drawing.Draw

  return (
    <div
      className="boxbuster-surface"
      data-boxbuster-surface=""
      data-spot={visit.spot}
      ref={surface}
    >
      {spot === undefined ? null : (
        <Draw
          map={store.map}
          placed={store.placed}
          spot={spot}
          width={size.width}
          height={size.height}
          targets={targets}
          {...(focused === undefined ? {} : { focused })}
          tv={status}
          {...(hand._tag === "Holding"
            ? {
                held: {
                  tapeId: hand.tapeId,
                  face: hand.face,
                  pose: hand.pose,
                },
              }
            : {})}
          {...(inDeck === undefined ? {} : { inDeck })}
          {...((work?.length ?? 0) > 0
            ? { deckLabels: (work ?? []).map(action => action.label) }
            : heldTape?.launch._tag === "Choose" &&
                visit.spot === store.spots.viewing
              ? { deckLabels: heldTape.launch.locations.map(l => l.label) }
              : {})}
        />
      )}
      <BoxbusterTargets
        targets={targets}
        landing={landingFor(targets, visit)}
        arrival={[
          visit.spot,
          heldId ?? "",
          visit.deck ?? "",
          status._tag,
          size.width > 0 ? "sized" : "",
        ].join("|")}
        tapes={tapes}
        onFocusChange={setFocused}
        onTape={tapeId => dispatch({ _tag: "PickUp", tapeId })}
        onExit={exit => dispatch({ _tag: "Walk", to: exit.to })}
        onDeck={locationId =>
          dispatch(
            locationId === undefined
              ? { _tag: "Insert" }
              : { _tag: "Insert", locationId },
          )
        }
        onEject={() => dispatch({ _tag: "Eject" })}
        onRetry={() => host.retry()}
        onWork={actionId => host.runAction(actionId)}
      />
    </div>
  )
}
