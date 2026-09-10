import { describe, expect, it } from "bun:test"
import type { SchemaAST } from "effect"
import { RetroArchPolicy, decodeRetroArchPolicy } from "./policy"
import { renderRetroArchSettings } from "./render-settings"
import types from "./settings-types.json"

// Exercise every fixed field in the real schema. Dynamic input ports stay
// renderer outputs too, but need their own source evidence before admission.
function sample(ast: SchemaAST.AST): unknown {
  switch (ast._tag) {
    case "Union": return sample(ast.types[0])
    case "Objects": return Object.fromEntries(
      ast.propertySignatures.map((property) => [property.name, sample(property.type)]),
    )
    case "Arrays": return []
    case "String": return "https://example.invalid/value"
    case "Number": return 1
    case "Boolean": return false
    case "Literal": return ast.literal
    default: throw new Error(`Unhandled policy field: ${ast._tag}`)
  }
}

describe("legacy nested policy renderer", () => {
  it("keeps the kind key/type table equal to the actual renderer outputs", () => {
    const policy = decodeRetroArchPolicy(sample(RetroArchPolicy.ast))
    const rendered = renderRetroArchSettings(policy)
    const table = Object.fromEntries(rendered.map(([key, value]) => [
      key,
      typeof value === "boolean" ? "Boolean" : typeof value === "number" ? "Number" : "String",
    ]))
    expect(table).toEqual(types)
  })

  it("preserves nested names, enum conversion, nullable omission and input ports", () => {
    const pairs = Object.fromEntries(renderRetroArchSettings(decodeRetroArchPolicy({
      video: { aspectRatio: "core-provided", vsync: false, shader: null },
      input: { quitGamepadCombo: "start-select", ports: { "2": { joypadIndex: 1 } } },
      audio: { device: 'hw:"quoted"', volumeDb: -3 },
    })))
    expect(pairs.aspect_ratio_index).toBe(22)
    expect(pairs.input_quit_gamepad_combo).toBe(4)
    expect(pairs.input_player2_joypad_index).toBe(1)
    expect(pairs.video_vsync).toBe(false)
    expect(pairs.audio_device).toBe('hw:"quoted"')
    expect(pairs.audio_volume).toBe(-3)
    expect(pairs).not.toHaveProperty("video_shader")
  })

  it("rejects flat cfg keys, wrong nested types and legacy range violations", () => {
    for (const value of [
      { video_vsync: true }, { video: { vsync: "true" } },
      { input: { analogDeadzone: 2 } }, { updater: { buildbotUrl: "http://example.invalid" } },
    ]) expect(() => decodeRetroArchPolicy(value)).toThrow()
  })
})
