/**
 * The cluster at the top right of every screen (ADR 0094): what it says of
 * Korri's model, the order it gives way in, and the pause panel's title that
 * shares its line. ui/organisms/VelvetCluster.slint draws it with the widths
 * held here; tests/velvet_package.rs draws it at every size.
 */
import { describe, expect, test } from "bun:test"
import { CELL_W, CLUSTER_GAP, BOLT_W, clusterFits, clusterIn, clusterOf, LOW_PERCENT, NETWORK_W, NO_CLUSTER, openClusterReserve, PERCENT_GAP } from "../src/cluster"
import { textWidth } from "../src/fit"
import { barsOf } from "../src/settings"
import { Play, play, scenario, scenarios } from "./korri"

const wifi = (strengthPercent?: number) => ({ connection: { _tag: "Wifi" as const, ssid: "attic", ...(strengthPercent === undefined ? {} : { strengthPercent }) }, wifi: { _tag: "NoAdapter" as const } })
const of = (more: object) => clusterOf({ clockLabel: "21:47", ...more })

describe("what the cluster says", () => {
  test("Wi-Fi: 0 to 4 bars, the Wi-Fi page's mapping", () => {
    for (const strength of [0, 1, 24, 25, 49, 50, 74, 75, 100]) {
      const cluster = of({ network: wifi(strength) })
      expect(cluster.network).toBe("bars")
      expect(cluster.bars).toBe(barsOf(strength))
    }
    expect([0, 12, 30, 60, 92].map(strength => of({ network: wifi(strength) }).bars)).toEqual([0, 1, 2, 3, 4])
  })

  test("Wi-Fi with no strength is the plain mark, never a weak signal; wired, offline, none", () => {
    expect(of({ network: wifi() })).toMatchObject({ network: "wifi", bars: 0 })
    expect(of({ network: { connection: { _tag: "Wired" }, wifi: { _tag: "NoAdapter" } } }).network).toBe("wired")
    expect(of({ network: { connection: { _tag: "Offline" }, wifi: { _tag: "NoAdapter" } } }).network).toBe("offline")
    expect(of({}).network).toBe("none")
  })

  test("the battery: a cell and its percent; a bolt while it charges; in Exit at 15 % and under, unless it charges", () => {
    expect(of({ battery: { percent: 82, charging: false } })).toMatchObject({ battery: "cell", percent: "82%", share: 0.82, low: false })
    expect(of({ battery: { percent: LOW_PERCENT, charging: false } })).toMatchObject({ battery: "cell", percent: "15%", low: true })
    expect(of({ battery: { percent: 16, charging: false } }).low).toBe(false)
    expect(of({ battery: { percent: 12, charging: false } })).toMatchObject({ battery: "cell", percent: "12%", low: true })
    expect(of({ battery: { percent: 12, charging: true } })).toMatchObject({ battery: "bolt", percent: "12%", low: false })
  })

  test("no percent: a bolt alone while it charges, else nothing; no battery: nothing", () => {
    expect(of({ battery: { charging: true } })).toMatchObject({ battery: "bolt", percent: "", low: false })
    expect(of({ battery: { charging: false } })).toMatchObject({ battery: "none", percent: "" })
    expect(of({})).toMatchObject({ battery: "none", percent: "" })
  })

  test("a percent outside 0 to 100 is held to it", () => {
    expect(of({ battery: { percent: 140, charging: false } }).percent).toBe("100%")
    expect(of({ battery: { percent: -3, charging: false } })).toMatchObject({ percent: "0%", low: true })
  })

  test("nothing to say draws nothing", () => {
    expect(clusterOf({})).toEqual(NO_CLUSTER)
    expect(clusterOf({ clockLabel: "" })).toEqual(NO_CLUSTER)
    expect(clusterOf({ network: wifi(80) }).shown).toBe(true)
  })

  test("its widths are the sum of its parts", () => {
    const cluster = of({ network: wifi(80), battery: { percent: 82, charging: false } })
    const least = NETWORK_W + CLUSTER_GAP + CELL_W
    expect(cluster.leastW).toBeCloseTo(least)
    expect(cluster.midW).toBeCloseTo(textWidth("21:47") + CLUSTER_GAP + least)
    expect(cluster.fullW).toBeCloseTo(cluster.midW + PERCENT_GAP + textWidth("82%"))
    expect(of({ battery: { charging: true } }).leastW).toBeCloseTo(BOLT_W)
  })
})

