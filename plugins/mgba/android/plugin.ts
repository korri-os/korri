// Android platform declaration; the core remains packaged in RetroArch's APK.
export const name = "mgba"
export const title = "mGBA"
export const description =
  "Provides the mGBA libretro core for Game Boy Advance games."
export const systems = {
  gba: {
    id: "gba",
    title: "Game Boy Advance",
  },
}
export const runtimes = {
  mgba: {
    id: "@korri:mgba/mgba",
    kind: "libretro-core",
    app: "@korri:retroarch/retroarch",
    path: "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so",
    supports: {
      systems: ["gba"],
    },
  },
}
export const discovery = {
  fileReleases: {
    "gba-files": {
      id: "@korri:mgba/gba-files",
      title: "Game Boy Advance ROM files",
      extensions: ["gba"],
      system: "gba",
      launcher: "@korri:retroarch/retroarch",
      runtime: "@korri:mgba/mgba",
    },
  },
}
