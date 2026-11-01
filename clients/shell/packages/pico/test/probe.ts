/**
 * The shell's headless probe (clients/shell/examples/surface-probe.rs): the
 * same Slint software renderer and view conversion the shell uses. Build it
 * with `cargo build --release --features fixture --example surface-probe`
 * in clients/shell, or point PICO_PROBE at it. Without it the tests that
 * need it are skipped, and say so.
 */
import { existsSync } from "node:fs"
import { join } from "node:path"
import { ROOT } from "./slint-source"

const BUILT = join(ROOT, "..", "..", "..", "..", ".cache", "shell-target", "release", "examples", "surface-probe")

export const PROBE = process.env["PICO_PROBE"] ?? (existsSync(BUILT) ? BUILT : undefined)

/** Pico's sizes: 320x240, 640x480, 480x800 (portrait), 1280x720, 1920x1080. */
export const FIVE_SIZES = "320x240,640x480,480x800,1280x720,1920x1080"

export function probe(args: readonly string[]): { readonly code: number; readonly out: string } {
  if (PROBE === undefined) throw new Error("no probe")
  const run = Bun.spawnSync([PROBE, ...args], { cwd: join(ROOT, ".."), stdout: "pipe", stderr: "pipe" })
  return { code: run.exitCode ?? 1, out: `${run.stdout.toString()}${run.stderr.toString()}` }
}
