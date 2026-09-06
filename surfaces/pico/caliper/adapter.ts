import { createElement, type ReactNode } from "react"
import { createRoot } from "react-dom/client"
import type { SurfaceInputAction } from "../../../contracts/surface/korri-surface"
import { picoSurface } from "../src/index"
import { createPreviewSession } from "./fixture-session"
import { renderPicoPart } from "./render-part"
import { PicoFixtureRoot, type FixtureRootHost, type FixtureRootProps, type RegisterFixture } from "./PicoFixtureRoot"
import { pageSeed, type PicoPageStory } from "./page-seed"
import { isScenario, scenarioOptions } from "./scenarios"

type Context = { readonly scopeId?: string }
type MountOptions = Context & { readonly mode: "fixture" | "live"; readonly initialValues?: unknown }
const actions: readonly SurfaceInputAction[] = ["back", "options", "menu", "system"]

/** Each project owns its mounts; events never cross into another project. */
export function createPicoAdapter() {
  const sessions = new Set<{ scopeId?: string; session: ReturnType<typeof createPreviewSession> }>()
  const register: RegisterFixture = (scopeId, session) => {
    const registration = { scopeId, session }
    sessions.add(registration)
    return () => { sessions.delete(registration) }
  }
  const targets = (context?: Context) => [...sessions].filter(s =>
    context?.scopeId === undefined || s.scopeId === context.scopeId)
  const events = [{
    id: "pico-input", label: "Pico input", defaultPayload: "options",
    payload: { kind: "select" as const, options: actions.map(id => ({ id, label: id })) },
    emit(value: unknown, context?: Context) {
      if (!actions.includes(value as SurfaceInputAction)) return
      targets(context).forEach(({ session }) => session.host.press(value as SurfaceInputAction))
    },
  }, {
    id: "pico-fixture", label: "Fixture scenario (resets model)", defaultPayload: "ready",
    payload: { kind: "select" as const, options: scenarioOptions },
    emit(value: unknown, context?: Context) {
      if (!isScenario(value)) return
      targets(context).forEach(({ session }) => session.select(value))
    },
  }]
  const PartRoot = (props: FixtureRootProps) => createElement(PicoFixtureRoot, { ...props, register })
  return {
    id: picoSurface.id,
    supportsLiveData: false,
    renderSurfacePart: renderPicoPart,
    partRegistryRoot: PartRoot,
    surfacePartMount(story: PicoPageStory, binding: { readonly sourceId: string; readonly inputValues: Readonly<Record<string, unknown>> }) {
      const seed = pageSeed(story, binding)
      return seed ? { initialValues: seed, node: null } : null
    },
    surfacePartEvents: (story: PicoPageStory) => story.layer === "page" ? events : [],
    devices: [
      { id: "rg353m", name: "RG353M", widthMm: 72, heightMm: 52 },
      { id: "thor", name: "THOR", widthMm: 132, heightMm: 76 },
      { id: "odin2portal", name: "ODIN 2 PORTAL", widthMm: 156, heightMm: 85 },
    ],
    sources: scenarioOptions,
    makeSeedInitialValues: async () => "ready",
    makeSeedInitialValuesForBinding: async ({ sourceId }: { sourceId: string }) => sourceId,
    previewScope: (children: ReactNode) => createElement("div", {
      className: "pico-caliper-scope pico-caliper-preview",
    }, createElement("div", { className: "intrinsic pico-theme pico-screen" }, children)),
    eventsForScreen: (_path: string) => events,
    mountSurface(host: HTMLElement, options: MountOptions) {
      if (options.mode !== "fixture") throw new Error("Pico's Caliper adapter supports fixtures only")
      const container = document.createElement("div")
      container.className = "pico-caliper-host"
      host.append(container)
      const root = createRoot(container)
      let controller: FixtureRootHost | undefined
      let disposed = false
      root.render(createElement(PicoFixtureRoot, {
        initialValues: options.initialValues, scopeId: options.scopeId,
        keyboardTarget: host, register,
        onHost(next) { controller = next; if (disposed) next.dispose() },
      }))
      return {
        router: null,
        dispose() {
          if (disposed) return
          disposed = true
          controller?.dispose()
          queueMicrotask(() => { root.unmount(); container.remove() })
        },
      }
    },
  }
}

export const picoAdapter = createPicoAdapter()
