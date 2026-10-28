/**
 * Pico previews Settings → Plugins from a copy of this module's output
 * (surfaces/pico/src/fixtures/plugin-settings.ts): a surface may not import
 * the portal. This test keeps the copy equal to what the portal publishes, so
 * the screenshots show the real rows. Regenerate the copy with
 * plugin-repositories.pico-fixtures.generate.ts.
 *
 * The import points from the portal into Pico's fixtures, not the other way,
 * because a surface may import nothing from Korri but `contracts/surface/`;
 * the portal is the only side that can hold both.
 */
import { describe, expect, it } from "bun:test"
import { pluginIds, pluginModels } from "../../../../surfaces/pico/src/fixtures/plugin-settings"
import { pluginSettingGroups } from "./plugin-repositories"
import { picoPluginIds } from "./plugin-repositories.pico-fixtures.generate"
import { picoPluginStates } from "./plugin-repositories.pico-states"

// Pico's models put its fixture Device group first.
const pluginGroupsOf = (model: { readonly settings: readonly unknown[] }) => model.settings.slice(1)

describe("Pico's plugin previews", () => {
  it("are exactly the rows the portal publishes", () => {
    for (const [name, state] of Object.entries(picoPluginStates)) {
      const model = pluginModels[name as keyof typeof pluginModels]
      expect([name, pluginGroupsOf(model)]).toEqual([name, pluginSettingGroups(state.facts, state.installed)])
    }
  })

  it("name the ids the portal gives those rows and actions", () => {
    expect(pluginIds as Record<string, string>).toEqual(picoPluginIds)
  })

  it("cover every publisher state on a review and the Publishers list", () => {
    const words = JSON.stringify(Object.values(pluginModels).map(pluginGroupsOf))
    for (const value of ["Configured", "Trusted", "Not trusted yet", "Key changed", "Not signed", "Revoke", "Trust the new key", "Trust @mallory", "Trust @lib"]) {
      expect(words).toContain(`"value":"${value}"`)
    }
  })
})
