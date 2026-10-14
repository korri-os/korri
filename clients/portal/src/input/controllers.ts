import type { SurfaceController } from "@contracts/surface/korri-surface"
import type { GamepadSnapshot } from "./gamepad-adapter"

/** Controller presence and evidence, not navigation or physical key values. */
export function createControllers() {
  let controllers: readonly SurfaceController[] = []
  const heard = new Set<string>()
  const listeners = new Set<() => void>()
  const publish = (next: readonly SurfaceController[]) => {
    if (JSON.stringify(next) === JSON.stringify(controllers)) return
    controllers = next
    for (const listener of [...listeners]) listener()
  }
  const input = (id: string) => {
    heard.add(id)
    publish(controllers.map(controller => controller.id === id ? { ...controller, heard: true } : controller))
  }
  return {
    snapshot: () => controllers,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener) } },
    added(id: string, name: string) {
      publish([...controllers.filter(controller => controller.id !== id), { id, name, heard: heard.has(id) }])
    },
    removed(id: string) { publish(controllers.filter(controller => controller.id !== id)) },
    disconnected() { publish([]) },
    input,
    gamepads(pads: readonly (GamepadSnapshot | null)[]) {
      publish(pads.filter((pad): pad is GamepadSnapshot => pad !== null && pad.connected).map(pad => {
        const id = `${pad.index}:${pad.id}`
        if (pad.buttons.some(button => button.pressed) || pad.axes.some(value => Math.abs(value) > 0.5)) heard.add(id)
        return { id, name: pad.id, heard: heard.has(id) }
      }))
    },
  }
}
export type Controllers = ReturnType<typeof createControllers>
