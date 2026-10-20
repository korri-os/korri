/**
 * The veil under every dialog: ZoneDialog lays the Mega Drive dither over
 * what it stands on, so every dialog over a page (a page's own questions,
 * GAMES' error, PAUSED's question, Korri's problem and question) has it,
 * and no page places a veil of its own under a dialog.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, render, screen } from "@testing-library/react"
import { readdirSync, readFileSync } from "node:fs"
import { join, relative } from "node:path"
import * as games from "../src/pages/ZoneGames.page.part"
import * as options from "../src/pages/ZoneOptions.page.part"
import * as saveData from "../src/pages/ZoneSaveData.page.part"
import * as status from "../src/ui/organisms/ZoneStatusLayer.organism.part"

afterEach(cleanup)

const SRC = join(import.meta.dir, "..", "src")
const walk = (dir: string): string[] => readdirSync(dir, { withFileTypes: true })
  .flatMap(entry => entry.isDirectory() ? walk(join(dir, entry.name)) : [join(dir, entry.name)])

/** The dialog is the sky panel; the element drawn just before it is its veil. */
function veiled(name: string): void {
  const dialog = screen.getAllByRole("dialog").find(node => node.classList.contains("zone-dialog"))
  expect(dialog, name).toBeDefined()
  expect(dialog!.previousElementSibling?.classList.contains("zone-veil"), name).toBe(true)
}

describe("every dialog over a page has the veil", () => {
  const cases: readonly [string, () => React.ReactNode][] = [
    ["OPTIONS' confirmation", options.ActionConfirm],
    ["OPTIONS' destructive confirmation", options.Destructive],
    ["SAVE DATA's question", saveData.ConfirmDelete],
    ["GAMES' error", games.Error],
    ["GAME OPTS.' question", games.GameOptsAsking],
    ["Korri's problem", status.default],
    ["Korri's question", status.Confirm],
  ]
  for (const [name, State] of cases) {
    test(name, () => {
      render(<State />)
      veiled(name)
    })
  }

  test("the veil is placed once: only the dialog, Busy's page and OPTIONS' keyboard draw one", () => {
    const drawers = walk(SRC)
      .filter(file => file.endsWith(".tsx") && !file.endsWith(".part.tsx"))
      .filter(file => /<ZoneVeil\b/.test(readFileSync(file, "utf8")))
      .map(file => relative(SRC, file))
      .sort()
    expect(drawers).toEqual(["pages/ZoneOptions.tsx", "ui/molecules/ZoneDialog.tsx", "ui/organisms/ZoneStatusLayer.tsx"])
  })
})
