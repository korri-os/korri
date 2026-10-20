/**
 * The `RpcFailure.code` strings the store tells apart, as korrid writes them
 * (services/korrid/SCRIPTING.md, "Acquisition providers"). A table of
 * constants, not a wire type: every other code is shown with korrid's own
 * message, which is plain copy.
 */
export const ACQUISITION_CODE = {
  /** `acquire-status`/`acquire-cancel` after a korrid restart. */
  unknownJob: "AcquisitionUnknownJob",
  /** `acquire` for a claim korrid does not hold: claims live in memory, so
   * after a restart a claim is held again only once search or details
   * returns it. */
  unknownClaim: "AcquisitionUnknownClaim",
  /** The claim names no enabled system: it cannot be acquired. */
  systemUnknown: "AcquisitionSystemUnknown",
  /** Failed: the provider answered with a page, not the file (NonFinalDownload). */
  notFinal: "DownloadNotFinal",
  /** Failed: the provider said it cannot give the file (FailedDownload). */
  resolutionFailed: "DownloadResolutionFailed",
} as const
