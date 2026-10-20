/**
 * The one bytes formatter (src/plan/bytes.ts), at every unit boundary and
 * one byte either side of the decimal threshold.
 */
import { describe, expect, test } from "bun:test"
import { formatZoneBytes, ZONE_BYTES_DECIMAL_FROM, zoneProgressShare } from "../src/plan/bytes"

describe("formatZoneBytes", () => {
  test("bytes under 1 KB are whole bytes", () => {
    expect(formatZoneBytes(0)).toBe("0 B")
    expect(formatZoneBytes(1)).toBe("1 B")
    expect(formatZoneBytes(999)).toBe("999 B")
  })

  test("KB from 1000 B, whole", () => {
    expect(formatZoneBytes(1_000)).toBe("1 KB")
    expect(formatZoneBytes(1_499)).toBe("1 KB")
    expect(formatZoneBytes(1_500)).toBe("2 KB")
    expect(formatZoneBytes(999_499)).toBe("999 KB")
  })

  test("a value that rounds to 1000 KB is written as 1 MB", () => {
    expect(formatZoneBytes(999_500)).toBe("1 MB")
    expect(formatZoneBytes(999_999)).toBe("1 MB")
  })

  test("MB from 1 000 000 B, whole below 10 MB", () => {
    expect(formatZoneBytes(1_000_000)).toBe("1 MB")
    expect(formatZoneBytes(3_400_000)).toBe("3 MB")
    expect(formatZoneBytes(9_499_999)).toBe("9 MB")
    expect(formatZoneBytes(ZONE_BYTES_DECIMAL_FROM - 1)).toBe("10 MB")
  })

  test("one decimal from 10 MB", () => {
    expect(ZONE_BYTES_DECIMAL_FROM).toBe(10_000_000)
    expect(formatZoneBytes(ZONE_BYTES_DECIMAL_FROM)).toBe("10.0 MB")
    expect(formatZoneBytes(12_340_000)).toBe("12.3 MB")
    expect(formatZoneBytes(612_400_000)).toBe("612.4 MB")
    expect(formatZoneBytes(999_949_999)).toBe("999.9 MB")
  })

  test("a value that rounds to 1000.0 MB is written as 1.0 GB", () => {
    expect(formatZoneBytes(999_950_000)).toBe("1.0 GB")
    expect(formatZoneBytes(999_999_999)).toBe("1.0 GB")
  })

  test("GB from 1 000 000 000 B, one decimal, and nothing larger", () => {
    expect(formatZoneBytes(1_000_000_000)).toBe("1.0 GB")
    expect(formatZoneBytes(1_160_000_000)).toBe("1.2 GB")
    expect(formatZoneBytes(1_500_000_000)).toBe("1.5 GB")
    expect(formatZoneBytes(2_000_000_000_000)).toBe("2000.0 GB")
  })

  test("a negative or non-finite count is 0 B", () => {
    expect(formatZoneBytes(-5)).toBe("0 B")
    expect(formatZoneBytes(Number.NaN)).toBe("0 B")
    expect(formatZoneBytes(Number.POSITIVE_INFINITY)).toBe("0 B")
  })
})

describe("zoneProgressShare", () => {
  test("a known total fills its share, clamped to the bar", () => {
    expect(zoneProgressShare({ done: 612, total: 1_224 })).toBe(0.5)
    expect(zoneProgressShare({ done: 0, total: 100 })).toBe(0)
    expect(zoneProgressShare({ done: 100, total: 100 })).toBe(1)
    expect(zoneProgressShare({ done: 200, total: 100 })).toBe(1)
    expect(zoneProgressShare({ done: -5, total: 100 })).toBe(0)
  })
  test("no total has no share; a total of 0 fills nothing", () => {
    expect(zoneProgressShare({ done: 212 })).toBeUndefined()
    expect(zoneProgressShare({ done: 5, total: 0 })).toBe(0)
  })
})
