import { mock } from "bun:test"
import { createRequire } from "node:module"
import { fileURLToPath } from "node:url"
import { GlobalRegistrator } from "@happy-dom/global-registrator"

// Bun.serve requires native Response objects. Keep one native HTTP stack
// instead of mixing it with Happy DOM's browser fetch/preflight emulation.
const nativeHttp = {
  fetch,
  Headers,
  Request,
  Response,
  WebSocket,
  AbortController,
  AbortSignal,
  DOMException,
}
GlobalRegistrator.register()
Object.assign(globalThis, nativeHttp)

/**
 * Web Animations: cancel() rejects `finished` and then sets the promise's
 * [[PromiseIsHandled]] slot to true, so a cancelled animation is not an
 * unhandled rejection (https://drafts.csswg.org/web-animations-1/#canceling-an-animation-section).
 * happy-dom 20 skips that step, and motion cancels animations on unmount.
 * Mark the promise handled before happy-dom rejects it.
 */
const cancelAnimation = Animation.prototype.cancel
Animation.prototype.cancel = function cancel(this: Animation) {
  this.finished.catch(() => undefined)
  cancelAnimation.call(this)
}

/**
 * Surfaces install their own toolchains, but React belongs to their host.
 * The onResolve-only plugin did not unify React under Bun 1.3.5. Register
 * each surface's peer module path before loading either surface, so ESM and
 * CommonJS consumers get the real host exports. No React behavior is replaced.
 * The portal's Vite config deduplicates react and react-dom the same way.
 */
const requireFromPortal = createRequire(import.meta.url)
for (const specifier of [
  "react",
  "react/jsx-runtime",
  "react/jsx-dev-runtime",
  "react-dom",
  "react-dom/client",
]) {
  const hostExports = requireFromPortal(specifier)
  for (const surface of ["@korri/shift", "@korri/pico"]) {
    const peerPath = fileURLToPath(
      new URL(`../node_modules/${specifier}`, import.meta.resolve(surface)),
    )
    mock.module(peerPath, () => hostExports)
  }
}
