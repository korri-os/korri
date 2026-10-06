/**
 * The runtime for a Pico program: a model, a pure update, and requests.
 *
 * `dispatch` runs `update` on the latest model and the latest context, stores
 * the result, and hands each request to `perform`. A request that answers (a
 * wait, a drawn QR code) returns a promise of a message, and its reply is
 * dispatched like any other message, unless the component has gone. That is
 * Elm's `Cmd` with a result.
 *
 * `dispatch` is stable for the life of the component, so a listener
 * registered once never sees a stale model. An update that returns the same
 * model renders nothing.
 */
import { useCallback, useEffect, useRef, useState } from "react"
import type { PicoRequest, PicoStep } from "./state/requests"

export interface PicoProgram<Model, Message, Context> {
  readonly model: Model
  readonly dispatch: (message: Message) => void
  /** The model and context as of now, for a DOM handler that must decide
   * before React renders again. */
  readonly latest: () => { readonly model: Model; readonly context: Context }
}

export function usePicoProgram<Model, Message, Context>(
  init: () => Model,
  update: (model: Model, message: Message, context: Context) => PicoStep<Model>,
  context: Context,
  perform: (request: PicoRequest) => Promise<Message> | void,
): PicoProgram<Model, Message, Context> {
  const [model, setModel] = useState(init)
  const modelRef = useRef(model)
  const contextRef = useRef(context)
  contextRef.current = context
  const performRef = useRef(perform)
  performRef.current = perform
  const alive = useRef(true)
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])

  const dispatch = useCallback((message: Message) => {
    if (!alive.current) return
    const step = update(modelRef.current, message, contextRef.current)
    if (step.model !== modelRef.current) {
      modelRef.current = step.model
      setModel(step.model)
    }
    for (const request of step.requests) {
      const reply = performRef.current(request)
      if (reply !== undefined) void reply.then(dispatch)
    }
  }, [update])

  const latest = useCallback(() => ({ model: modelRef.current, context: contextRef.current }), [])
  return { model, dispatch, latest }
}
