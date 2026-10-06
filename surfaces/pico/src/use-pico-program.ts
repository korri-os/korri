/**
 * The runtime for a Pico program: a model, a pure update, and requests.
 *
 * `dispatch` runs `update` on the latest model and the latest context, stores
 * the result, and hands each request to `perform`. It is stable for the life
 * of the component, so a listener registered once never sees a stale model.
 *
 * This is the whole of the Elm architecture Pico uses. Subscriptions (host
 * buttons, the idle timer) live in the hook that uses this one.
 */
import { useCallback, useRef, useState } from "react"
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
  perform: (request: PicoRequest) => void,
): PicoProgram<Model, Message, Context> {
  const [model, setModel] = useState(init)
  const modelRef = useRef(model)
  const contextRef = useRef(context)
  contextRef.current = context
  const performRef = useRef(perform)
  performRef.current = perform

  const dispatch = useCallback((message: Message) => {
    const step = update(modelRef.current, message, contextRef.current)
    modelRef.current = step.model
    setModel(step.model)
    for (const request of step.requests) performRef.current(request)
  }, [update])

  const latest = useCallback(() => ({ model: modelRef.current, context: contextRef.current }), [])
  return { model, dispatch, latest }
}
