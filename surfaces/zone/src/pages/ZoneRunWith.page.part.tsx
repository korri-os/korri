import type { SurfaceRunnerChoice } from "@contracts/surface/korri-surface"
import { useRef, useSyncExternalStore } from "react"
import {
  createRunWithProgram,
  fixtureRunnerBusy,
  fixtureRunnerConflict,
  fixtureRunnerError,
  fixtureRunnerLoading,
  fixtureRunnerManyRoutes,
  fixtureRunnerOneRoute,
  fixtureRunnerReady,
  fixtureRunnerSaved,
  fixtureRunnerStale,
  fixtureRunnerWarnings,
  runWithCallbacks,
} from "../fixtures/fixture-options"
import { ZonePartFrame } from "../fixtures/ZonePartFrame"
import { plan } from "../plan/zone-select"
import { ZoneGround } from "../ui/organisms/ZoneGround"
import { ZoneRunWith } from "./ZoneRunWith"

export const name = "Run with"
export const note = "Which installed runner plays one game: Korri's state as a tag, its routes, warnings and actions"

export const composition = {
  default: [
    { part: "src/ui/templates/ZoneShell.template.part.tsx", state: "default" },
    { part: "src/ui/organisms/ZoneRunWithRoute.organism.part.tsx", state: "default" },
    { part: "src/ui/atoms/ZoneTag.atom.part.tsx", state: "Saved" },
  ],
  Saved: [{ part: "src/ui/organisms/ZoneRunWithRoute.organism.part.tsx", state: "Saved" }],
  Warnings: [
    { part: "src/ui/organisms/ZoneRunWithRoute.organism.part.tsx", state: "Warnings" },
    { part: "src/ui/organisms/ZoneWarningLines.organism.part.tsx", state: "default" },
  ],
  Conflict: [{ part: "src/ui/atoms/ZoneTag.atom.part.tsx", state: "Remote" }],
  Error: [{ part: "src/ui/atoms/ZoneTag.atom.part.tsx", state: "Remote" }],
  Busy: [{ part: "src/ui/atoms/ZoneTag.atom.part.tsx", state: "Here" }],
  Loading: [{ part: "src/ui/atoms/ZoneTag.atom.part.tsx", state: "Continue" }],
} as const

/* The helpers Caliper hands an authored check, typed locally: Zone imports
 * nothing from Caliper (test/authoring-gate.test.ts). */
interface CheckTools {
  readonly expect: (value: unknown) => { toBe(expected: unknown): void }
  readonly waitFor: (assertion: () => void) => Promise<void>
}

/**
 * The focused row's facts the screen does not show whole, and whether each
 * stands whole on the row's own line: a cut label or value with no whole
 * line is a fact the screen has lost.
 */
function lostFacts(): string {
  const row = document.querySelector<HTMLElement>(".zone-option-row[data-focused]")
  if (row === null) return "no focused row"
  const lost: string[] = []
  for (const cell of row.querySelectorAll<HTMLElement>("[data-cut-for]")) {
    const whole = cell.matches("[data-whole-words]") ? cell : cell.querySelector<HTMLElement>(":scope > [data-whole-words]")
    if (whole === null) continue
    const cut = cell.dataset["cut"] !== undefined || whole.scrollWidth > whole.clientWidth
    if (!cut) continue
    const name = cell.dataset["cutFor"] ?? ""
    const line = row.querySelector<HTMLElement>(`.zone-option-row-whole-line[data-for="${name}"]`)
    if (line === null || getComputedStyle(line).display === "none") lost.push(`${name}: ${whole.textContent ?? ""}`)
  }
  return lost.join("; ")
}

/* A route's program or build the row cuts stands whole on the focused
 * row's own line. */
export const checks = {
  default: {
    "the focused route's cut facts stand whole under it": async ({ expect, waitFor }: CheckTools) => {
      await waitFor(() => {
        expect(lostFacts()).toBe("")
      })
    },
  },
}

/** Unexported: the real page on a live program, so focus and actions answer. */
function Live({ choice, share = 1 }: { readonly choice: SurfaceRunnerChoice; readonly share?: number }) {
  const program = useRef<ReturnType<typeof createRunWithProgram> | null>(null)
  program.current ??= createRunWithProgram(choice)
  const live = program.current
  const snapshot = useSyncExternalStore(live.subscribe, live.get)
  // The binding measures its root; a preview reads the frame once.
  const layout = plan(innerWidth * share, innerHeight)
  return (
    <div className="zone-theme zone-screen" data-ground="options">
      <ZoneGround />
      <ZoneRunWith
        band={layout.band}
        strips={layout.strips}
        build="korri 0.4.1 · 9a1fb22"
        choice={snapshot.choice}
        clock="21:47"
        focused={snapshot.state.focused}
        {...runWithCallbacks(live.dispatch)}
      />
    </div>
  )
}

/** Two routes, nothing saved: Korri's message, each route's actions, reload and cancel. */
export default function Ready() {
  return <Live choice={fixtureRunnerReady} />
}

/** A saved game choice picks mGBA: SAVED and THIS GAME over its row. */
export function Saved() {
  return <Live choice={fixtureRunnerSaved} />
}

/** Launched with warnings: the choice's warning at the top, the route's under its row. */
export function Warnings() {
  return <Live choice={fixtureRunnerWarnings} />
}

/** A saved runner is gone: the orphaned choice is listed, with a way to clear it. */
export function Stale() {
  return <Live choice={fixtureRunnerStale} />
}

/** Another game is running: no routes, Korri's message and its stop action. */
export function Conflict() {
  return <Live choice={fixtureRunnerConflict} />
}

/** Korri is at work on the choice: the routes stay, only cancel acts. */
export function Busy() {
  return <Live choice={fixtureRunnerBusy} />
}

/** Korri could not read the runners: its message, reload and cancel. */
export function Error() {
  return <Live choice={fixtureRunnerError} />
}

/** Korri is reading the runners: the message and cancel. */
export function Loading() {
  return <Live choice={fixtureRunnerLoading} />
}

/** One route, and the longest title in the catalog. */
export function OneRoute() {
  return <Live choice={fixtureRunnerOneRoute} />
}

/** Five routes, longer than the screen: the list scrolls, a cut-off row at the edge. */
export function ManyRoutes() {
  return <Live choice={fixtureRunnerManyRoutes} />
}

/** A narrow column. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Live choice={fixtureRunnerSaved} share={0.36} />
    </ZonePartFrame>
  )
}
