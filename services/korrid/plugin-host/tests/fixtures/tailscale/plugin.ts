// Test-only identity; trusted publisher composition owns the namespace.
// Login is an explicit operator `tailscale up`, not a daemon credential effect.
export const name = "tailscale"
export const title = "Tailscale"
export const services = ["tailscaled"]
