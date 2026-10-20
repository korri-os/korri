/**
 * Shift game action confirmation — the question a destructive game action
 * asks before Shift sends it (Stop, Remove download).
 *
 * The treaty says a surface confirms a destructive action before firing it.
 * This is the same side sheet Settings uses for a confirmed action: a title,
 * the action's own description, then the choice. Cancel comes first and takes
 * the cursor when the sheet opens, so a stray confirm backs out instead of
 * deleting something. The confirming row keeps the danger tone it had in the
 * actions sheet. Korri gives game actions no confirmation copy, so the title
 * is built from the action's label and the message is its description.
 */
import type { SurfaceAction } from "@contracts/surface/korri-surface"
import { ShiftSheetAction } from "../molecules/ShiftSheetAction"
import { ShiftSheetBody } from "./ShiftSheetBody"
import { ShiftSheetGroup } from "./ShiftSheetGroup"
import { ShiftSheetHeader } from "./ShiftSheetHeader"
import { ShiftSheetPanel } from "./ShiftSheetPanel"
import { ShiftSheetRoot } from "./ShiftSheetRoot"
import { ShiftSheetTitle } from "./ShiftSheetTitle"

export interface ShiftGameActionConfirmSheetProps {
  readonly gameTitle: string
  readonly action: SurfaceAction
  readonly onConfirm: () => void
  readonly onCancel: () => void
}

export function ShiftGameActionConfirmSheet({
  gameTitle,
  action,
  onConfirm,
  onCancel,
}: ShiftGameActionConfirmSheetProps) {
  const title = `${action.label}?`
  return (
    <ShiftSheetRoot open onClose={onCancel} label={title}>
      <ShiftSheetPanel>
        <ShiftSheetHeader>
          <ShiftSheetTitle>{title}</ShiftSheetTitle>
        </ShiftSheetHeader>
        <ShiftSheetBody>
          <ShiftSheetGroup title={gameTitle}>
            <p className="shift-game-action-confirm-message">
              {action.description ?? "This cannot be undone."}
            </p>
            <ShiftSheetAction label="Cancel" onSelect={onCancel} />
            <ShiftSheetAction label={action.label} tone="danger" onSelect={onConfirm} />
          </ShiftSheetGroup>
        </ShiftSheetBody>
      </ShiftSheetPanel>
    </ShiftSheetRoot>
  )
}
