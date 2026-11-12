/**
 * Writes every component's preview file, `<Name>.<layer>.preview.slint`,
 * beside it: named states that draw the component alone, with data from
 * Velvet's own scenarios (preview/scenarios) played through logic.ts.
 *
 *     bun packages/velvet/preview/previews.ts        (in clients/shell)
 *
 * Each state inherits VelvetPreview (ui/preview-frame.slint), which feeds
 * the Tokens global as main.slint does. The preview renderer
 * (clients/shell/tests/velvet_package.rs) draws every state at five sizes
 * with the logic's plan for that size. The data is the view at 1280 x 720:
 * a state shows a component, not a screen, and the scenarios' frames are
 * the screens.
 *
 * A preview imports the foundation, the frame and its own component, and
 * nothing else (test/atomic-gate.test.ts).
 */
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { qrDataUrl } from "../src/qr"
import { PACKAGE, play, scenario } from "../test/korri"
import { literal } from "./slint-literal"

const LAYERS = { atoms: "atom", molecules: "molecule", organisms: "organism", pages: "page" } as const
type Layer = keyof typeof LAYERS

/** The view a scenario ends on. */
const views = new Map<string, any>()
function view(name: string): any {
  if (!views.has(name)) views.set(name, play(scenario(name), 1280, 720).view)
  return views.get(name)
}

const up = (layer: Layer) => (layer === "pages" ? "../" : "../../")
const lit = (layer: Layer) => (value: unknown) => literal(value, up(layer))
const FILL = "x: 0; y: 0; width: parent.width; height: parent.height;"
const centre = (width: string, height: string) => `x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; width: ${width}; height: ${height};`
const U = "Tokens.unit"

interface State { readonly name: string; readonly body: string }
const files: { layer: Layer; component: string; states: State[] }[] = []
const add = (layer: Layer, component: string, states: State[]) => files.push({ layer, component, states })
const one = (component: string, props: string) => `${component} { ${props} }`

