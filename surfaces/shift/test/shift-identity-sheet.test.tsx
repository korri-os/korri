import { afterEach, expect, test } from "bun:test"
import { cleanup, fireEvent, render } from "@testing-library/react"
import type { SurfaceIdentityManagement } from "@contracts/surface/korri-surface"
import {
  IDENTITY_BACKUP_ACTION,
  ShiftIdentitySheet,
} from "../src/ui/organisms/ShiftIdentitySheet"

afterEach(cleanup)

const identity: SurfaceIdentityManagement = {
  localBackupAvailable: true,
  retiredPublicKeys: [],
  status: { _tag: "Idle" },
}

// ShiftSurface passes a new inline callback on every render. Each render must
// not count as a new action: dismissing again makes the host publish a new
// model, which renders the surface again without end.
function sheet(action: string | null, dismissed: string[]) {
  return (
    <ShiftIdentitySheet
      action={action}
      identity={{ ...identity }}
      onClose={() => {}}
      onExport={() => {}}
      onSwitchLocal={() => {}}
      onSwitchNip46={() => {}}
      onDeleteRetired={() => {}}
      onDismissStatus={() => dismissed.push(action ?? "none")}
    />
  )
}

test("a re-render with the same action neither dismisses status nor clears input", () => {
  const dismissed: string[] = []
  const view = render(sheet(null, dismissed))
  view.rerender(sheet(null, dismissed))
  expect(dismissed).toEqual(["none"])

  view.rerender(sheet(IDENTITY_BACKUP_ACTION, dismissed))
  expect(dismissed).toEqual(["none", IDENTITY_BACKUP_ACTION])
  const password = document.querySelector<HTMLInputElement>('input[type="password"]')
  if (password === null) throw new Error("Expected the backup password field")
  fireEvent.change(password, { target: { value: "correct horse" } })

  view.rerender(sheet(IDENTITY_BACKUP_ACTION, dismissed))
  expect(dismissed).toEqual(["none", IDENTITY_BACKUP_ACTION])
  expect(password.value).toBe("correct horse")
})
