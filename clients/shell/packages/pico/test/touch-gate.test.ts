/**
 * Touch reaches the root (ADR 0055): every component that declares the
 * `activated` (or `dragged`) callback has it forwarded on every instance, up
 * to main.slint's roots, whatever name the instance is imported under. An
 * instance that drops it leaves its targets drawn but untappable: the PLAY
 * and action rows of a game's own screen were, through `PicoGameDetail as
 * GameDetail`, until this gate.
 */
import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative, resolve } from "node:path"

const ROOT = join(import.meta.dir, "..")

function slintFiles(dir = ROOT): string[] {
  const out: string[] = []
  for (const entry of readdirSync(dir)) {
    if (["node_modules", "preview", "test", "src", "fonts"].includes(entry) || entry.startsWith(".")) continue
    const path = join(dir, entry)
    if (statSync(path).isDirectory()) out.push(...slintFiles(path))
    else if (entry.endsWith(".slint") && !entry.endsWith(".preview.slint")) out.push(path)
  }
  return out
}

const code = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "")

/** The components each file declares that carry `callback NAME(`, and those they inherit it from. */
function declaring(files: readonly string[], name: string): Set<string> {
  const found = new Set<string>()
  const inherits = new Map<string, string>()
  for (const file of files) {
    const text = code(readFileSync(file, "utf8"))
    for (const match of text.matchAll(/component\s+([\w-]+)(?:\s+inherits\s+([\w-]+))?\s*\{/g)) {
      if (match[2] !== undefined) inherits.set(match[1]!, match[2])
    }
    const component = /component\s+([\w-]+)/.exec(text)?.[1]
    if (component !== undefined && new RegExp(`callback\\s+${name}\\s*\\(`).test(text)) found.add(component)
  }
  let grew = true
  while (grew) {
    grew = false
    for (const [child, parent] of inherits) if (found.has(parent) && !found.has(child)) {
      found.add(child)
      grew = true
    }
  }
  return found
}

/** Every instance of a declaring component (by its local name) that does not forward the callback. */
function unforwarded(files: readonly string[], name: string): string[] {
  const carriers = declaring(files, name)
  const missing: string[] = []
  for (const file of files) {
    const text = code(readFileSync(file, "utf8"))
    // Local names: each import, aliases included.
    const local = new Map<string, string>()
    for (const match of text.matchAll(/import\s*\{([^}]*)\}\s*from\s*"([^"]+)"/g)) {
      for (const part of match[1]!.split(",")) {
        const [imported, alias] = part.trim().split(/\s+as\s+/)
        if (imported) local.set((alias ?? imported).trim(), imported.trim())
      }
    }
    for (const match of text.matchAll(/(?:^|[\s:=])([A-Z][\w-]*)\s*\{/g)) {
      const lineStart = text.lastIndexOf("\n", match.index!) + 1
      if (/\b(component|struct|inherits|global)\b/.test(text.slice(lineStart, match.index! + match[0].length))) continue
      const component = local.get(match[1]!) ?? match[1]!
      if (!carriers.has(component)) continue
      // The instance's own bindings: up to its first child element, or 600 characters.
      const at = match.index! + match[0].length
      const rest = text.slice(at, at + 600)
      const child = rest.search(/\n\s*(?:[\w-]+\s*:=\s*)?(?:(?:if|for)\b[^\n{]*?:\s*)?[A-Z][\w-]*\s*\{/)
      const head = child === -1 ? rest : rest.slice(0, child)
      if (!new RegExp(`\\b${name}\\s*\\(`).test(head)) {
        missing.push(`${relative(ROOT, file)}: ${match[1]} (${component}) does not forward ${name}`)
      }
    }
  }
  return missing
}

const files = [...slintFiles(), resolve(ROOT, "main.slint")].filter((file, i, all) => all.indexOf(file) === i)

describe("touch reaches the root", () => {
  for (const name of ["activated", "dragged"]) {
    test(`every instance of a component with \`${name}\` forwards it`, () => {
      expect(unforwarded(files, name)).toEqual([])
    })
  }

  test("both roots declare the callbacks the shell binds", () => {
    const main = code(readFileSync(join(ROOT, "main.slint"), "utf8"))
    for (const root of ["Surface", "Overlay"]) {
      const body = main.slice(main.indexOf(`export component ${root}`))
      expect(body).toMatch(/callback\s+activated\s*\(\s*target:\s*string,\s*at:\s*float,\s*released:\s*bool\s*\)/)
      expect(body).toMatch(/callback\s+dragged\s*\(\s*region:\s*string,\s*by:\s*int\s*\)/)
    }
  })
})

describe("the gesture rules", () => {
  const read = (file: string) => code(readFileSync(join(ROOT, file), "utf8"))
  const tap = read("ui/atoms/PicoTap.slint")
  const finish = tap.slice(tap.indexOf("function finish"), tap.indexOf("pointer-event"))

  test("a cancel completes nothing: no activation, no released, no fling; the scroll region stops holding", () => {
    expect(tap).toMatch(/event\.kind == PointerEventKind\.cancel\) \{\s*finish\(false\);/)
    const cancel = finish.slice(finish.indexOf("if (!lifted)"), finish.indexOf("} else if (wakes)"))
    expect(cancel).toMatch(/root\.holding\(false\)/)
    expect(cancel).toMatch(/root\.slid\(false\)/)
    expect(cancel).not.toMatch(/activated|slid\(true\)/)
    // A rack told `slid(false)` settles with no fling.
    expect(read("ui/atoms/PicoSlide.slint")).toMatch(/public function release\(fling: bool\) \{\s*held = false;\s*if \(!fling \|\|/)
  })

  test("a lift this area did not see begin completes nothing (a finger that went down elsewhere)", () => {
    expect(tap).toMatch(/event\.kind == PointerEventKind\.down\) \{\s*armed = true;/)
    expect(finish).toMatch(/^function finish\(lifted: bool\) \{\s*if \(!armed\) \{/)
    expect(finish).toMatch(/armed = false;/)
    expect(tap).toMatch(/moved => \{\s*if \(!armed\) \{\s*return;/)
  })

  test("the attract's cover wakes on any lift, a tap or a drag", () => {
    expect(finish).toMatch(/\} else if \(wakes\) \{\s*root\.activated\(target, -1, false\);/)
    expect(read("ui/organisms/PicoAttract.slint")).toMatch(/wakes: true;/)
  })

  test("no target sends `at` but a range: no chip halves by `at`", () => {
    expect(tap).not.toMatch(/halves/)
    const sends = [...finish.matchAll(/root\.activated\(target, ([^,]+),/g)].map(match => match[1])
    expect(sends.filter(at => at !== "-1")).toEqual(["along(self.mouse-x)", "along(self.mouse-x)"])
  })
})

describe("a rack under a finger", () => {
  const read = (file: string) => code(readFileSync(join(ROOT, file), "utf8"))
  const slide = read("ui/atoms/PicoSlide.slint")

  test("follows the finger exactly: the row stands at the finger's place, not its focus binding plus an offset", () => {
    // Where the finger puts the row: where it stood when the finger came down
    // plus the finger's travel, inside the row's ends. The focus binding
    // (`rest`) is read only when the finger comes down and while settling.
    const follow = slide.slice(slide.indexOf("function follow()"), slide.indexOf("public function move"))
    expect(follow).toMatch(/place = Math\.clamp\(start \+ travel,/)
    expect(follow).not.toMatch(/rest/)
    expect(slide).toMatch(/start = active \? place : rest;/)
    expect(slide).not.toMatch(/offset|changed rest/)
    // Whole items, counted from the travel alone: no step waits for logic.js.
    expect(follow).toMatch(/let whole = Math\.round\(travel \/ pitch\);/)
  })

  test("each rack draws at the slide's place while it is active, with no animation in between", () => {
    expect(read("ui/organisms/PicoCartShelf.slint")).toMatch(/x: -\(slide\.active \? slide\.place : scroll\);/)
    expect(read("ui/organisms/PicoResumeList.slint")).toMatch(/- \(slide\.active \? slide\.place : rack\.scroll\);/)
    expect(read("ui/organisms/PicoLibraryBrowser.slint")).toMatch(/- \(slide\.active \? slide\.place : parent\.scroll\);/)
    expect(read("ui/organisms/PicoStoreResults.slint")).toMatch(/- \(slide\.active \? slide\.place : parent\.scroll\);/)
  })
})
