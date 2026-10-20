import type { SurfaceGame } from "@contracts/surface/korri-surface"

/**
 * What a game still has to fetch to this device, in Shift's words (ADR 0013).
 * Korri sends bytes, a queued flag and a position; Shift owns the phrasing.
 * It explains why Download, Cancel download or Remove download appear under
 * Options. Absent when Korri said nothing, which is every game that plays at
 * once.
 */
export function shiftDownloadLabel(download: SurfaceGame["download"]): string | undefined {
  if (download === undefined) return undefined
  if (download.progress !== undefined) {
    const { done, total } = download.progress
    if (total === undefined || total <= 0) return "Downloading"
    return `Downloading ${Math.max(0, Math.min(100, Math.floor((done / total) * 100)))}%`
  }
  if (download.queued === true) return "Waiting to download"
  return download.bytes === undefined ? "Needs a download" : `Needs a ${shiftBytesLabel(download.bytes)} download`
}

/** "1.5 GB", "612 MB", "48 KB": decimal units, one decimal below ten. */
export function shiftBytesLabel(bytes: number): string {
  const scale = (value: number, unit: string) => {
    const text = value < 10 ? value.toFixed(1) : String(Math.round(value))
    return `${text.endsWith(".0") ? text.slice(0, -2) : text} ${unit}`
  }
  if (bytes >= 1e9) return scale(bytes / 1e9, "GB")
  if (bytes >= 1e6) return scale(bytes / 1e6, "MB")
  if (bytes >= 1e3) return scale(bytes / 1e3, "KB")
  return `${Math.round(bytes)} B`
}
