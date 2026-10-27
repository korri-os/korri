import { plugin } from "bun"
import { createRequire } from "node:module"
import { dirname, resolve } from "node:path"
import { GlobalRegistrator } from "@happy-dom/global-registrator"

GlobalRegistrator.register()

// Vite reads Fiber's `module` entry. Bun otherwise picks its CJS `main`,
// which requires Three's asynchronous ESM module and fails before tests load.
// Resolve to the package's real ESM source; do not replace renderer exports.
const require = createRequire(import.meta.url)
const packagePath = require.resolve("@react-three/fiber/package.json")
const metadata: { module?: unknown } = require(packagePath)
if (typeof metadata.module !== "string") {
  throw new Error("@react-three/fiber must declare its ESM module entry")
}
const modulePath = resolve(dirname(packagePath), metadata.module)
const renderer = await import(modulePath)
const runtimeEntry = require.resolve("@react-three/fiber")
plugin({
  name: "fiber-production-esm-entry",
  setup(build) {
    // Bun's runtime hooks do not match bare package specifiers. Match the
    // resolved entry and forward the actual ESM exports without a test double.
    build.onLoad(
      { filter: /\/node_modules\/@react-three\/fiber\/.*\.js$/ },
      ({ path }) => {
        if (path === runtimeEntry) return { loader: "object", exports: renderer }
      },
    )
  },
})
