/**
 * Where the store and the counter go, as a pure function of the container.
 *
 * The counter carries every decision, so it is never dropped. The store is the
 * place, and it is drawn only when it gets enough room to read; otherwise the
 * counter takes the whole surface. Every threshold is a guess until measured
 * on the devices (640x480 handhelds up to a 1920x1080 panel).
 */

export type BoxbusterLayout =
  /** No store: WebGL is missing, or there is no readable room for it. */
  | { readonly _tag: "CounterOnly" }
  /** The store fills the rest; the counter is docked on the right. */
  | { readonly _tag: "Side"; readonly counterWidth: number }
  /** The store fills the rest; the counter is docked along the bottom. */
  | { readonly _tag: "Below"; readonly counterHeight: number }

/** The smallest store view worth drawing. */
const STORE_MIN_WIDTH = 320
const STORE_MIN_HEIGHT = 200

/** Docked beside: the counter's share of the width, and its bounds. A list of
 * titles and a held tape need about 240 px to stay readable. */
const SIDE_SHARE = 0.4
const SIDE_MIN = 240
const SIDE_MAX = 440

/** Docked below: the counter's share of the height, and its bounds. */
const BELOW_SHARE = 0.5
const BELOW_MIN = 220
const BELOW_MAX = 420

const clamp = (value: number, min: number, max: number) =>
  Math.max(min, Math.min(max, value))

function side(width: number, height: number): BoxbusterLayout | undefined {
  const counterWidth = clamp(Math.round(width * SIDE_SHARE), SIDE_MIN, SIDE_MAX)
  return width - counterWidth >= STORE_MIN_WIDTH && height >= STORE_MIN_HEIGHT
    ? { _tag: "Side", counterWidth }
    : undefined
}

function below(width: number, height: number): BoxbusterLayout | undefined {
  const counterHeight = clamp(
    Math.round(height * BELOW_SHARE),
    BELOW_MIN,
    BELOW_MAX,
  )
  return height - counterHeight >= STORE_MIN_HEIGHT && width >= STORE_MIN_WIDTH
    ? { _tag: "Below", counterHeight }
    : undefined
}

/**
 * The counter docks along the container's longer axis, so the store keeps the
 * plentiful one. If that dock leaves the store too small, the other dock is
 * tried before the store is given up.
 */
export function layoutFor(
  width: number,
  height: number,
  canDrawStore: boolean,
): BoxbusterLayout {
  if (!canDrawStore) return { _tag: "CounterOnly" }
  const docked =
    width >= height
      ? (side(width, height) ?? below(width, height))
      : (below(width, height) ?? side(width, height))
  return docked ?? { _tag: "CounterOnly" }
}
