/**
 * The plan's constants are the tokens: every threshold and length the
 * Slint view reads from ui/tokens.slint equals the one Zone's TypeScript
 * plans name (src/plan/), so the view and the plans cannot drift.
 */
import { describe, expect, test } from "bun:test"
import { ZONE_ART_ROWS } from "../src/plan/art-px"
import { ZONE_FIND_KEYS_MOST, ZONE_KEY_HEIGHTS, ZONE_KEY_UNIT_LEAST } from "../src/plan/keyboard"
import { ZONE_PAUSED_BAND_LEAST, ZONE_PAUSED_BAND_MOST, ZONE_PAUSED_BAND_SHARE, ZONE_PAUSED_ROWS_LEAST, ZONE_PAUSED_WEDGE_TOP } from "../src/plan/paused"
import { ZONE_TOP_INLINE_WIDTH, ZONE_TOP_KEYS_WIDTH, ZONE_TOP_TIGHT_WIDTH } from "../src/plan/top-strip"
import { ZONE_CARD_FOCUS_MAX, ZONE_CARD_RATIO, ZONE_CARD_SIDE_SCALE, ZONE_FOOT_STRIP, ZONE_TAB_STRIP, ZONE_TALL_HEIGHT, ZONE_WIDE_WIDTH } from "../src/plan/zone-select"
import { ZONE_IDLE_AFTER_MS, ZONE_IDLE_CYCLE_MS } from "../src/state/navigation"
import { read, stripComments } from "./slint-source"

const tokens = new Map<string, number>()
for (const [, name, value] of stripComments(read("ui/tokens.slint")).matchAll(/out property <float> ([\w-]+): ([\d.]+);/g)) tokens.set(name!, Number(value))

const HELD: readonly (readonly [token: string, plan: number])[] = [
  ["art-rows", ZONE_ART_ROWS],
  ["wide-at", ZONE_WIDE_WIDTH],
  ["tall-at", ZONE_TALL_HEIGHT],
  ["strip-inline-at", ZONE_TOP_INLINE_WIDTH],
  ["strip-tight-at", ZONE_TOP_TIGHT_WIDTH],
  ["strip-keys-at", ZONE_TOP_KEYS_WIDTH],
  ["card-most", ZONE_CARD_FOCUS_MAX],
  ["card-aspect", ZONE_CARD_RATIO],
  ["side-share", ZONE_CARD_SIDE_SCALE],
  ["top-short", ZONE_TAB_STRIP.short],
  ["top-tall", ZONE_TAB_STRIP.tall],
  ["foot-short", ZONE_FOOT_STRIP.short],
  ["foot-tall", ZONE_FOOT_STRIP.tall],
  ["keys-most", ZONE_FIND_KEYS_MOST],
  ["key-tall", ZONE_KEY_HEIGHTS[0]],
  ["key-least", ZONE_KEY_UNIT_LEAST],
  ["idle-after-ms", ZONE_IDLE_AFTER_MS],
  ["idle-cycle-ms", ZONE_IDLE_CYCLE_MS],
  ["paused-band-share", ZONE_PAUSED_BAND_SHARE],
  ["paused-band-least", ZONE_PAUSED_BAND_LEAST],
  ["paused-band-most", ZONE_PAUSED_BAND_MOST],
  ["paused-wedge-top", ZONE_PAUSED_WEDGE_TOP],
  ["paused-rows-least", ZONE_PAUSED_ROWS_LEAST],
]

describe("the plan's constants are the tokens", () => {
  for (const [token, plan] of HELD) {
    test(`${token} is held equal to the plan's ${plan}`, () => expect(tokens.get(token)).toBe(plan))
  }
})
