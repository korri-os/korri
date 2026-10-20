/**
 * Every state of the OPTIONS and RUN WITH parts renders, and renders words.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { cleanup, render } from "@testing-library/react"
import type { ComponentType } from "react"
import * as options from "../src/pages/ZoneOptions.page.part"
import * as runWith from "../src/pages/ZoneRunWith.page.part"
import * as entry from "../src/ui/organisms/ZoneOptionsEntry.organism.part"
import * as route from "../src/ui/organisms/ZoneRunWithRoute.organism.part"
import * as warnings from "../src/ui/organisms/ZoneWarningLines.organism.part"

afterEach(cleanup)

const statesOf = (part: Record<string, unknown>): [string, ComponentType][] =>
  Object.entries(part).filter(([key, value]) => /^[A-Z]/.test(key) && typeof value === "function") as [string, ComponentType][]

describe.each([
  ["ZoneOptions", options],
  ["ZoneRunWith", runWith],
  ["ZoneOptionsEntry", entry],
  ["ZoneRunWithRoute", route],
  ["ZoneWarningLines", warnings],
] as const)("%s", (_, part) => {
  test.each(statesOf(part))("%s puts words on the page", (_state, State) => {
    const { container } = render(<State />)
    expect((container.textContent ?? "").trim().length).toBeGreaterThan(0)
  })
})
