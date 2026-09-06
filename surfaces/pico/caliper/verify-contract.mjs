#!/usr/bin/env node
/** Validate against the installed Caliper, without a runtime/package dependency. */
import assert from "node:assert/strict"
import { mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs"
import { createRequire } from "node:module"
import { join } from "node:path"
import { spawnSync } from "node:child_process"
import { fileURLToPath } from "node:url"

assert(process.env.CALIPER_ROOT, "Set CALIPER_ROOT to the Caliper checkout used by the launcher")
const contract = realpathSync(join(process.env.CALIPER_ROOT, "src/lab/surface-registry.ts"))
const directory = fileURLToPath(new URL("./", import.meta.url))
const temporary = mkdtempSync(join(directory, ".contract-"))
const tsc = createRequire(import.meta.url).resolve("typescript/bin/tsc")
try {
  writeFileSync(join(temporary, "tsconfig.json"), JSON.stringify({
    extends: "../../tsconfig.json", include: ["check.ts"],
  }))
  const preamble = `import type { LabSurfaceAdapter } from ${JSON.stringify(contract)}\n`
  const run = () => spawnSync(process.execPath, [tsc, "-p", join(temporary, "tsconfig.json")], { encoding: "utf8" })
  writeFileSync(join(temporary, "check.ts"), `${preamble}const adapter: LabSurfaceAdapter = { id: "tripwire" }\nvoid adapter\n`)
  const red = run()
  assert(red.status !== 0 && red.stdout.includes("missing the following properties"),
    `Contract tripwire did not produce the expected missing-fields error:\n${red.stdout}${red.stderr}`)
  writeFileSync(join(temporary, "check.ts"), `${preamble}import { picoAdapter } from "../adapter"\nconst adapter: LabSurfaceAdapter = picoAdapter\nvoid adapter\n`)
  const green = run()
  assert.equal(green.status, 0, `${green.stdout}${green.stderr}`)
  console.log("PASS: rejected the incomplete adapter, accepted Pico against the launcher's actual contract")
} finally {
  rmSync(temporary, { recursive: true, force: true })
}
