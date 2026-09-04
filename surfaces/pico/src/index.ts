/**
 * Pico — the public entry point.
 *
 * Two ways in, one implementation: `picoSurface` satisfies the treaty for a
 * host that does not speak React, and `PicoSurface` is the component for a host
 * that already renders React. Nothing else is exported; everything below this
 * file is Pico's business and may change without a host caring.
 */
export { PicoSurface } from "./PicoSurface"
export { picoSurface } from "./mount"
import "./pico.css"
