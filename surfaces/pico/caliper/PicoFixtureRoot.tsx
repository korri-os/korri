import { useEffect, useRef, useState, type ReactNode } from "react"
import type { SurfaceInputAction } from "../../../contracts/surface/korri-surface"
import { PicoSurface } from "../src/PicoSurface"
import { createPreviewSession } from "./fixture-session"
import { readFixtureSeed } from "./page-seed"

export interface FixtureRootHost {
  reseed(value: unknown): void
  dispose(): void
}
export interface FixtureRootProps {
  readonly initialValues?: unknown
  readonly scopeId?: string
  readonly onHost?: (host: FixtureRootHost) => void
  readonly children?: ReactNode
}
export type RegisterFixture = (scopeId: string | undefined, session: ReturnType<typeof createPreviewSession>) => () => void

const keys: Readonly<Record<string, SurfaceInputAction>> = {
  Escape: "back", f: "options", m: "menu", s: "system",
}

/** The same live subtree for a device and a placed page. Model reseeding never
 * reconstructs Pico's controllers; only an explicit entry-view edit restarts
 * local navigation. Caliper cannot bypass the production interaction logic. */
export function PicoFixtureRoot({ initialValues, scopeId, onHost, register, keyboardTarget }: FixtureRootProps & {
  readonly register: RegisterFixture
  readonly keyboardTarget?: HTMLElement
}) {
  const seed = useRef(readFixtureSeed(initialValues)).current
  const [entry, setEntry] = useState(seed.entry)
  const [session] = useState(() => createPreviewSession(next => setModel(next), seed.source, seed.entry?._tag === "Overlay"))
  const [model, setModel] = useState(session.model)
  const node = useRef<HTMLDivElement>(null)
  const dispose = useRef(() => {})
  useEffect(() => {
    const off = register(scopeId, session)
    const target = keyboardTarget ?? node.current!
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.repeat || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return
      if (event.target instanceof Element && event.target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return
      const action = keys[event.key]
      if (!action) return
      event.preventDefault()
      event.stopPropagation()
      session.host.press(action)
    }
    target.addEventListener("keydown", onKey)
    const cleanup = () => { off(); target.removeEventListener("keydown", onKey) }
    dispose.current = cleanup
    return cleanup
  }, [register, scopeId, session, keyboardTarget])
  useEffect(() => {
    onHost?.({
      reseed(value) {
        const next = readFixtureSeed(value)
        setEntry(next.entry)
        session.select(next.source)
      },
      dispose() { dispose.current() },
    })
  }, [onHost, session])
  return (
    <div ref={node} className="pico-caliper-scope pico-caliper-mount" tabIndex={0}
      aria-label="Pico fixture: F Find, M layout, S settings, Escape Back">
      <PicoSurface key={JSON.stringify(entry)} host={session.host} initialView={entry} model={model} />
    </div>
  )
}