// ---- Atoms ----
{
  const l = lit("atoms")
  const home = view("home"), keys = view("settings-text").keys as any[]
  const pills = view("detail-two-places").detailButtons as any[]
  add("atoms", "VelvetBack", [{ name: "Shown", body: one("VelvetBack", "x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; shown: true;") }])
  add("atoms", "VelvetBadge", [
    { name: "Short", body: one("VelvetBadge", 'x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; text: "GBA";') },
    { name: "Long", body: one("VelvetBadge", 'x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; text: "Nintendo 64";') },
  ])
  add("atoms", "VelvetBar", [0.5, 1, 0.04].map((share, index) => ({ name: ["Half", "Full", "Started"][index]!, body: one("VelvetBar", `x: parent.width * 0.2; y: parent.height / 2; width: parent.width * 0.6; share: ${share};`) })))
  add("atoms", "VelvetCaps", [
    { name: "Title", body: one("VelvetCaps", `x: Tokens.screen.pad-x * ${U}; y: parent.height * 0.3; width: parent.width - 2 * self.x; lines: ["EMBER", "ROAD"]; size: Tokens.screen.title * ${U};`) },
    { name: "Centred", body: one("VelvetCaps", `x: Tokens.screen.pad-x * ${U}; y: parent.height * 0.3; width: parent.width - 2 * self.x; lines: ["COULD NOT START"]; size: Tokens.screen.head * ${U}; centred: true;`) },
    { name: "LongWord", body: one("VelvetCaps", `x: Tokens.screen.pad-x * ${U}; y: parent.height * 0.3; width: parent.width - 2 * self.x; lines: ["ANTIDISESTABLISH", "MENTARIANISM"]; size: Tokens.screen.head * ${U};`) },
  ])
  add("atoms", "VelvetEdge", [
    { name: "Lit", body: one("VelvetEdge", `${centre("parent.height * 0.4", "parent.height * 0.6")} lit: true;`) },
    { name: "Rest", body: `Rectangle { ${centre("parent.height * 0.4", "parent.height * 0.6")} background: Tokens.light-10; VelvetEdge { width: parent.width; height: parent.height; } }` },
  ])
  add("atoms", "VelvetFact", [
    { name: "Plain", body: one("VelvetFact", `x: Tokens.screen.pad-x * ${U}; y: parent.height / 2; fact: ${l(home.home.mark.facts.first.b)};`) },
    { name: "Strong", body: one("VelvetFact", `x: Tokens.screen.pad-x * ${U}; y: parent.height / 2; fact: ${l(home.home.mark.facts.first.a)};`) },
  ])
  add("atoms", "VelvetGhost", [{ name: "Place", body: one("VelvetGhost", centre("parent.height * 0.4", "parent.height * 0.6")) }])
  add("atoms", "VelvetHead", [{ name: "Library", body: one("VelvetHead", `x: Tokens.screen.pad-x * ${U}; y: Tokens.screen.pad-y * ${U}; height: Tokens.screen.head-h * ${U}; text: "LIBRARY";`) }])
  add("atoms", "VelvetIcon", ["clock", "watch", "here", "peer", "play", "resume", "grid", "sliders", "check", "plus", "gear", "menu", "more", "back", "lock", "save", "down", "note", "key", "wifi", "person", "stop"].map(name => ({
    name: name.charAt(0).toUpperCase() + name.slice(1), body: one("VelvetIcon", `${centre("self.size", "self.size")} name: "${name}"; ink: Tokens.house-light; size: Math.min(parent.width, parent.height) * 0.3;`),
  })))
  const key = (id: string) => keys.find(entry => entry.focusId === id)
  add("atoms", "VelvetKeycap", [
    { name: "Letter", body: one("VelvetKeycap", `${centre(`Tokens.screen.key-w * ${U}`, `Tokens.screen.key-h * ${U}`)} key: ${l({ ...key("k:q"), focused: false })};`) },
    { name: "Focused", body: one("VelvetKeycap", `${centre(`Tokens.screen.key-w * ${U}`, `Tokens.screen.key-h * ${U}`)} key: ${l({ ...key("k:q"), focused: true })};`) },
    { name: "Done", body: one("VelvetKeycap", `${centre(`Tokens.screen.key-w * ${U} * 2`, `Tokens.screen.key-h * ${U}`)} key: ${l(key("k:done"))};`) },
  ])
  add("atoms", "VelvetLight", [
    { name: "Ember", body: one("VelvetLight", `${FILL} glow: Tokens.tint(${view("home").stage.light});`) },
    { name: "Tide", body: one("VelvetLight", `${FILL} glow: Tokens.tint(${view("row").stage.light});`) },
    { name: "House", body: one("VelvetLight", `${FILL}`) },
  ])
  add("atoms", "VelvetPill", [
    { name: "Filled", body: one("VelvetPill", `x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; pill: ${l({ ...pills[0], focused: true, quiet: false })};`) },
    { name: "Quiet", body: one("VelvetPill", `x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; pill: ${l({ ...pills[1], focused: false, quiet: true })};`) },
    { name: "Dim", body: one("VelvetPill", `x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; pill: ${l({ ...pills[0], label: "Next", icon: "", focused: true, dim: true })};`) },
    { name: "Danger", body: one("VelvetPill", `x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; pill: ${l({ ...pills[0], label: "Remove folder", icon: "", focused: false, quiet: true, danger: true })};`) },
    { name: "Menu", body: one("VelvetPill", `x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; pill: ${l(home.home.menu)}; size: Tokens.screen.small * ${U};`) },
  ])
  add("atoms", "VelvetQr", [{ name: "Backup", body: one("VelvetQr", `${centre("Math.min(parent.width, parent.height) * 0.6", "self.width")} code: @image-url("../../preview/art/qr.png");`) }])
  add("atoms", "VelvetScrim", [
    { name: "Wide", body: `Rectangle { ${FILL} background: Tokens.light-45; VelvetScrim { width: parent.width; height: parent.height; } }` },
    { name: "Narrow", body: `Rectangle { ${FILL} background: Tokens.light-45; VelvetScrim { width: parent.width; height: parent.height; narrow: true; } }` },
  ])
  add("atoms", "VelvetShape", [{ name: "Play", body: one("VelvetShape", `${centre("parent.height * 0.3", "parent.height * 0.3")} commands: "M4.3 2.4V13.6L13.4 8Z";`) }])
  add("atoms", "VelvetSignal", [0, 1, 2, 3, 4].map(bars => ({ name: `Bars${bars}`, body: one("VelvetSignal", `x: (parent.width - self.width) / 2; y: (parent.height - self.height) / 2; bars: ${bars}; size: Math.min(parent.width, parent.height) * 0.2;`) })))
  add("atoms", "VelvetTap", [{ name: "Target", body: `Rectangle { ${centre("parent.width * 0.3", "parent.height * 0.3")} background: Tokens.light-10; VelvetTap { width: parent.width; height: parent.height; target: "play"; } }` }])
  add("atoms", "VelvetWords", [
    { name: "Fact", body: one("VelvetWords", `x: Tokens.screen.pad-x * ${U}; y: parent.height / 2; text: "Played 2 hours ago";`) },
    { name: "Strong", body: one("VelvetWords", `x: Tokens.screen.pad-x * ${U}; y: parent.height / 2; text: "Ready to resume"; strong: true; ink: Tokens.house-light;`) },
    { name: "Wraps", body: one("VelvetWords", `x: Tokens.screen.pad-x * ${U}; y: parent.height * 0.3; width: parent.width - 2 * self.x; text: "Bring your first games in through a plugin. Once you play, the game you come back to waits for you right here."; wraps: true; size: Tokens.screen.body * ${U};`) },
  ])
}

