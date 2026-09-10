// Test-only copy of the real Tailscale declaration moved to korri-os/plugins.
// The fixture's Nix composition supplies publisher identity in its manifest.
// Daemon fields remain internal until the native-unit slice.
// Service fields come from upstream cmd/tailscaled/tailscaled.service. The host
// supplies isolated state/runtime directories and an unprivileged service user.
export const name = "tailscale"
export const title = "Tailscale"
export const daemons = [
  {
    Type: "notify",
    ExecStart: [
      "bin/tailscaled",
      "--state=${STATE_DIRECTORY}/tailscaled.state",
      "--socket=${RUNTIME_DIRECTORY}/tailscaled.sock",
      "--port=41641",
    ],
    ExecStopPost: ["bin/tailscaled", "--cleanup"],
    CapabilityBoundingSet: ["CAP_NET_ADMIN", "CAP_NET_RAW"],
  },
]
