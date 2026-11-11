import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import type { KorriModel, SurfaceMessage, SurfaceRequest } from "../src/model"
import { picoLaunchViewFromModel } from "../src/pico-launch-view"
import { rowView } from "../src/native/panel"
import { catalogScreen, init, update } from "../src/native/program"
import { fixtureModel } from "./fixtures/fixture-host"

// Extracted, without changing rows/IDs/choices, from korrid's real controller
// export in one_launch_editor_is_ephemeral_and_keep_writes_only_the_chosen_value.
// Retain four layers, two launch scopes and the Video/Keep groups. This is an
// editor fixture, not a claim that the fixture game's actual runner supports 4x.
function launchFixture() {
  const value: NonNullable<KorriModel["launchSettings"]> = JSON.parse(
    readFileSync(new URL("./fixtures/shared-launch-settings.json", import.meta.url), "utf8"),
  )
  if (value._tag !== "Ready") throw new Error("Expected the controller's ready editor")
  return value
}
const launchSettings = launchFixture()
const launchPlace = launchSettings.places.find(place => place.layer.id === "launch" && place.scope.kind === "any")!
const items = launchPlace.groups.flatMap(group => group.items)
const scale = items.find(item => item.interaction?.kind === "number")!
const keep = items.find(item => item.id.startsWith("launch:keep:"))!
const add = items.find(item => item.label === "Add setting")!

function surface(width = 640, height = 480) {
  const games = fixtureModel.catalog._tag === "Ready" ? [...fixtureModel.catalog.games] : []
  const model: KorriModel = {
    ...fixtureModel,
    catalog: { _tag: "Ready", total: games.length, window: games, firstIndex: 0, query: { text: "", order: "korri" } },
    launchSettings,
  }
  let state = init(model, {})
  const requests: SurfaceRequest[] = []
  const send = (message: SurfaceMessage) => {
    const next = update(state, message, model)
    state = next.state
    requests.push(...next.requests)
    return next
  }
  send({ _tag: "Resized", width, height })
  send({ _tag: "KorriPublished" })
  const tap = (target: string) => send({ _tag: "Activated", target })
  const launch = picoLaunchViewFromModel({ ...fixtureModel, launchSettings })!
  tap(`spine:${launch.layers.findIndex(layer => layer.id === "launch")}`)
  requests.length = 0 // Ignore catalog focus requests from initial publication/resizing.
  return { tap, send, requests, screen: () => catalogScreen(state, model), state: () => state }
}

describe("Pico shared launch settings", () => {
  test("keeps the published layers, scopes and shared setting facts", () => {
    const launch = picoLaunchViewFromModel({ ...fixtureModel, launchSettings })!
    expect(launch.layers.map(layer => layer.title)).toEqual(["Device", "System gba", "Game", "This launch"])
    const once = launch.layers.at(-1)!
    expect(once.scopes.map(scope => scope.label)).toEqual(["Any runner", "mGBA"])
    expect(once.scopes[0].rows.find(row => row.id === scale.id)).toMatchObject({ label: "Render scale", value: "4", deviceOnly: true })
    const retained = once.scopes[0].rows.find(row => row.id === keep.id)!
    expect(rowView(retained, false)).toMatchObject({ label: "Keep Render scale", value: "4", description: "Render scale: 4x. Choose where to keep it." })
  })

  for (const [width, height] of [[640, 480], [480, 800], [1024, 240], [1024, 768]]) {
    test(`Keep offers Game first and sends the exact published request at ${width}x${height}`, () => {
      const pico = surface(width, height)
      expect(pico.screen().targets.some(target => target.id === `row:${keep.id}`)).toBe(true)
      pico.tap(`row:${keep.id}`)
      expect(pico.requests).toEqual([])
      expect(pico.screen().targets[0].id).toBe("pick:game:wario/any")
      expect(pico.state().focus).toBe("pick:game:wario/any")
      pico.send({ _tag: "Pressed", button: "confirm" })
      expect(pico.requests).toEqual([{ _tag: "ChangeSetting", settingId: keep.id, value: "game:wario/any" }])
    })
  }

  test("Keep can choose another published place without inventing a destination", () => {
    const pico = surface()
    pico.tap(`row:${keep.id}`)
    pico.tap("pick:device/any")
    expect(pico.requests).toEqual([{ _tag: "ChangeSetting", settingId: keep.id, value: "device/any" }])
  })

  test("adding a shared value stays on the existing setting request path", () => {
    const pico = surface()
    pico.tap(`row:${add.id}`)
    pico.tap("pick:video.aspect-ratio")
    expect(pico.requests).toEqual([{ _tag: "ChangeSetting", settingId: add.id, value: "video.aspect-ratio" }])
  })

  test("a one-launch number uses Pico's existing editor and exact opaque ID", () => {
    const pico = surface()
    pico.tap(`row:${scale.id}`)
    expect(pico.screen().view).toMatchObject({ page: "launch", mode: "editor", editor: { text: "4" } })
    pico.tap("key:clear")
    pico.tap("key:2:8") // The existing keyboard's digit 2.
    pico.tap("answer:0")
    expect(pico.requests).toEqual([{ _tag: "ChangeSetting", settingId: scale.id, value: "2" }])
  })

  test("a stale Keep target is ignored", () => {
    const pico = surface()
    pico.tap("row:launch:keep:stale:video.render-scale")
    expect(pico.requests).toEqual([])
  })
})
