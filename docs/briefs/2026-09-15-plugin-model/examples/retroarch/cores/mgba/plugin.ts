// Ordinary plugin exports. The helper fills missing seams and handlers.
// The effective schema merges this addition with the RetroArch schema.
export const name = "mgba"
export const title = "mGBA"

export const settings = {
  type: "object",
  properties: {
    core: {
      type: "object",
      properties: {
        "skip-bios": {
          type: "boolean",
          title: "Skip BIOS intro",
          // Helper-owned annotation. Korri does not interpret native keys.
          "x-retroarch": {
            key: "mgba_skip_bios",
            values: { true: "ON", false: "OFF" },
          },
        },
      },
    },
  },
}

export const defaults = {
  video: { "integer-scale": true },
  core: { "skip-bios": true },
}

// No handlers: use the helper's ordinary launch.prepare implementation.
// An exported handler for that operation would replace it explicitly.
