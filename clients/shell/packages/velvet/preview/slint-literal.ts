/**
 * A view value as a Slint literal, for the preview states
 * (preview/previews.ts) and the tests: keys in kebab case, an image as
 * `@image-url`, relative to the file that holds it.
 */
const kebab = (name: string): string => name.replace(/[A-Z]/g, c => `-${c.toLowerCase()}`)

/** `up` is the path from the preview file back to the package root ("../../"). */
export function literal(value: unknown, up: string): string {
  if (value === null || value === undefined) return '""'
  if (typeof value === "string") return JSON.stringify(value).replace(/\\u([0-9a-f]{4})/gi, (_, hex: string) => `\\u{${hex.replace(/^0+/, "") || "0"}}`)
  if (typeof value === "number") return Number.isInteger(value) ? String(value) : String(Math.round(value * 1000) / 1000)
  if (typeof value === "boolean") return value ? "true" : "false"
  if (Array.isArray(value)) return `[${value.map(item => literal(item, up)).join(", ")}]`
  const entry = value as Record<string, unknown>
  // The empty picture (a data URL in a view) is the property's own default here.
  if (typeof entry.$image === "string") return entry.$image === "" || entry.$image.startsWith("data:") ? '@image-url("")' : `@image-url("${up}${entry.$image}")`
  // A field at its type's default is left out: a struct literal fills it in.
  const fields = Object.entries(entry)
    .filter(([key]) => key !== "focusRegion" && key !== "focusActive" && key !== "focusPlan")
    .map(([key, inner]) => [key, literal(inner, up)] as const)
    .filter(([, text]) => text !== "false" && text !== "0" && text !== '""' && text !== "[]" && text !== "{  }" && text !== '@image-url("")')
    .map(([key, text]) => `${kebab(key)}: ${text}`)
  return `{ ${fields.join(", ")} }`
}
