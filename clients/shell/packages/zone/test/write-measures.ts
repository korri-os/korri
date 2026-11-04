/**
 * Writes preview/evidence/measures.json: words at caps sizes in every look,
 * with the width src/plan/measure.ts plans for them. tests/zone_package.rs
 * draws each with ZoneMeasure and holds the two equal.
 *
 *     bun packages/zone/test/write-measures.ts
 */
import { writeFileSync } from "node:fs"
import { join } from "node:path"
import { ZONE_LOOKS } from "../src/plan/look"
import { measureWords, shownWords, type ZoneWordFace } from "../src/plan/measure"

const WORDS = [
  "Games", "Options", "Save data", "Find", "Get games", "Play", "Play here", "Play on zao", "Continue", "Opts.",
  "Yes", "No", "Launch anyway", "Cancel", "Reload runners", "Remember for this game", "Hollow Knight",
  "The Legend of Zelda: Oracle of Seasons", "Wario Land 4", "korri 0.4.1", "AVAWAY Tokyo", "Back up", "Delete",
]
const FACES: readonly ZoneWordFace[] = ["caps", "fact", "korri"]
const CAPS = [10, 20, 30, 40]
const out = []
for (const look of ZONE_LOOKS)
  for (const face of FACES)
    for (const cap of CAPS)
      for (const words of WORDS) out.push({ look, face, cap, text: shownWords(words, face, look), width: measureWords(words, cap, face, look) })
writeFileSync(join(import.meta.dir, "..", "preview", "evidence", "measures.json"), JSON.stringify(out, null, 0) + "\n")
console.log(out.length, "measures")
