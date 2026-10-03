/**
 * Whether this browser can draw the store at all.
 *
 * Some kiosks run Chromium with `--disable-gpu`, where WebGL may be missing.
 * Boxbuster must never be a blank screen there: without WebGL the counter is
 * the whole surface. The probe context is released at once, so it does not
 * count against the browser's limit on live WebGL contexts.
 */
export function canDrawStore(): boolean {
  try {
    const canvas = document.createElement("canvas")
    const gl = canvas.getContext("webgl2") ?? canvas.getContext("webgl")
    if (gl === null) return false
    gl.getExtension("WEBGL_lose_context")?.loseContext()
    return true
  } catch {
    return false
  }
}
