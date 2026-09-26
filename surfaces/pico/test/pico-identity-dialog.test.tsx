import { afterEach, expect, test } from "bun:test"
import { cleanup, fireEvent, render } from "@testing-library/react"
import type { SurfaceIdentityManagement } from "@contracts/surface/korri-surface"
import { PICO_IDENTITY_BACKUP_ACTION } from "../src/pico-settings-view"
import { PicoIdentityDialog } from "../src/ui/organisms/PicoIdentityDialog"

afterEach(cleanup)

const identity: SurfaceIdentityManagement = {
  localBackupAvailable: true,
  retiredPublicKeys: [],
  status: { _tag: "Idle" },
}

// PicoSurface passes a new inline callback on every render. Each render must
// not count as a new action: dismissing again makes the host publish a new
// model, which renders the surface again without end.
function dialog(action: string | null, dismissed: string[]) {
  return (
    <PicoIdentityDialog
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
  const view = render(dialog(null, dismissed))
  view.rerender(dialog(null, dismissed))
  expect(dismissed).toEqual(["none"])

  view.rerender(dialog(PICO_IDENTITY_BACKUP_ACTION, dismissed))
  expect(dismissed).toEqual(["none", PICO_IDENTITY_BACKUP_ACTION])
  const password = document.querySelector<HTMLInputElement>('input[type="password"]')
  if (password === null) throw new Error("Expected the backup password field")
  fireEvent.change(password, { target: { value: "correct horse" } })

  view.rerender(dialog(PICO_IDENTITY_BACKUP_ACTION, dismissed))
  expect(dismissed).toEqual(["none", PICO_IDENTITY_BACKUP_ACTION])
  expect(password.value).toBe("correct horse")
})
