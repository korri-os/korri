/**
 * Vite on Pico's source, in its own process, for the Chromium tests: the
 * happy-dom preload replaces the timers Vite needs in the test process.
 */
import { join } from "node:path"

const PICO = join(import.meta.dir, "..", "..")

export async function serveBrowserPages(): Promise<{ readonly address: string; stop(): void }> {
  const vite = Bun.spawn(
    [process.execPath, join(PICO, "node_modules/vite/bin/vite.js"), "--config", "test/browser/vite.config.ts", "--port", "0"],
    { cwd: PICO, stdout: "pipe", stderr: "inherit", env: { ...process.env, NO_COLOR: "1" } },
  )
  const reader = (vite.stdout as ReadableStream<Uint8Array>).getReader()
  let printed = ""
  for (;;) {
    const { value, done } = await reader.read()
    if (done) throw new Error(`vite stopped before it served: ${printed}`)
    printed += new TextDecoder().decode(value)
    const found = printed.match(/http:\/\/127\.0\.0\.1:\d+\//)
    if (found !== null) return { address: found[0], stop: () => vite.kill() }
  }
}
