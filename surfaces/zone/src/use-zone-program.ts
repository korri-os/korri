/**
 * The runtime for a Zone program: a model, a pure update, and requests.
 *
 * The model lives in a store held by a ref and read through
 * `useSyncExternalStore`, so no component keeps React state. `dispatch` runs
 * `update` on the latest model and the latest context, stores the result,
 * notifies React, and hands each request to `perform`. An update that
 * returns the same model renders nothing.
 *
 * `dispatch` is stable for the life of the component, so a listener
 * registered once never sees a stale model.
 */
import { useCallback, useRef, useSyncExternalStore } from "react"
import type { ZoneRequest, ZoneStep } from "./state/requests"

export interface ZoneProgram<Model, Message> {
  readonly model: Model
  readonly dispatch: (message: Message) => void
}

interface Store<Model> {
  model: Model
  readonly listeners: Set<() => void>
}

export function useZoneProgram<Model, Message, Context>(
  init: () => Model,
  update: (model: Model, message: Message, context: Context) => ZoneStep<Model>,
  context: Context,
  perform: (request: ZoneRequest) => void,
): ZoneProgram<Model, Message> {
  const store = useRef<Store<Model> | null>(null)
  if (store.current === null) store.current = { model: init(), listeners: new Set() }
  const contextRef = useRef(context)
  contextRef.current = context
  const performRef = useRef(perform)
  performRef.current = perform

  const subscribe = useCallback((listener: () => void) => {
    const current = store.current!
    current.listeners.add(listener)
    return () => {
      current.listeners.delete(listener)
    }
  }, [])
  const getSnapshot = useCallback(() => store.current!.model, [])
  const model = useSyncExternalStore(subscribe, getSnapshot, getSnapshot)

  const dispatch = useCallback((message: Message) => {
    const current = store.current!
    const step = update(current.model, message, contextRef.current)
    if (step.model !== current.model) {
      current.model = step.model
      for (const listener of [...current.listeners]) listener()
    }
    for (const request of step.requests) performRef.current(request)
  }, [update])

  return { model, dispatch }
}
