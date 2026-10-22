/** Editor for one Shift setting, using the existing contextual side sheet. */
import type {
  SurfaceSettingItem,
  SurfaceSettingsStatus,
} from "@contracts/surface/korri-surface"
import { useEffect, useRef, useState } from "react"
import { ShiftSheetAction } from "../molecules/ShiftSheetAction"
import { ShiftSheetBody } from "./ShiftSheetBody"
import { ShiftSheetGroup } from "./ShiftSheetGroup"
import { ShiftSheetHeader } from "./ShiftSheetHeader"
import { ShiftSheetPanel } from "./ShiftSheetPanel"
import { ShiftSheetRoot } from "./ShiftSheetRoot"
import { ShiftSheetTitle } from "./ShiftSheetTitle"

export function ShiftSettingSheet({
  item,
  status,
  onChange,
  onDismissProblem,
  onClose,
}: {
  readonly item: SurfaceSettingItem | null
  readonly status: SurfaceSettingsStatus
  readonly onChange: (value: string) => void
  readonly onDismissProblem: () => void
  readonly onClose: () => void
}) {
  const isSensitive = item?.interaction?.kind === "sensitiveText"
  const [text, setText] = useState(isSensitive ? "" : (item?.value ?? ""))
  const savingSettingId = useRef<string | null>(null)
  // A background refresh rebuilds item objects. Keep an in-progress edit unless
  // the user actually moved to a different setting.
  useEffect(() => setText(isSensitive ? "" : (item?.value ?? "")), [item?.id, isSensitive])
  useEffect(() => {
    if (status._tag === "Saving" && status.settingId === item?.id) {
      savingSettingId.current = item.id
    } else if (status._tag === "Idle" && savingSettingId.current !== null) {
      const shouldClose = savingSettingId.current === item?.id
      savingSettingId.current = null
      if (shouldClose) onClose()
    }
  }, [item?.id, status, onClose])

  if (!item?.interaction || item.interaction.kind === "action") return null
  // Korri marks the choice in force by value: labels may repeat.
  const interaction = item.interaction
  const current = (choice: { readonly value: string }) =>
    interaction.kind === "choice" && choice.value === interaction.selected
  const saving = status._tag === "Saving"
  const problem = status._tag === "Problem" && status.settingId === item.id

  return (
    <ShiftSheetRoot open onClose={onClose} label={`Change ${item.label}`}>
      <ShiftSheetPanel>
        <ShiftSheetHeader>
          <ShiftSheetTitle>{item.label}</ShiftSheetTitle>
        </ShiftSheetHeader>
        <ShiftSheetBody>
          {problem ? (
            <ShiftSheetGroup title="Couldn't save">
              <p className="shift-setting-problem">{status.message}</p>
              <ShiftSheetAction label="Dismiss" onSelect={onDismissProblem} />
            </ShiftSheetGroup>
          ) : item.interaction.kind === "choice" ? (
            <ShiftSheetGroup title="Choose">
              {item.interaction.choices.map(choice => (
                <ShiftSheetAction
                  key={choice.value}
                  label={`${choice.label}${current(choice) ? " · Current" : ""}`}
                  disabled={saving}
                  onSelect={() => onChange(choice.value)}
                />
              ))}
            </ShiftSheetGroup>
          ) : (
            <ShiftSheetGroup title={isSensitive ? "Secret" : "Name"}>
              <input
                className="shift-setting-input"
                type={isSensitive ? "password" : "text"}
                value={text}
                placeholder={item.interaction.placeholder}
                maxLength={item.interaction.maxLength}
                disabled={saving}
                onChange={event => setText(event.currentTarget.value)}
                aria-label={item.label}
              />
              <ShiftSheetAction
                label={saving ? "Saving…" : "Save"}
                disabled={saving || text.trim().length === 0}
                onSelect={() => onChange(text)}
              />
              {isSensitive && item.interaction.clearLabel !== undefined ? (
                <ShiftSheetAction
                  label={item.interaction.clearLabel}
                  disabled={saving}
                  onSelect={() => onChange("")}
                />
              ) : null}
            </ShiftSheetGroup>
          )}
        </ShiftSheetBody>
      </ShiftSheetPanel>
    </ShiftSheetRoot>
  )
}
