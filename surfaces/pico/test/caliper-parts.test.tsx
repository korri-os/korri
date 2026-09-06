import { expect, test } from "bun:test"
import { readdirSync } from "node:fs"
import { createElement, isValidElement, type ReactElement } from "react"
import { renderPicoPart } from "../caliper/render-part"

const root = new URL("../src/", import.meta.url)
const parts = readdirSync(root, { recursive: true, encoding: "utf8" }).filter(file => file.endsWith(".part.tsx"))

test.each(parts)("%s is a pure element factory usable outside React render", async file => {
  const part = await import(new URL(file, root).href) as { default: () => ReactElement }
  // Deliberately outside a React render: adding a Hook to a part factory must
  // fail here. Hooks belong to the product component returned by the factory.
  const element = renderPicoPart({
    render: () => createElement(part.default), generatedContract: { inputs: [] },
  }, { inputValues: {} })
  expect(isValidElement(element)).toBe(true)
})

test("Inspector overrides preserve arrays, children and callbacks from the authored root", () => {
  const actions = [{ id: "go" }]
  const onClick = () => undefined
  const FixtureRoot = (_props: Record<string, unknown>) => null
  const Factory = () => createElement(FixtureRoot, { actions, onClick, tone: "info" }, "AUTHORED")
  const result = renderPicoPart({ render: () => createElement(Factory),
    generatedContract: { inputs: [{ id: "tone" }] },
  }, { inputValues: { tone: "warn", actions: [], onClick: null, children: "WRONG" } }) as ReactElement<Record<string, unknown>>
  expect(result.type).toBe(FixtureRoot)
  expect(result.props.actions).toBe(actions)
  expect(result.props.onClick).toBe(onClick)
  expect(result.props.children).toBe("AUTHORED")
  expect(result.props.tone).toBe("warn")
})

test("missing overrides retain authored values rather than inserting synthesized defaults", () => {
  const Factory = () => createElement("div", { title: "AUTHORED" })
  const result = renderPicoPart({ render: () => createElement(Factory),
    generatedContract: { inputs: [{ id: "title" }] },
  }, { inputValues: {} }) as ReactElement<{ title: string }>
  expect(result.props.title).toBe("AUTHORED")
})
