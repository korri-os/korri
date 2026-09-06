import type { SurfaceGameplayControl, SurfaceModel } from "../../../contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel, fixtureOverlay } from "../src/fixtures/fixture-host"

/** Preview consequences only. No RPC, native bridge or filesystem mutations. */
export function createPreviewSession(publish: (model: SurfaceModel) => void) {
  let model: SurfaceModel = { ...fixtureModel, buildLabel: "CALIPER FIXTURE" }
  const recorded = createFixtureHost()
  const update = (next: SurfaceModel) => { model = next; publish(model) }
  const reset = () => update({ ...fixtureModel, buildLabel: "CALIPER FIXTURE" })
  const busy = (label: string, gameId?: string) => update({ ...model,
    status: { _tag: "Busy", kicker: `PREVIEW: ${label}`, gameId,
      detail: "Simulated request — no device action was performed." } })
  const host: ReturnType<typeof createFixtureHost> = {
    ...recorded,
    launchGame(id, location) { recorded.launchGame(id, location); busy("Starting game", id) },
    runAction(id) { recorded.runAction(id); busy(id) },
    runGameAction(game, action) { recorded.runGameAction(game, action); busy(action, game) },
    changeSetting(id, value) {
      recorded.changeSetting(id, value)
      update({ ...model, settingsStatus: { _tag: "Idle" }, settings: model.settings.map(group => ({
        ...group, items: group.items.map(item => {
          if (item.id !== id) return item
          // Setting values are display labels; commands carry the opaque value.
          const label = item.interaction?.kind === "choice"
            ? item.interaction.choices.find(choice => choice.value === value)?.label
            : undefined
          return { ...item, value: label ?? value }
        }),
      })) })
    },
    dismissSettingsProblem() {
      recorded.dismissSettingsProblem(); update({ ...model, settingsStatus: { _tag: "Idle" } })
    },
    invokeGameplayControl(id, value) {
      recorded.invokeGameplayControl(id, value)
      if (model.presentation.kind !== "gameplay-overlay") return
      const presentation = model.presentation
      const control = [...presentation.controls, ...presentation.groups.flatMap(g => g.controls)]
        .find(c => c.id === id)
      if (!control?.enabled) return
      // The fixture's Quit command ends the simulated game, unlike Resume.
      if (id === "quit") { reset(); return }
      if (control.dismissOnSuccess) { host.dismissGameplayOverlay(); return }
      const change = (c: SurfaceGameplayControl): SurfaceGameplayControl => {
        if (c.id !== id) return c
        if (c.interaction.kind === "command") return { ...c,
          description: "PREVIEW: request recorded — no device action was performed." }
        if (!value) return c
        const i = c.interaction
        if (i.kind === "toggle" && value.kind === "toggle") return { ...c, interaction: { ...i, value: value.value } }
        if (i.kind === "choice" && value.kind === "choice") return { ...c, interaction: { ...i, value: value.value } }
        if (i.kind === "range" && value.kind === "range") return { ...c, interaction: { ...i, value: value.value } }
        return c
      }
      update({ ...model, presentation: { ...presentation, controls: presentation.controls.map(change),
        groups: presentation.groups.map(g => ({ ...g, controls: g.controls.map(change) })) } })
    },
    dismissGameplayOverlay() {
      recorded.dismissGameplayOverlay()
      update({ ...model, presentation: { kind: "catalog" }, status: { _tag: "Running", kicker: "PREVIEW: Game running", gameId: "hollow" } })
    },
    retry() { recorded.retry(); busy("Retrying") },
    dismiss() { recorded.dismiss(); reset() },
    reload() { recorded.reload(); reset() },
  }
  return {
    host, get model() { return model },
    select(value: string) {
      reset()
      switch (value) {
        case "loading": update({ ...model, catalog: { _tag: "Loading" } }); break
        case "empty": update({ ...model, catalog: { _tag: "Empty" } }); break
        case "busy": busy("Starting game", "hollow"); break
        case "problem": update({ ...model, status: { _tag: "Problem", kicker: "PREVIEW: Launch failed", reason: "Simulated failure", canRetry: true, gameId: "hollow" } }); break
        case "overlay": update({ ...model, presentation: fixtureOverlay, status: { _tag: "Running", kicker: "PREVIEW: Playing", gameId: "hollow" } }); break
      }
    },
  }
}