// ---- Molecules ----
{
  const l = lit("molecules")
  const home = view("home"), bare = view("set-no-art"), set = view("set"), lobby = view("lobby-systems")
  const two = view("detail-two-places").detailButtons as any[]
  const pad = `x: Tokens.screen.pad-x * ${U};`
  add("molecules", "VelvetButtons", [
    { name: "Row", body: one("VelvetButtons", `${pad} y: parent.height / 2; width: parent.width - 2 * self.x; pills: ${l(two)};`) },
    { name: "Stack", body: one("VelvetButtons", `${pad} y: parent.height * 0.3; width: parent.width - 2 * self.x; pills: ${l(two)}; stack: true;`) },
    { name: "Centred", body: one("VelvetButtons", `${pad} y: parent.height / 2; width: parent.width - 2 * self.x; pills: ${l(view("problem").curtainButtons)}; centred: true;`) },
  ])
  add("molecules", "VelvetFactLine", [{ name: "Four", body: one("VelvetFactLine", `${pad} y: parent.height / 2; line: ${l(home.home.mark.facts.first)};`) }])
  add("molecules", "VelvetFacts", [
    { name: "WithBadge", body: one("VelvetFacts", `${pad} y: parent.height / 2; width: parent.width - 2 * self.x; facts: ${l(home.home.mark.facts)}; badge: "PC";`) },
    { name: "TwoLines", body: one("VelvetFacts", `${pad} y: parent.height / 2; width: parent.width - 2 * self.x; facts: ${l(play(scenario("home"), 480, 800).view.home.mark.facts)};`) },
  ])
  const marks: [string, any][] = [["Logo", home.home.mark], ["Title", view("home-no-art").home.mark], ["LongLogo", view("row").home.rowMark]]
  for (const component of ["VelvetMark", "VelvetMarquee"]) {
    add("molecules", component, marks.map(([name, mq]) => ({ name, body: one(component, `${pad} y: Tokens.screen.pad-y * ${U}; width: Tokens.screen.hero-w * ${U}; height: parent.height * 0.7; mq: ${l(mq)};`) })))
  }
  const cards = (bare.setTiles as any[]).filter(tile => tile.kind === "game" && tile.poster.shown)
  add("molecules", "VelvetPoster", cards.slice(0, 8).map((tile, index) => ({ name: `Card${index + 1}`, body: one("VelvetPoster", `${centre("self.height * Tokens.tile-ratio", "parent.height * 0.8")} poster: ${l(tile.poster)};`) })))
  const settings = view("settings").listRows as any[], overlay = view("overlay-groups").rows as any[], wifi = view("wifi").listRows as any[]
  const rowOf = (name: string, row: any) => ({ name, body: one("VelvetRow", `${pad} y: parent.height / 2 - self.height / 2; width: Math.min(parent.width - 2 * self.x, Tokens.screen.page-w * ${U}); it: ${l(row)};`) })
  add("molecules", "VelvetRow", [
    rowOf("Focused", settings.find(row => row.focused)),
    rowOf("Rest", { ...settings.find(row => row.focused), focused: false }),
    rowOf("Head", settings.find(row => row.kind === "head")),
    rowOf("Range", overlay.find(row => row.kind === "range")),
    rowOf("Network", wifi.find(row => row.focusId?.startsWith("w:net:"))),
    rowOf("Secret", settings.find(row => row.focusId?.startsWith("s:plugin-credential:"))),
  ])
  add("molecules", "VelvetSteps", [
    { name: "ThreeOfFour", body: one("VelvetSteps", `x: parent.width * 0.2; y: parent.height / 2; width: parent.width * 0.6; steps: ${l(view("busy-steps").curtainSteps)}; line: ${JSON.stringify(view("busy-steps").curtain.stepLine ?? "Step 3 of 4")};`) },
  ])
  const tile = (name: string, value: any, more = "") => ({ name, body: one("VelvetTile", `${centre("self.height * Tokens.tile-ratio", "parent.height * 0.7")} tile: ${l(value)}; ${more}`) })
  const shown = (set.setTiles as any[]).filter(entry => entry.shown && entry.kind === "game")
  const by = (kind: string) => shown.find(entry => entry.artKind === kind)
  add("molecules", "VelvetTile", [
    tile("Poster", { ...by("poster"), focused: false, lit: false }),
    tile("PosterFocused", { ...by("poster"), focused: true, badge: "PC" }),
    tile("Cover", by("cover")),
    tile("Logo", by("logo")),
    tile("NoArt", cards[0]),
    tile("NoArtFocused", { ...cards[1], lit: true, badge: "NES" }),
    tile("Resting", by("poster"), "rests: true;"),
    tile("System", (lobby.shelfTiles as any[]).find(entry => entry.kind === "system")),
    tile("WayIn", (home.homeRow as any[]).find(entry => entry.kind === "nav")),
  ])
}

