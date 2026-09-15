// Full DIY runner. No RetroArch family or family helper.
// This import is type-only and points at the review contract for readability.
import type { Handlers, Runner, FileRule } from "../../plugin-contract"

export const name = "ppsspp"
export const title = "PPSSPP"

export const runners: Record<string, Runner> = {
  ppsspp: {
    id: "@local:ppsspp/ppsspp",
    title: "PPSSPP",
    systems: ["psp"],
  },
}
export const discovery: Record<string, FileRule> = {
  psp: {
    id: "@local:ppsspp/psp-files",
    extensions: ["iso", "cso", "pbp"],
    system: "psp",
    runners: ["@local:ppsspp/ppsspp"],
  },
}
export const handlers: Handlers = {
  "launch.prepare": ({ input, files, context }) => {
    const target = input.selection.target
    if (target.kind !== "file") throw new Error("PPSSPP example needs a file target")
    // The native argv escape-hatch policy needs a PPSSPP-specific implementation.
    // Do not silently ignore an override this review example does not handle.
    if (input.overrides.args || input.overrides.config)
      throw new Error("This illustrative PPSSPP handler does not implement native overrides yet")
    const config = `${context.paths.data}/ppsspp`
    return {
      command: files.program,
      args: ["--fullscreen", target.path],
      directories: [config],
      env: { XDG_CONFIG_HOME: config, XDG_DATA_HOME: config },
    }
  },
}
