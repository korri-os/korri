import type {
  SurfaceModel,
  SurfaceSettingItem,
} from "@contracts/surface/korri-surface"
import { PICO_FONTS, type PicoFontId } from "./pico-fonts"

/**
 * One settings row as the screen draws it.
 *
 * The treaty's interaction kinds collapse to what Pico can actually offer from
 * a d-pad and two buttons. `choice` becomes a cycler that sends the next value
 * on confirm — the same control legacy drew as `‹ VALUE ›`. `text` and
 * `sensitiveText` are shown but not edited: Pico has no on-screen keyboard yet,
 * and a row that looks editable and is not would be a lie the user finds out
 * about with their thumb.
 *
 * `step` is a cycler for a long list: it shows only the current choice, as
 * `◀ CHOICE ▶`, because ten face names side by side do not fit a row.
 */
export type PicoSettingControl =
  | { readonly kind: "fact" }
  | { readonly kind: "action"; readonly actionId: string; readonly destructive: boolean; readonly confirmation?: PicoConfirmation }
  | { readonly kind: "cycle"; readonly next: string; readonly options: readonly string[]; readonly current: number }
  | { readonly kind: "step"; readonly next: string; readonly current: string }
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

export const PICO_IDENTITY_BACKUP_ACTION = "identity:backup"
export const PICO_IDENTITY_SWITCH_LOCAL_ACTION = "identity:switch:local"
export const PICO_IDENTITY_SWITCH_NIP46_ACTION = "identity:switch:nip46"
export const picoIdentityRetiredExportAction = (key: string) => `identity:retired:export:${key}`
export const picoIdentityRetiredDeleteAction = (key: string) => `identity:retired:delete:${key}`

export interface PicoSettingsView {
  readonly groups: readonly PicoSettingsGroupView[]
  readonly buildLabel?: string
}

/** The id of Pico's own face row. Pico handles a change to it; Korri never sees it. */
export const PICO_FONT_SETTING = "pico:font"

/**
 * Pico's own settings: things only this surface has, kept by Pico on this
 * device. Listed after everything Korri published, under Pico's name, so they
 * never read as Korri's.
 */
function picoGroup(font: PicoFontId): PicoSettingsGroupView {
  const at = Math.max(0, PICO_FONTS.findIndex((candidate) => candidate.id === font))
  const current = PICO_FONTS[at]
  const next = PICO_FONTS[(at + 1) % PICO_FONTS.length]
  return {
    title: "PICO",
    rows: [{
      id: PICO_FONT_SETTING,
      label: "Font",
      description: "Kept on this device. Changes at once.",
      control: { kind: "step", next: next?.id ?? font, current: current?.name ?? font },
      state: "idle",
    }],
  }
}

/**
 * The settings screen's view. `pico` carries Pico's own preferences; without
 * it the view holds only what Korri published, as a preview of Korri's part
 * shows it.
 */
export function picoSettingsViewFromModel(
  model: SurfaceModel,
  pico?: { readonly font: PicoFontId },
): PicoSettingsView {
  const identity = model.identityManagement
  const identityRows: PicoSettingRowView[] = identity ? [
    ...(identity.localBackupAvailable ? [{
      id: PICO_IDENTITY_BACKUP_ACTION,
      label: "Back up current identity",
      description: "Encrypted NIP-49 text and QR code",
      control: { kind: "action" as const, actionId: PICO_IDENTITY_BACKUP_ACTION, destructive: false },
      state: "idle" as const,
    }] : []),
    {
      id: PICO_IDENTITY_SWITCH_LOCAL_ACTION,
      label: "Switch from backup",
      control: { kind: "action" as const, actionId: PICO_IDENTITY_SWITCH_LOCAL_ACTION, destructive: true },
      state: "idle" as const,
    },
    {
      id: PICO_IDENTITY_SWITCH_NIP46_ACTION,
      label: "Switch to NIP-46",
      control: { kind: "action" as const, actionId: PICO_IDENTITY_SWITCH_NIP46_ACTION, destructive: true },
      state: "idle" as const,
    },
    ...identity.retiredPublicKeys.flatMap(publicKey => {
      const short = `${publicKey.slice(0, 8)}…${publicKey.slice(-8)}`
      return [{
        id: picoIdentityRetiredExportAction(publicKey),
        label: `Back up retired ${short}`,
        control: { kind: "action" as const, actionId: picoIdentityRetiredExportAction(publicKey), destructive: false },
        state: "idle" as const,
      }, {
        id: picoIdentityRetiredDeleteAction(publicKey),
        label: `Delete retired ${short}`,
        control: { kind: "action" as const, actionId: picoIdentityRetiredDeleteAction(publicKey), destructive: true },
        state: "idle" as const,
      }]
    }),
  ] : []
  return {
    groups: [
      ...model.settings.map((group) => ({
        title: group.title.toUpperCase(),
        rows: group.items.map((item) => rowFor(item, model)),
      })),
      ...(identityRows.length ? [{ title: "IDENTITY", rows: identityRows }] : []),
      ...(pico === undefined ? [] : [picoGroup(pico.font)]),
    ],
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
