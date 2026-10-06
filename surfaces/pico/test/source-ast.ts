/**
 * Source parsing for Pico's gates.
 *
 * TypeScript 7 no longer exports a compiler API from the `typescript` package,
 * so the gates parse with oxc, the parser under oxlint. The tree is ESTree with
 * TypeScript extensions; `visitNodes` visits every node in source order.
 */
import { parseSync, type Node, type Program } from "oxc-parser"

export type { Node, Program }

/** Parse one module. A gate must not judge a file it could not read. */
export function parseModule(file: string, text: string): Program {
  const { program, errors } = parseSync(file, text)
  if (errors.length > 0) {
    throw new Error(`${file}: ${errors.map(error => error.message).join("; ")}`)
  }
  return program
}

function isNode(value: unknown): value is Node {
  return typeof value === "object" && value !== null
    && typeof (value as { type?: unknown }).type === "string"
}

/** Visit `root` and every node below it, parents before children. */
export function visitNodes(root: Node, visit: (node: Node) => void): void {
  visit(root)
  for (const [key, value] of Object.entries(root)) {
    if (key === "parent") continue
    if (Array.isArray(value)) {
      for (const item of value) if (isNode(item)) visitNodes(item, visit)
    } else if (isNode(value)) {
      visitNodes(value, visit)
    }
  }
}

/** The text a string-like node renders: literals, template parts and JSX text. */
export function renderedText(node: Node): string | undefined {
  switch (node.type) {
    case "Literal":
      return typeof node.value === "string" ? node.value : undefined
    case "TemplateElement":
      return node.value.cooked ?? node.value.raw
    case "JSXText":
      return node.value
    default:
      return undefined
  }
}
