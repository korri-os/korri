/**
 * Boxbuster — the public entry point.
 *
 * Two ways in, one implementation: `boxbusterSurface` satisfies the treaty for
 * a host that does not speak React, and `BoxbusterSurface` is the component for
 * a host that already renders React. Nothing else is exported.
 *
 * This module pulls in three.js. A host that offers several surfaces should
 * load it on demand, so a device that never opens Boxbuster never downloads it.
 */
export { BoxbusterSurface } from "./BoxbusterSurface"
export { boxbusterSurface } from "./mount"
