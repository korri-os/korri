import { mock } from "bun:test"
import { createRequire } from "node:module"
import { fileURLToPath } from "node:url"
import { GlobalRegistrator } from "@happy-dom/global-registrator"

GlobalRegistrator.register()

/**
 * Surfaces install their own toolchains, but React belongs to their host.
 * The onResolve-only plugin did not unify React under Bun 1.3.5. Register
 * each surface's peer module path before loading either surface, so ESM and
 * CommonJS consumers get the real host exports. No React behavior is replaced.
 * Vite's React plugin provides the equivalent react/react-dom deduplication.
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