// ---- Organisms ----
{
  const l = lit("organisms")
  const curtain = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetCurtain", `${FILL} curtain: ${l(v.curtain)}; kicker: ${l(v.curtainKicker)}; lines: ${l(v.curtainLines)}; steps: ${l(v.curtainSteps)}; buttons: ${l(v.curtainButtons)};`) }
  }
  add("organisms", "VelvetCurtain", [curtain("Steps", "busy-steps"), curtain("Progress", "busy-progress"), curtain("Problem", "problem"), curtain("LongProblem", "problem-long"), curtain("Confirm", "confirm"), curtain("ChooseSave", "choose-save"), curtain("Running", "running")])
  const empty = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetEmpty", `${FILL} empty: ${l(v.empty)}; title: ${l(v.emptyTitle)}; lines: ${l(v.emptyLines)}; buttons: ${l(v.emptyButtons)};`) }
  }
  add("organisms", "VelvetEmpty", [empty("Invitation", "empty"), empty("NoStore", "empty-no-store"), empty("Error", "error"), empty("Loading", "loading")])
  const grid = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetGrid", `x: 0; y: Tokens.screen.lib-top * ${U}; width: parent.width; height: parent.height - self.y; tiles: ${l(v.setTiles)}; marks: ${l(v.setMarks)}; first-row: ${v.set.firstRow};`) }
  }
  add("organisms", "VelvetGrid", [grid("Art", "set"), grid("NoArt", "set-no-art"), grid("OneGame", "set-system")])
  const keyboard = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetKeyboard", `${FILL} keyboard: ${l(v.keyboard)}; keys: ${l(v.keys)};`) }
  }
  add("organisms", "VelvetKeyboard", [keyboard("Text", "settings-text"), keyboard("Symbols", "settings-text-symbols"), keyboard("Secret", "settings-secret"), keyboard("Search", "store-search")])
  const lobby = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetLobby", `x: 0; y: Tokens.screen.lib-top * ${U}; width: parent.width; height: parent.height - self.y; shelves: ${l(v.shelves)}; tiles: ${l(v.shelfTiles)}; shelf-at: ${v.home.shelfAt}; entered: true;`) }
  }
  add("organisms", "VelvetLobby", [lobby("Lately", "lobby"), lobby("NoArt", "lobby-no-art"), lobby("Systems", "lobby-systems")])
  add("organisms", "VelvetNotice", [{ name: "Invite", body: one("VelvetNotice", `x: (parent.width - self.width) / 2; y: parent.height - self.height - Tokens.screen.pad-y * ${U}; notice: ${l(view("invite-notice").notice)};`) }])
  const pause = (component: string, layer: Layer) => (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one(component, `${FILL} title: ${lit(layer)(v.titleLines)}; title-size: ${v.titleSize} * ${U}; clock: ${JSON.stringify(v.clock)}; facts: ${lit(layer)(v.facts)}; rows: ${lit(layer)(v.rows)};`) }
  }
  add("organisms", "VelvetPause", [pause("VelvetPause", "organisms")("Plain", "overlay"), pause("VelvetPause", "organisms")("Groups", "overlay-groups"), pause("VelvetPause", "organisms")("Stream", "overlay-stream")])
  const pick = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetPick", `${FILL} hero: ${v.home.hero}; cards: ${l(v.homeCards)}; mark: ${l(v.home.mark)}; buttons: ${l([v.home.play])};`) }
  }
  const cinema = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetPick", `${FILL} hero: ${v.detail.hero}; cards: ${l(v.detailCards)}; mark: ${l(v.detail.mark)}; buttons: ${l(v.detailButtons)}; stack: ${v.detail.stack}; cinema: true; foot-facts: ${l(v.detail.facts)}; foot-badge: ${JSON.stringify(v.detail.badge)}; note: ${JSON.stringify(v.detail.note)};`) }
  }
  add("organisms", "VelvetPick", [pick("Hero", "home"), pick("NoArt", "home-no-art"), cinema("Cinema", "detail"), cinema("CinemaNoArt", "detail-no-art"), cinema("TwoPlaces", "detail-two-places")])
  const programme = (component: string, layer: Layer) => (name: string, scene: string) => {
    const v = view(scene)
    const ll = lit(layer)
    return { name, body: one(component, `${FILL} page: ${ll(v.list)}; rows: ${ll(v.listRows)}; buttons: ${ll(v.listButtons)}; panel-lines: ${ll(v.listPanelLines)}; tiles: ${ll(v.listTiles)};`) }
  }
  add("organisms", "VelvetProgramme", [["Settings", "settings"], ["Wifi", "wifi"], ["Backup", "identity-backup"], ["Store", "store-results"], ["Offer", "store-offer"], ["FirstBoot", "first-boot"]].map(([name, scene]) => programme("VelvetProgramme", "organisms")(name!, scene!)))
  add("organisms", "VelvetRows", [
    { name: "Settings", body: one("VelvetRows", `x: Tokens.screen.pad-x * ${U}; y: Tokens.screen.pad-y * ${U}; width: Tokens.screen.page-w * ${U}; height: parent.height - 2 * self.y; rows: ${l(view("settings").listRows)};`) },
    { name: "Launch", body: one("VelvetRows", `x: Tokens.screen.pad-x * ${U}; y: Tokens.screen.pad-y * ${U}; width: Tokens.screen.page-w * ${U}; height: parent.height - 2 * self.y; rows: ${l(view("launch-settings").listRows)};`) },
  ])
  const sheet = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetSheet", `${FILL} sheet: ${l(v.sheet)}; rows: ${l(v.sheetRows)};`) }
  }
  add("organisms", "VelvetSheet", [sheet("Sort", "set-sort"), sheet("Menu", "menu"), sheet("Actions", "detail-actions")])
  const stage = (name: string, scene: string) => ({ name, body: one("VelvetStage", `${FILL} stage: ${l(view(scene).stage)};`) })
  add("organisms", "VelvetStage", [stage("Band", "home"), stage("Cinema", "detail"), stage("LightOnly", "home-no-art"), stage("Library", "lobby"), stage("House", "empty")])

  // ---- Pages ----
  const p = lit("pages")
  const browse = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetBrowse", `${FILL} home: ${p(v.home)}; cards: ${p(v.homeCards)}; row-tiles: ${p(v.homeRow)}; shelves: ${p(v.shelves)}; shelf-tiles: ${p(v.shelfTiles)};`) }
  }
  add("pages", "VelvetBrowse", [browse("Home", "home"), browse("HomeNoArt", "home-no-art"), browse("Row", "row"), browse("Lobby", "lobby"), browse("OneGame", "home-one")])
  const detail = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetDetail", `${FILL} detail: ${p(v.detail)}; cards: ${p(v.detailCards)}; buttons: ${p(v.detailButtons)};`) }
  }
  add("pages", "VelvetDetail", [detail("Cinema", "detail"), detail("NoArt", "detail-no-art"), detail("LongTitle", "detail-long-title"), detail("TwoPlaces", "detail-two-places"), detail("Poster", "detail-poster")])
  const house = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetHouse", `${FILL} empty: ${p(v.empty)}; title: ${p(v.emptyTitle)}; lines: ${p(v.emptyLines)}; buttons: ${p(v.emptyButtons)};`) }
  }
  add("pages", "VelvetHouse", [house("Invitation", "empty"), house("Error", "error")])
  add("pages", "VelvetList", [["Settings", "settings"], ["Wifi", "wifi"], ["Identity", "identity"], ["Saves", "saves"], ["Runner", "runner"], ["LaunchSettings", "launch-settings"], ["Invites", "invites"], ["Store", "store"]].map(([name, scene]) => programme("VelvetList", "pages")(name!, scene!)))
  add("pages", "VelvetPaused", [pause("VelvetPaused", "pages")("Plain", "overlay"), pause("VelvetPaused", "pages")("Stream", "overlay-stream")])
  const set = (name: string, scene: string) => {
    const v = view(scene)
    return { name, body: one("VelvetSet", `${FILL} set: ${p(v.set)}; tiles: ${p(v.setTiles)}; marks: ${p(v.setMarks)};`) }
  }
  add("pages", "VelvetSet", [set("EveryGame", "set"), set("NoArt", "set-no-art"), set("Pill", "set-pill"), set("OneSystem", "set-system")])
}

