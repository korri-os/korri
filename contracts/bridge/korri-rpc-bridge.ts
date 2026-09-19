/**
 * The Linux portal host's private `window.KorriRpc` binding.
 *
 * The host installs this binding before the trusted portal loads. Credentials
 * stay in memory. They must not appear in page URLs, browser storage, logs, or
 * public static assets.
 */
export interface KorriRpcBridgeSurface {
  /** The device's korrid port on 127.0.0.1. */
  korridPort(): number
  /** The bearer capability for this korrid server lifetime. Never persist it. */
  korridCapability(): string
}
