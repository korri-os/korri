import { createElement, type ReactNode } from "react"
import type { SurfaceInputAction } from "../../../contracts/surface/korri-surface"
import { picoSurface } from "../src/index"
import { createPreviewSession } from "./fixture-session"
import { renderPicoPart } from "./render-part"

type Context = { readonly scopeId?: string }
type MountOptions = Context & { readonly mode: "fixture" | "live"; readonly initialValues?: unknown }
const keys: Readonly<Record<string, SurfaceInputAction>> = {
  Escape: "back", f: "options", m: "menu", s: "system",
}
const actions: readonly SurfaceInputAction[] = ["back", "options", "menu", "system"]
const scenarios = ["ready", "loading", "empty", "busy", "problem", "overlay"] as const

/** Each project owns its mounts; events never cross into another project. */
export function createPicoAdapter() {
  const sessions = new Set<{ scopeId?: string; session: ReturnType<typeof createPreviewSession> }>()
  const targets = (context?: Context) => [...sessions].filter(s =>
    context?.scopeId === undefined || s.scopeId === context.scopeId)
  return {
    id: picoSurface.id,
    supportsLiveData: false,
    renderSurfacePart: renderPicoPart,
    devices: [
      { id: "rg353m", name: "RG353M", widthMm: 72, heightMm: 52 },
      { id: "thor", name: "THOR", widthMm: 132, heightMm: 76 },
      { id: "odin2portal", name: "ODIN 2 PORTAL", widthMm: 156, heightMm: 85 },
    ],
    // Presentation scenarios, not an inventory of screens/components.
    sources: scenarios.map(id => ({ id, label: `Fixture: ${id}` })),
    makeSeedInitialValues: async () => "ready",
    makeSeedInitialValuesForBinding: async ({ sourceId }: { sourceId: string }) => sourceId,
    previewScope: (children: ReactNode) => createElement("div", {
      className: "pico-caliper-scope pico-caliper-preview",
    }, createElement("div", { className: "intrinsic pico-theme pico-screen" }, children)),
    eventsForScreen: (_path: string) => [{
      id: "pico-input", label: "Pico input", defaultPayload: "options",
      payload: { kind: "select" as const, options: actions.map(id => ({ id, label: id })) },
      emit(value: unknown, context?: Context) {
        if (!actions.includes(value as SurfaceInputAction)) return
        targets(context).forEach(({ session }) => session.host.press(value as SurfaceInputAction))
      },
    }, {
      id: "pico-fixture", label: "Fixture scenario (resets model)", defaultPayload: "ready",
      payload: { kind: "select" as const, options: scenarios.map(id => ({ id, label: id })) },
      emit(value: unknown, context?: Context) {
        if (!scenarios.includes(value as typeof scenarios[number])) return
        targets(context).forEach(({ session }) => session.select(String(value)))
      },
    }],
    mountSurface(host: HTMLElement, options: MountOptions) {
      if (options.mode !== "fixture") throw new Error("Pico's Caliper adapter supports fixtures only")
      // A dedicated child makes deferred cleanup safe even when Caliper remounts
      // the same host before React can dispose the previous root.
      const container = document.createElement("div")
      container.className = "pico-caliper-scope pico-caliper-mount"
      container.tabIndex = 0
      container.setAttribute("aria-label", "Pico fixture: F Find, M layout, S settings, Escape Back")
      host.append(container)
      let mounted: ReturnType<typeof picoSurface.mount> | undefined
      const session = createPreviewSession(model => mounted?.update(model))
      session.select(typeof options.initialValues === "string" ? options.initialValues : "ready")
      mounted = picoSurface.mount(container, session.model, session.host)
      const registration = { scopeId: options.scopeId, session }
      sessions.add(registration)
      const onKey = (event: KeyboardEvent) => {
        if (event.defaultPrevented || event.repeat || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return
        const target = event.target
        if (target instanceof Element && target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return
        const action = keys[event.key]
        if (!action) return
        event.preventDefault()
        event.stopPropagation()
        session.host.press(action)
      }
      host.addEventListener("keydown", onKey)
      let disposed = false
      return {
        router: null,
        dispose() {
          if (disposed) return
          disposed = true
          sessions.delete(registration)
          host.removeEventListener("keydown", onKey)
          queueMicrotask(() => { mounted?.unmount(); container.remove() })
        },
      }
    },
  }
}

export const picoAdapter = createPicoAdapter()
