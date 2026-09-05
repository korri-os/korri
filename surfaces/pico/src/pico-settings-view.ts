import type {
  SurfaceModel,
  SurfaceSettingItem,
} from "@contracts/surface/korri-surface"

/**
 * One settings row as the screen draws it.
 *
 * The treaty's interaction kinds collapse to what Pico can actually offer from
 * a d-pad and two buttons. `choice` becomes a cycler that sends the next value
 * on confirm — the same control legacy drew as `‹ VALUE ›`. `text` and
 * `sensitiveText` are shown but not edited: Pico has no on-screen keyboard yet,
 * and a row that looks editable and is not would be a lie the user finds out
 * about with their thumb.
 */
export type PicoSettingControl =
  | { readonly kind: "fact" }
  | { readonly kind: "action"; readonly actionId: string; readonly destructive: boolean; readonly confirmation?: PicoConfirmation }
  | { readonly kind: "cycle"; readonly next: string; readonly options: readonly string[]; readonly current: number }
  | { readonly kind: "text" }

export interface PicoConfirmation {
  readonly title: string
  readonly message: string
  readonly confirmLabel: string
}

export interface PicoSettingRowView {
  readonly id: string
  readonly label: string
  readonly value?: string
  readonly description?: string
  readonly control: PicoSettingControl
  /** What Korri says about this row right now. */
  readonly state: "idle" | "saving" | { readonly problem: string }
}

export interface PicoSettingsGroupView {
  readonly title: string
  readonly rows: readonly PicoSettingRowView[]
}

export interface PicoSettingsView {
  readonly groups: readonly PicoSettingsGroupView[]
  readonly buildLabel?: string
}

export function picoSettingsViewFromModel(model: SurfaceModel): PicoSettingsView {
  return {
    groups: model.settings.map((group) => ({
      title: group.title.toUpperCase(),
      rows: group.items.map((item) => rowFor(item, model)),
    })),
    ...(model.buildLabel === undefined ? {} : { buildLabel: model.buildLabel }),
  }
}

function rowFor(item: SurfaceSettingItem, model: SurfaceModel): PicoSettingRowView {
  const status = model.settingsStatus
  const state =
    status._tag === "Saving" && status.settingId === item.id
      ? "saving"
      : status._tag === "Problem" && status.settingId === item.id
        ? { problem: status.message }
        : "idle"
  return {
    id: item.id,
    label: item.label,
    ...(item.value === undefined ? {} : { value: item.value }),
    ...(item.description === undefined ? {} : { description: item.description }),
    control: controlFor(item),
    state,
  }
}

function controlFor(item: SurfaceSettingItem): PicoSettingControl {
  const interaction = item.interaction
  if (interaction === undefined) return { kind: "fact" }
  switch (interaction.kind) {
    case "action":
      return {
        kind: "action",
        actionId: interaction.actionId,
        destructive: interaction.destructive === true,
        ...(interaction.confirmation === undefined ? {} : { confirmation: interaction.confirmation }),
      }
    case "choice": {
      const labels = interaction.choices.map((choice) => choice.label)
      const current = Math.max(0, interaction.choices.findIndex((choice) => choice.label === item.value))
      const nextChoice = interaction.choices[(current + 1) % interaction.choices.length]
      return {
        kind: "cycle",
        next: nextChoice?.value ?? "",
        options: labels,
        current,
      }
    }
    case "text":
    case "sensitiveText":
      return { kind: "text" }
  }
}
