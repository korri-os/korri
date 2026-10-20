import { useRef, useSyncExternalStore } from "react"
import { fixtureRoutes, fixtureRunnerWarnings } from "../../fixtures/fixture-options"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import type { SurfaceRunnerRoute } from "@contracts/surface/korri-surface"
import { ZoneRunWithRoute } from "./ZoneRunWithRoute"

export const name = "Run with route"
export const note = "One installed route: program and exact build, SAVED when a saved choice picks it, its warnings and its actions"

export const composition = {
  default: [
    { part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneActionRow.molecule.part.tsx", state: "ThreeButtons" },
  ],
  Saved: [{ part: "src/ui/molecules/ZoneTagRow.molecule.part.tsx", state: "TwoTags" }],
  Warnings: [{ part: "src/ui/organisms/ZoneWarningLines.organism.part.tsx", state: "default" }],
} as const

/** A ref-backed cursor so the preview follows focus; the product keeps it in src/state/run-with.ts. */
function createCursor() {
  let focused: string | undefined
  const listeners = new Set<() => void>()
  return {
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    get: () => focused,
    focus(id: string) {
      focused = id
      for (const listener of listeners) listener()
    },
  }
}

/** Unexported: the route with a live cursor. */
function Live({ route, saved = [] }: { readonly route: SurfaceRunnerRoute; readonly saved?: readonly string[] }) {
  const cursor = useRef<ReturnType<typeof createCursor> | null>(null)
  cursor.current ??= createCursor()
  const live = cursor.current
  const focused = useSyncExternalStore(live.subscribe, live.get)
  return (
    <ZoneRunWithRoute
      focused={focused}
      onAction={() => undefined}
      onFocus={live.focus}
      route={route}
      rowId={`route:${route.runnerId}`}
      saved={saved}
    />
  )
}

const first = fixtureRoutes[0]!
const warned = fixtureRunnerWarnings._tag === "Closed" ? first : fixtureRunnerWarnings.routes[0]!

/** A route and its three actions. */
export default function Default() {
  return <Live route={first} />
}

/** The route a saved choice picks: SAVED and the choice's name over it. */
export function Saved() {
  return <Live route={first} saved={["This game"]} />
}

/** Korri's warning for the route, in red under the row. */
export function Warnings() {
  return <Live route={warned} />
}

/** An action Korri publishes inert: dimmed, its reason under it, still reachable. */
export function DisabledAction() {
  return (
    <Live
      route={{
        ...first,
        actions: [
          ...first.actions.slice(0, 1),
          { id: "runner:3:game:0", label: "Remember for this game", enabled: false, description: "Stop the game first" },
        ],
      }}
    />
  )
}

/** A narrow column: the build wraps, the actions stack. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Live route={first} saved={["This game", "System gba"]} />
    </ZonePartFrame>
  )
}
