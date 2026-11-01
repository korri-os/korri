/**
 * Every preview compiles and draws its component, at every one of Pico's
 * five sizes (the decomposition gate checks that each one exists, imports
 * its component and draws it in source; this renders them).
 */
import { expect, test } from "bun:test"
import { mkdtempSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { FIVE_SIZES, PROBE, probe } from "./probe"

test.skipIf(PROBE === undefined)("every preview state renders something at every size", () => {
  const out = mkdtempSync(join(tmpdir(), "pico-previews-"))
  const run = probe(["previews", "pico", "--sizes", FIVE_SIZES, "--out", out])
  expect(run.out.split("\n").filter(line => line.startsWith("FAIL"))).toEqual([])
  expect(run.code).toBe(0)
}, 600_000)
