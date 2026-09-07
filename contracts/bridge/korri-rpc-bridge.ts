/**
 * The shell's `window.KorriRpc` binding, shared by Android and Linux.
 * Extracted without renaming the existing korridPort/korridCapability methods
 * from KorriNativeBridgeSurface. This binding grants no hardware operations.
 *
 * The shell installs it before the trusted portal loads and removes access
 * before untrusted navigation or teardown. Credentials stay in memory. They
 * must not appear in page URLs, browser storage, logs, or public static assets.
 */
export interface KorriRpcBridgeSurface {
  /** The device's korrid port on 127.0.0.1, or -1 when it is not running. */
  korridPort(): number
  /** The bearer capability for that korrid server lifetime. Never persist it. */
  korridCapability(): string
}