describe("a short room: the percent gives way first, then the clock; the glyphs stay", () => {
  const cluster = of({ network: wifi(80), battery: { percent: 82, charging: false } })
  test("each threshold, and just under it", () => {
    expect(clusterFits(cluster, cluster.fullW)).toEqual({ percent: true, clock: true, width: cluster.fullW })
    expect(clusterFits(cluster, cluster.fullW - 0.01)).toEqual({ percent: false, clock: true, width: cluster.midW })
    expect(clusterFits(cluster, cluster.midW - 0.01)).toEqual({ percent: false, clock: false, width: cluster.leastW })
    expect(clusterFits(cluster, 0)).toEqual({ percent: false, clock: false, width: cluster.leastW })
  })

  test("clusterIn keeps only what stands, at one width", () => {
    const glyphs = clusterIn(cluster, 1)
    expect(glyphs).toMatchObject({ shown: true, clock: "", percent: "", network: "bars", battery: "cell", share: 0.82 })
    expect(glyphs.fullW).toBeCloseTo(cluster.leastW)
    expect(clusterIn(cluster, cluster.midW)).toMatchObject({ clock: "21:47", percent: "" })
    expect(clusterIn(cluster, 99)).toMatchObject({ clock: "21:47", percent: "82%" })
    // A clock alone in no room: nothing stands.
    expect(clusterIn(of({}), 1)).toEqual(NO_CLUSTER)
  })
})

describe("the pause panel", () => {
  test("the Battery readout went; the stream's readouts stay; the battery is the cluster's", () => {
    const run = play(scenario("overlay-stream"), 1920, 1080)
    const labels = (run.view.readouts as { label: string }[]).map(readout => readout.label)
    expect(labels).not.toContain("Battery")
    expect(labels).toEqual(expect.arrayContaining(["Latency", "Stream", "Frames dropped"]))
    expect(run.view.cluster).toMatchObject({ clock: "21:47", network: "bars", battery: "cell", percent: "82%" })
  })

  test("at every size the cluster takes at most its share of the line, and the title the rest", () => {
    for (const [width, height] of [[1920, 1080], [1280, 720], [640, 480], [480, 800], [320, 240]] as const) {
      const run = play(scenario("status-low-pause"), width, height)
      const cluster = run.view.cluster
      expect(cluster.shown, `${width}x${height}`).toBe(true)
      expect(cluster.network).toBe("bars")
      expect(cluster.battery).toBe("cell")
      expect(cluster.low).toBe(true)
    }
    // The cluster's width is what the logic fitted it to: never more than its share of the line.
    for (const [width, height] of [[1920, 1080], [640, 480], [320, 240]] as const) {
      const run = play(scenario("status-low-pause"), width, height)
      const p = run.view.screen
      const line = p.pauseW - 2 * Math.min(p.padX, p.pauseW * 0.1)
      expect(run.view.cluster.fullW * p.small, `${width}x${height}`).toBeLessThanOrEqual(line * 0.45)
    }
  })
})

describe("every screen carries it, the Floor's own window does not repeat it", () => {
  test("home, a set, the detail, a list page, the empty library", () => {
    for (const name of ["home", "set", "detail", "settings", "empty", "status-offline", "status-wired"]) {
      expect(play(scenario(name), 1280, 720).view.cluster.shown, name).toBe(true)
    }
    expect(play(scenario("status-nothing"), 1280, 720).view.cluster.shown).toBe(false)
  })

  test("home at every depth: the pick, the row and the lobby; the row's facts leave the cluster its room", () => {
    for (const [width, height] of [[1920, 1080], [640, 480], [480, 800], [320, 240]] as const) {
      for (const name of ["home", "status-row", "status-lobby"]) {
        expect(play(scenario(name), width, height).view.cluster.shown, `${name} ${width}x${height}`).toBe(true)
      }
      const run = play(scenario("status-row"), width, height)
      const p = run.view.screen
      const reserve = openClusterReserve(run.model, p.width - 2 * p.padX, p.small)
      const facts = run.view.home.rowMark.facts
      for (const line of [facts.first, facts.second].filter(Boolean)) {
        const words = ["a", "b", "c", "d"].slice(0, line.count).map(key => line[key].text as string)
        // Each fact is its icon, its words and a gap: the line's words alone stay inside the room.
        const used = words.reduce((sum, text) => sum + textWidth(text) * p.fact, 0)
        expect(used, `${width}x${height}: ${words.join(", ")}`).toBeLessThanOrEqual(p.rowMarkW - reserve)
      }
    }
  })

  test("with a window each (ADR 0086), the Stage's holds it and the Floor's does not", () => {
    const run = new Play({ ...scenario("home").models[0], windows: [{ panes: ["stage"] }, { panes: ["floor"] }] }, 1920, 1080)
    run.press("down")
    expect((globalThis.surface.view(run.state, run.model, 0) as any).cluster.shown).toBe(true)
    expect((globalThis.surface.view(run.state, run.model, 1) as any).cluster.shown).toBe(false)
  })

  test("every scenario's cluster is one of the kinds the view draws", () => {
    for (const scene of scenarios()) {
      const cluster = play(scene, 1280, 720).view.cluster
      expect(["bars", "wifi", "wired", "offline", "none"]).toContain(cluster.network)
      expect(["cell", "bolt", "none"]).toContain(cluster.battery)
    }
  })
})
