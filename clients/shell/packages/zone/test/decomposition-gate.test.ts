/**
 * Decomposition: Zone's React tree, component for component, in .slint.
 *
 * Every component of surfaces/zone/src/ui and src/pages (as it stood at
 * the cut, listed below) is one Slint component at the same layer, in its
 * own file, unless it is merged into another for the reason given. Every
 * component has its preview file beside it, `<Name>.<layer>.preview.slint`,
 * the Caliper part's role: named preview components with fixture data that
 * import and render the component. The previews render through the shell's
 * own interpreter (clients/shell/tests/package_previews.rs).
 */
import { describe, expect, test } from "bun:test"
import { existsSync } from "node:fs"
import { join } from "node:path"
import { componentsIn, isPreview, LAYER_SUFFIX, layerOf, LAYERS, PACKAGE, read, slintFiles, stripComments } from "./slint-source"

/** Zone's React components at the cut, by layer: the Slint file each became, or why it was merged. */
const REACT: Readonly<Record<string, string>> = {
  // atoms
  ZoneBanner: "ui/atoms/ZoneBanner.slint",
  ZoneBars: "ui/atoms/ZoneBars.slint",
  ZoneButton: "ui/atoms/ZoneButton.slint",
  ZoneCaps: "ui/atoms/ZoneCaps.slint",
  ZoneCoverWell: "ui/atoms/ZoneCoverWell.slint",
  ZoneEmblem: "ui/atoms/ZoneEmblem.slint",
  ZoneKeycap: "ui/atoms/ZoneKeycap.slint",
  ZonePanel: "ui/atoms/ZonePanel.slint",
  ZoneQrWell: "ui/atoms/ZoneQrWell.slint",
  ZoneSignal: "ui/atoms/ZoneSignal.slint",
  ZoneSlab: "merged into ui/atoms/ZonePanel.slint: a slab was a panel that takes no room, kept apart in CSS only to stop a ResizeObserver loop; Slint panels never measure themselves",
  ZoneTab: "ui/atoms/ZoneTab.slint",
  ZoneTag: "ui/atoms/ZoneTag.slint",
  ZoneTriangle: "ui/atoms/ZoneTriangle.slint",
  ZoneUnderline: "ui/atoms/ZoneUnderline.slint",
  ZoneVeil: "ui/atoms/ZoneVeil.slint",
  ZoneVersus: "ui/atoms/ZoneVersus.slint",
  ZoneWords: "ui/atoms/ZoneWords.slint",
  // molecules
  ZoneActionRow: "ui/molecules/ZoneActionRow.slint",
  ZoneBusyStrip: "ui/molecules/ZoneBusyStrip.slint",
  ZoneCardBand: "ui/molecules/ZoneCardBand.slint",
  ZoneCard: "ui/molecules/ZoneCard.slint",
  ZoneDialog: "ui/molecules/ZoneDialog.slint",
  ZoneFoot: "ui/molecules/ZoneFoot.slint",
  ZoneKeyboard: "ui/molecules/ZoneKeyboard.slint",
  ZoneOptionGroup: "ui/molecules/ZoneOptionGroup.slint",
  ZoneOptionRow: "ui/molecules/ZoneOptionRow.slint",
  ZoneSaveCard: "ui/molecules/ZoneSaveCard.slint",
  ZoneSectionCard: "ui/molecules/ZoneSectionCard.slint",
  ZoneTabStrip: "ui/molecules/ZoneTabStrip.slint",
  ZoneTagRow: "ui/molecules/ZoneTagRow.slint",
  ZoneTitleCard: "ui/molecules/ZoneTitleCard.slint",
  // organisms
  ZoneAttract: "ui/organisms/ZoneAttract.slint",
  ZoneControllerCard: "ui/organisms/ZoneControllerCard.slint",
  ZoneGround: "ui/organisms/ZoneGround.slint",
  ZoneLaunchCard: "ui/organisms/ZoneLaunchCard.slint",
  ZoneOptionsEntry: "ui/organisms/ZoneOptionsEntry.slint",
  ZonePausedControl: "ui/organisms/ZonePausedControl.slint",
  ZoneRunWithRoute: "ui/organisms/ZoneRunWithRoute.slint",
  ZoneSaveBand: "ui/organisms/ZoneSaveBand.slint",
  ZoneSaveEntry: "merged into ui/organisms/ZoneOptionsEntry.slint: both raise the one keyboard over a veiled page; what it types and for what comes from the view",
  ZoneSecretSlab: "ui/organisms/ZoneSecretSlab.slint",
  ZoneStatusLayer: "ui/organisms/ZoneStatusLayer.slint",
  ZoneStepStrip: "ui/organisms/ZoneStepStrip.slint",
  ZoneTally: "ui/organisms/ZoneTally.slint",
  ZoneWarningLines: "ui/organisms/ZoneWarningLines.slint",
  // templates
  ZoneShell: "ui/templates/ZoneShell.slint",
  // pages
  ZoneGames: "pages/ZoneGames.slint",
  ZoneLaunchSettings: "pages/ZoneLaunchSettings.slint",
  ZoneOnboarding: "pages/ZoneOnboarding.slint",
  ZoneOptions: "pages/ZoneOptions.slint",
  ZonePaused: "pages/ZonePaused.slint",
  ZoneRunWith: "pages/ZoneRunWith.slint",
  ZoneSaveData: "pages/ZoneSaveData.slint",
  ZoneStore: "pages/ZoneStore.slint",
}

