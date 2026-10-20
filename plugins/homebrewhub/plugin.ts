// Curated Game Boy homebrew from Homebrew Hub (hh3.gbdev.io), declared as
// games whose content is a pinned artifact (ADR 0013, ADR 0012). The entries
// come from legacy product/plugins/acquisition-fixtures/index.ts (legacy
// 0e4cec9da3d77e6578b8a01a5d83420ba0d98e62). korrid downloads and verifies
// the file into its cache on first use; this file only declares it. The
// system `gb` and its runner come from the publisher's gambatte plugin.

export const name = "homebrewhub"
export const title = "Homebrew Hub"
export const description = "Curated Game Boy homebrew from Homebrew Hub, downloaded on first use."

// Without `unpack`, the ready directory holds the verified file named by the
// URL's last segment. `version` names that directory; Homebrew Hub publishes
// no version, so it is the first 12 hex digits of the file's SHA-256.
// Measured 2026-10-20T05:55:56Z (README, live-probe.ts).
export const artifacts = {
  "2048gb": {
    id: "@korri:homebrewhub/2048gb",
    version: "3ea2376b15b3",
    title: "2048gb",
    files: [
      {
        url: "https://hh3.gbdev.io/static/database-gb/entries/2048gb/2048.gb",
        sha256: "sha256:3ea2376b15b34bd26b10e6b31d2753bf8b086098dbdb5ee84d9c0f03d66d3d7f",
        bytes: 32768,
      },
    ],
  },
}

// The release's hash is its content file's hash: the downloaded file itself.
export const games = {
  "2048gb": {
    title: "2048gb",
    releases: {
      "3ea2376b15b3": {
        system: "gb",
        hashes: ["sha256:3ea2376b15b34bd26b10e6b31d2753bf8b086098dbdb5ee84d9c0f03d66d3d7f"],
        artifacts: ["@korri:homebrewhub/2048gb"],
        file: { artifact: "@korri:homebrewhub/2048gb", path: "2048.gb" },
      },
    },
  },
}
