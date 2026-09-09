import { expect, test } from "bun:test"
import { createRequire } from "node:module"
import * as React from "react"

const requireFromShift = createRequire(
  new URL("../../../surfaces/shift/package.json", import.meta.url),
)

test("the installed surface and the portal share one React runtime", async () => {
  const surfaceReact = await import(requireFromShift.resolve("react"))
  expect(surfaceReact.useState).toBe(React.useState)
  expect(surfaceReact.useEffect).toBe(React.useEffect)
})
