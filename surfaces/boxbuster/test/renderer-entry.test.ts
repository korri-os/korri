import { describe, expect, test } from "bun:test"
import { createRequire } from "node:module"
import { dirname, resolve } from "node:path"
import * as renderer from "@react-three/fiber"

const require = createRequire(import.meta.url)
const packagePath = require.resolve("@react-three/fiber/package.json")
const metadata: { module: string } = require(packagePath)
const productionRenderer = await import(
  resolve(dirname(packagePath), metadata.module)
)

describe("the test renderer entry", () => {
  test("loads the real exports from the ESM entry used by the production bundler", () => {
    expect(renderer.Canvas).toBe(productionRenderer.Canvas)
    expect(renderer.createRoot).toBe(productionRenderer.createRoot)
    expect(renderer.useFrame).toBe(productionRenderer.useFrame)
    expect(renderer.Canvas).toBeFunction()
    expect(renderer.createRoot).toBeFunction()
    expect(renderer.useFrame).toBeFunction()
  })
})
