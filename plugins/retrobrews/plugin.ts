// Curated Game Boy Advance and NES homebrew from the RetroBrews collections
// on GitHub, declared as games whose content is a pinned artifact (ADR 0013,
// ADR 0012). The entries come from legacy
// product/plugins/acquisition-fixtures/index.ts (legacy
// 0e4cec9da3d77e6578b8a01a5d83420ba0d98e62). korrid downloads and verifies
// each file into its cache on first use; this file only declares it. The
// systems `gba` and `nes` and their runners come from the publisher's mgba
// and fceumm plugins.

export const name = "retrobrews"
export const title = "RetroBrews"
export const description =
  "Curated Game Boy Advance and NES homebrew from RetroBrews, downloaded on first use."

// Without `unpack`, the ready directory holds the verified file named by the
// URL's last segment. `version` names that directory; RetroBrews publishes no
// versions, so it is the first 12 hex digits of the file's SHA-256.
// Measured 2026-10-20T05:55:56Z, 05:58:00Z and 07:39:12Z (README, live-probe.ts).
export const artifacts = {
  ambushed: {
    id: "@korri:retrobrews/ambushed",
    version: "c4efb1e252e3",
    title: "Ambushed",
    files: [
      {
        url: "https://raw.githubusercontent.com/retrobrews/nes-games/master/ambushed.nes",
        sha256: "sha256:c4efb1e252e39dd957ecc76ff2a868159bb3a6dc66e07addfcdbf9e6660dfb62",
        bytes: 16400,
      },
    ],
  },
  anguna: {
    id: "@korri:retrobrews/anguna",
    version: "cd3b969d5ed6",
    title: "Anguna",
    files: [
      {
        url: "https://raw.githubusercontent.com/retrobrews/gba-games/master/anguna.gba",
        sha256: "sha256:cd3b969d5ed6b38b2039c11b894d6db2fc3fad209a7fd046bee241cc66069851",
        bytes: 1710888,
      },
    ],
  },
}

// A release's hash is its content identity as discovery computes it: the
// SHA-256 after the system's header rule strips its bytes (discovery/identify/
// hash.rs). The publisher's `nes` system strips the 16-byte iNES header
// (`NES\x1a`), so Ambushed's release hash covers bytes 16..end and differs
// from its artifact file hash. `gba` has no header rule, so Anguna's release
// hash is the whole file's. The release key is the first 12 hex digits of the
// release hash; the artifact version those of the file hash.
export const games = {
  ambushed: {
    title: "Ambushed",
    releases: {
      b9c6afa60056: {
        system: "nes",
        hashes: ["sha256:b9c6afa600568aa0144aed787acf778d6237187f89776b7d3548889700cd589a"],
        artifacts: ["@korri:retrobrews/ambushed"],
        file: { artifact: "@korri:retrobrews/ambushed", path: "ambushed.nes" },
      },
    },
  },
  anguna: {
    title: "Anguna",
    releases: {
      cd3b969d5ed6: {
        system: "gba",
        hashes: ["sha256:cd3b969d5ed6b38b2039c11b894d6db2fc3fad209a7fd046bee241cc66069851"],
        artifacts: ["@korri:retrobrews/anguna"],
        file: { artifact: "@korri:retrobrews/anguna", path: "anguna.gba" },
      },
    },
  },
}