// The backup's code for VelvetQr's preview: Velvet's own encoder draws it.
mkdirSync(join(PACKAGE, "preview/art"), { recursive: true })
writeFileSync(join(PACKAGE, "preview/art/qr.png"), Buffer.from(qrDataUrl("ncryptsec1" + "qpzr".repeat(20)).split(",")[1]!, "base64"))

// Old previews go first, so a component that left takes its preview with it.
for (const dir of ["ui/atoms", "ui/molecules", "ui/organisms", "pages"]) {
  for (const name of readdirSync(join(PACKAGE, dir))) if (name.endsWith(".preview.slint")) rmSync(join(PACKAGE, dir, name))
}
let states = 0
for (const file of files) {
  const dir = file.layer === "pages" ? "pages" : `ui/${file.layer}`
  const ui = file.layer === "pages" ? "../ui/" : "../"
  const path = join(PACKAGE, dir, `${file.component}.${LAYERS[file.layer]}.preview.slint`)
  const text = [
    `// ${file.component}'s preview states, with data from Velvet's scenarios.`,
    "// Written by preview/previews.ts; do not edit by hand.",
    "",
    `import { Tokens } from "${ui}tokens.slint";`,
    `import { VelvetPreview } from "${ui}preview-frame.slint";`,
    `import { ${file.component} } from "${file.component}.slint";`,
    "",
    ...file.states.map(state => `export component ${state.name} inherits VelvetPreview {\n    ${state.body}\n}\n`),
  ].join("\n")
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, text)
  states += file.states.length
}
console.log(`${files.length} preview files, ${states} states`)
