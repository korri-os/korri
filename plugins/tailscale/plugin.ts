// Identity and contributes.daemons follow Korri's existing plugin declaration.
// Service fields come from upstream cmd/tailscaled/tailscaled.service. The host
// supplies isolated state/runtime directories and an unprivileged service user.
({
  namespace: "@korri",
  name: "tailscale",
  title: "Tailscale",
  contributes: {
    daemons: [{
      Type: "notify",
      ExecStart: [
        "bin/tailscaled",
        "--state=${STATE_DIRECTORY}/tailscaled.state",
        "--socket=${RUNTIME_DIRECTORY}/tailscaled.sock",
        "--port=41641",
      ],
      ExecStopPost: ["bin/tailscaled", "--cleanup"],
      CapabilityBoundingSet: ["CAP_NET_ADMIN", "CAP_NET_RAW"],
    }],
  },
})
