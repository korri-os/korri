/**
 * How the chosen surface is found and remembered.
 *
 * A device that can only load one fixed URL still has to be able to keep a
 * choice across restarts, and a browser has to be able to override it.
 */
import { describe, expect, test } from "bun:test"
import { resolveSurfacePreference, SURFACE_PARAM } from "./surface-preference"
import { DEFAULT_SURFACE_ID } from "./surface-registry"

function storage(seed?: string): Storage & { readonly written: string[] } {
  const written: string[] = []
  let value = seed
  return {
    written,
    getItem: () => value ?? null,
    setItem: (_key: string, next: string) => {
      value = next
      written.push(next)
    },
    removeItem: () => {
      value = undefined
    },
    clear: () => {
      value = undefined
    },
    key: () => null,
    length: 0,
  } as Storage & { readonly written: string[] }
}

/** Storage that refuses every operation, as a full or disabled store does. */
const hostileStorage = {
  getItem: () => {
    throw new Error("denied")
  },
  setItem: () => {
    throw new Error("denied")
  },
} as unknown as Storage

describe("resolveSurfacePreference", () => {
  test("takes the surface named in the query and remembers it", () => {
    const store = storage()

    expect(resolveSurfacePreference({ search: `?${SURFACE_PARAM}=pico` }, store))
      .toBe("pico")
    expect(store.written).toEqual(["pico"])
  })

  test("uses what was remembered when the query says nothing", () => {
    expect(resolveSurfacePreference({ search: "" }, storage("pico"))).toBe("pico")
  })

  test("lets the query override what was remembered", () => {
    const store = storage("pico")

    expect(
      resolveSurfacePreference({ search: `?${SURFACE_PARAM}=shift` }, store),
    ).toBe("shift")
    expect(store.written).toEqual(["shift"])
  })

  test("ignores a surface that does not exist, in either source", () => {
    // The value is user-writable, so an unrecognised one is not trusted.
    expect(
      resolveSurfacePreference({ search: `?${SURFACE_PARAM}=nope` }, storage()),
    ).toBe(DEFAULT_SURFACE_ID)
    expect(resolveSurfacePreference({ search: "" }, storage("nope"))).toBe(
      DEFAULT_SURFACE_ID,
    )
  })

  test("still boots when storage refuses to answer", () => {
    // A display preference is not worth failing a boot over.
    expect(resolveSurfacePreference({ search: "" }, hostileStorage)).toBe(
      DEFAULT_SURFACE_ID,
    )
    expect(
      resolveSurfacePreference({ search: `?${SURFACE_PARAM}=pico` }, hostileStorage),
    ).toBe("pico")
  })

  test("defaults when there is no storage at all", () => {
    expect(resolveSurfacePreference({ search: "" })).toBe(DEFAULT_SURFACE_ID)
  })
})
