/**
 * Pico's Caliper project descriptor.
 *
 * Every path is relative to this file's directory. The version that lived in an
 * external profile hard-coded one machine's home directory, which is half the
 * reason it stopped working — a review environment that only resolves on the
 * machine it was created on is not one.
 *
 * `filesystemRoots` stays narrow on purpose: pointed at the monorepo root it
 * dies with EMFILE trying to watch every worktree.
 */
export default {
  id: "korri-pico-a7960dd2",
  name: "@korri/pico",
  entry: "./project-entry.ts",
  aliases: {
    "@korri/pico/preview": "./src/preview.ts",
    "@korri/pico": "./src/index.ts",
    "@contracts": "../../contracts",
  },
  filesystemRoots: ["./src", "../../contracts", "../../packages/intrinsic-design"],
  capabilities: {
    backendlessFixtures: true,
  },
}