/**
 * Components with no React counterpart, each a unit CSS or a page's markup
 * drew without a name: the reason it exists in Slint.
 */
const ADDED: Readonly<Record<string, string>> = {
  "ui/atoms/ZoneShape.slint": "a filled path: CSS drew every slant with clip-path; Slint needs one Path element for them",
  "ui/atoms/ZoneMeasure.slint": "measures words in a look's face without drawing them: the DOM measured its own boxes",
  "ui/atoms/ZoneTap.slint": "where a finger presses a focus target (ADR 0055): the DOM's click and pointer events did this",
  "ui/atoms/ZoneSlide.slint": "how a rack that moves by focus follows a finger, card by card (ADR 0055): the DOM's scroll and fling did this",
  "ui/molecules/ZoneFactLine.slint": "GAMES' facts line, markup inside ZoneGames.tsx",
  "ui/organisms/ZoneRowList.slint": "the scrolled list of rows every list page shares; CSS scrolled each page's own box",
  "ui/organisms/ZoneSkyBand.slint": "OPTIONS' sky band with its lips and black wedge, drawn by four pages' stylesheets",
  "ui/organisms/ZoneGameOpts.slint": "GAME OPTS.' sheet, markup inside ZoneGames.tsx",
}

const files = slintFiles()
const components = files.filter(file => layerOf(file) !== undefined && !isPreview(file))

describe("decomposition", () => {
  test("every React component of Zone has its Slint component, or a merge with its reason", () => {
    const missing = Object.entries(REACT).filter(([, target]) => !target.startsWith("merged") && !existsSync(join(PACKAGE, target)))
    expect(missing.map(([name, target]) => `${name} → ${target}`)).toEqual([])
  })

  test("each Slint component is named for its file and stands at its React component's layer", () => {
    const wrong = Object.entries(REACT).filter(([name, target]) => {
      if (target.startsWith("merged") || !existsSync(join(PACKAGE, target))) return false
      return !componentsIn(read(target)).includes(name)
    })
    expect(wrong.map(([name]) => name)).toEqual([])
  })

  test("no component without a React counterpart or a stated reason", () => {
    const known = new Set([...Object.values(REACT), ...Object.keys(ADDED)])
    expect(components.filter(file => !known.has(file))).toEqual([])
  })

  test("every layer is used, pages and a template among them", () => {
    expect([...new Set(components.map(layerOf))].sort()).toEqual([...LAYERS].sort())
  })

  test("every component has its preview file beside it", () => {
    const without = components.filter(file => {
      const layer = layerOf(file)!
      return !existsSync(join(PACKAGE, file.replace(/\.slint$/, `.${LAYER_SUFFIX[layer]}.preview.slint`)))
    })
    expect(without).toEqual([])
  })

  test("every preview file sits beside a component, and every state renders that component", () => {
    const wrong = files.filter(isPreview).filter(file => {
      const component = file.replace(/\.[a-z]+\.preview\.slint$/, ".slint")
      if (!existsSync(join(PACKAGE, component))) return true
      const name = component.slice(component.lastIndexOf("/") + 1, -".slint".length)
      const source = stripComments(read(file))
      const imported = new RegExp(`import\\s*\\{[^}]*\\b${name}\\b[^}]*\\}\\s*from\\s*"\\./${name}\\.slint"`).test(source)
      // Every preview state renders the component.
      const states = source.split(/export\s+component\s+/).slice(1)
      const rendered = states.length > 0 && states.every(state => new RegExp(`(^|[\\s:=])${name}\\s*\\{`, "m").test(state))
      return !imported || !rendered
    })
    expect(wrong).toEqual([])
  })

  test("every preview file exports at least one named preview state", () => {
    const empty = files.filter(isPreview).filter(file => !/export\s+component\s+[A-Z]\w*\s+inherits\s+ZonePreview/.test(stripComments(read(file))))
    expect(empty).toEqual([])
  })
})
