/**
 * Pico's component tree as it stood in React (surfaces/pico/src/ui and
 * src/pages, 2026-11-01), one Slint component per React component at the
 * same layer. The decomposition gate holds the package to it.
 */
export const LAYERS = ["atoms", "molecules", "organisms", "templates", "pages"] as const
export type Layer = (typeof LAYERS)[number]

/** The suffix of a preview file: `<Name>.<suffix>.preview.slint`. */
export const SUFFIX: Record<Layer, string> = {
  atoms: "atom",
  molecules: "molecule",
  organisms: "organism",
  templates: "template",
  pages: "page",
}

/** Where a layer's files live, from the package root. */
export const FOLDER: Record<Layer, string> = {
  atoms: "ui/atoms",
  molecules: "ui/molecules",
  organisms: "ui/organisms",
  templates: "ui/templates",
  pages: "pages",
}

export const PICO_TREE: Record<Layer, readonly string[]> = {
  atoms: [
    "PicoBadge", "PicoBusyMeter", "PicoButton", "PicoChip", "PicoClock", "PicoCoverArt", "PicoDownloadBar", "PicoHint",
    "PicoKey", "PicoMenuKey", "PicoPaletteBar", "PicoPixelDisc", "PicoPixelLock", "PicoRow", "PicoSegments",
    "PicoSignalBars", "PicoSlider", "PicoStat", "PicoStepTrack", "PicoSub", "PicoTally", "PicoTitle",
  ],
  molecules: [
    "PicoButtonBar", "PicoCard", "PicoCart", "PicoChoiceRow", "PicoControllerRow", "PicoControlRow", "PicoEmptySlot",
    "PicoGameFacts", "PicoInviteNotice", "PicoInviteRow", "PicoKeyboard", "PicoNetworkMarks", "PicoNetworkRow",
    "PicoNotice", "PicoQueryField", "PicoResultRow", "PicoScopeTabs", "PicoSettingRow", "PicoSetupHeading",
    "PicoStarterPack", "PicoStatRun", "PicoStatusBar", "PicoStreamPlayers", "PicoTabs", "PicoTextField",
  ],
  organisms: [
    "PicoAttract", "PicoCartGrid", "PicoCartShelf", "PicoChoicePicker", "PicoControllerCheck", "PicoGameActions",
    "PicoGameDetail", "PicoGameHero", "PicoGamesFound", "PicoGameStage", "PicoIdentityDialog", "PicoIdentitySetup",
    "PicoInviteNotices", "PicoLaunchStage", "PicoLibraryBrowser", "PicoLocationPicker", "PicoMenu", "PicoModal",
    "PicoPauseMenu", "PicoPlayingNow", "PicoResumeList", "PicoSetupDone", "PicoSetupWelcome", "PicoStoreEntry",
    "PicoStoreProviders", "PicoStoreResults", "PicoTextEditor", "PicoWifiSetup",
  ],
  templates: ["PicoDisplays", "PicoGameOverlay", "PicoPanelScreen", "PicoScreenShell"],
  pages: [
    "PicoGameDetail", "PicoHome", "PicoLaunchSettings", "PicoLibrary", "PicoOnboarding", "PicoOverlay",
    "PicoRunnerPicker", "PicoSettingsPanel", "PicoSettings", "PicoShowcase", "PicoStore",
  ],
}

/** React components that became part of another Slint component, and why. */
export const MERGED: Readonly<Record<string, { readonly into: string; readonly why: string }>> = {}

/**
 * Slint components with no React counterpart, and why. CSS gave the web
 * build a text line box and wrapping for free; Slint's Text has neither a
 * line height nor a whole-pixel baseline, so text is an atom here.
 */
export const ADDED: Readonly<Record<string, { readonly layer: Layer; readonly why: string }>> = {
  PicoLine: { layer: "atoms", why: "One line of text in a line box with its glyphs on the pixel grid: CSS line-height did this." },
  PicoPara: { layer: "atoms", why: "Words that wrap at the face's size: CSS did this for every block of text." },
  PicoReveal: { layer: "atoms", why: "Says where the cursor's control stands once laid out; the DOM's focus() did this." },
  PicoScroll: { layer: "atoms", why: "A region that keeps the cursor's control in sight: the host's scrollIntoView did this for the DOM." },
}
