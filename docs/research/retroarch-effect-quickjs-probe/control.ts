// Native Bun control only, NOT evidence that these decoders run in QuickJS.
import { decodeRetroArchPolicy } from "../../../plugins/retroarch/policy"
import { renderRetroArchSettings } from "../../../plugins/retroarch/render-settings"
import { readFileSync } from "node:fs"
import { isSchemaError } from "../../../plugins/retroarch/node_modules/effect/src/Schema"

const cases: [string, unknown, boolean, Record<string, unknown>][] = JSON.parse(readFileSync(process.argv[2], "utf8"))
let failed = false
for (const [name, input, accepted, expectedPairs] of cases) {
  let output: { ok: true; pairs: Record<string, unknown> } | { ok: false; errorKind: string; error: string }
  try { output = { ok: true, pairs: Object.fromEntries(renderRetroArchSettings(decodeRetroArchPolicy(input))) } }
  catch (error) { output = { ok: false, errorKind: isSchemaError(error) ? "SchemaError" : error instanceof Error ? error.name : "unknown", error: String(error) } }
  const passed = output.ok === accepted && (output.ok
    ? Object.entries(expectedPairs).every(([k, v]) => output.ok && output.pairs[k] === v)
    : output.errorKind === "SchemaError" && !output.error.includes("ReferenceError"))
  console.log(JSON.stringify({ engine: `Bun ${Bun.version} (control, not QuickJS)`, case: name, passed, output }))
  failed ||= !passed
}
if (failed) process.exitCode = 1
