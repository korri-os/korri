export type ShiftDesignPartLayer =
  | "page"
  | "template"
  | "organism"
  | "molecule"
  | "atom"

export interface ShiftDesignPart {
  readonly id: string
  readonly layer: ShiftDesignPartLayer
  readonly name: string
}

export const SHIFT_DESIGN_PARTS = {
  home: { id: "shift.home", layer: "page", name: "Home" },
  library: { id: "shift.library", layer: "page", name: "Library" },
  backdrop: {
    id: "shift.cine-backdrop",
    layer: "molecule",
    name: "Backdrop",
  },
  statusBar: {
    id: "shift.status-bar",
    layer: "molecule",
    name: "Status Bar",
  },
  battery: { id: "shift.battery", layer: "atom", name: "Battery" },
  clock: { id: "shift.clock", layer: "atom", name: "Clock" },
  networkIcon: {
    id: "shift.network-icon",
    layer: "atom",
    name: "Network Icon",
  },
  hero: { id: "shift.cine-hero", layer: "organism", name: "Hero" },
  legend: {
    id: "shift.cine-legend",
    layer: "molecule",
    name: "Legend",
  },
  rail: { id: "shift.cine-rail", layer: "organism", name: "Rail" },
  tile: { id: "shift.cine-tile", layer: "molecule", name: "Tile" },
  cineLibraryTile: {
    id: "shift.cine-library-tile",
    layer: "molecule",
    name: "Library Rail Tile",
  },
  cineLibraryHero: {
    id: "shift.cine-library-hero",
    layer: "organism",
    name: "Library Rail Hero",
  },
  cineStoreTile: {
    id: "shift.cine-store-tile",
    layer: "molecule",
    name: "Store Rail Tile",
  },
  cineStoreHero: {
    id: "shift.cine-store-hero",
    layer: "organism",
    name: "Store Rail Hero",
  },
  cineSurpriseTile: {
    id: "shift.cine-surprise-tile",
    layer: "molecule",
    name: "Surprise Rail Tile",
  },
  cineSurpriseHero: {
    id: "shift.cine-surprise-hero",
    layer: "organism",
    name: "Surprise Rail Hero",
  },
  detailActions: {
    id: "shift.detail-actions",
    layer: "molecule",
    name: "Detail Actions",
  },
  detailHints: {
    id: "shift.detail-hints",
    layer: "molecule",
    name: "Detail Hints",
  },
  libraryTile: {
    id: "shift.library-tile",
    layer: "molecule",
    name: "Library Tile",
  },
  // The library variants are source-agnostic layouts (they take `games` as a
  // slot), so they are TEMPLATES; a library page is a variant bound to a
  // committed source/route, which is still pending the control-model decision.
  libraryGrid: {
    id: "shift.library-grid",
    layer: "template",
    name: "Library — Grid",
  },
  libraryShelves: {
    id: "shift.library-shelves",
    layer: "template",
    name: "Library — Shelves",
  },
  libraryLens: {
    id: "shift.library-lens",
    layer: "template",
    name: "Library — Lens",
  },
  libraryFilterBar: {
    id: "shift.library-filterbar",
    layer: "template",
    name: "Library — Filter Bar",
  },
  libraryDeck: {
    id: "shift.library-deck",
    layer: "template",
    name: "Library — Deck",
  },
  libraryReel: {
    id: "shift.library-reel",
    layer: "template",
    name: "Library — Reel",
  },
  // Page-level layouts arranging organisms around a data slot.
  homeTemplate: {
    id: "shift.home-template",
    layer: "template",
    name: "Cinematic Home",
  },
  detailTemplate: {
    id: "shift.detail-template",
    layer: "template",
    name: "Detail Split",
  },
  // Shared library scaffolding inlined across the variants.
  libraryHeader: {
    id: "shift.library-header",
    layer: "molecule",
    name: "Library Header",
  },
  libraryEmpty: {
    id: "shift.library-empty",
    layer: "atom",
    name: "Library Empty",
  },
  libraryHeading: {
    id: "shift.library-heading",
    layer: "atom",
    name: "Library Heading",
  },
  libraryCount: {
    id: "shift.library-count",
    layer: "atom",
    name: "Library Count",
  },
  libraryShelfTitle: {
    id: "shift.library-shelf-title",
    layer: "atom",
    name: "Library Shelf Title",
  },
  libraryTileBadge: {
    id: "shift.library-tile-badge",
    layer: "atom",
    name: "Library Tile Badge",
  },
  libraryTileTitle: {
    id: "shift.library-tile-title",
    layer: "atom",
    name: "Library Tile Title",
  },
  libraryGridView: {
    id: "shift.library-grid-view",
    layer: "organism",
    name: "Library Grid View",
  },
  libraryShelf: {
    id: "shift.library-shelf",
    layer: "organism",
    name: "Library Shelf",
  },
  libraryShelfStack: {
    id: "shift.library-shelf-stack",
    layer: "organism",
    name: "Library Shelf Stack",
  },
  // Shared leaf atoms.
  coverArt: { id: "shift.cover-art", layer: "atom", name: "Cover Art" },
  monogram: { id: "shift.monogram", layer: "atom", name: "Monogram" },
  // Shared slide-out side sheet (surface-agnostic overlay primitive).
  sheet: { id: "shift.sheet", layer: "organism", name: "Sheet" },
  sheetPanel: {
    id: "shift.sheet-panel",
    layer: "organism",
    name: "Sheet Panel",
  },
  sheetAction: {
    id: "shift.sheet-action",
    layer: "molecule",
    name: "Sheet Action",
  },
  sheetGroup: {
    id: "shift.sheet-group",
    layer: "molecule",
    name: "Sheet Group",
  },
  gameActionsSheet: {
    id: "shift.game-actions-sheet",
    layer: "organism",
    name: "Game Actions Sheet",
  },
  // Settings: a readable page of device facts.
  settingsTemplate: {
    id: "shift.settings-template",
    layer: "template",
    name: "Settings",
  },
  settingGroup: {
    id: "shift.setting-group",
    layer: "molecule",
    name: "Setting Group",
  },
  settingRow: {
    id: "shift.setting-row",
    layer: "molecule",
    name: "Setting Row",
  },
  // Reel internals.
  reelTitle: {
    id: "shift.reel-title",
    layer: "atom",
    name: "Reel Title",
  },
  reelTags: { id: "shift.reel-tags", layer: "atom", name: "Reel Tags" },
  reelSpinButton: {
    id: "shift.reel-spin-button",
    layer: "atom",
    name: "Reel Spin Button",
  },
  reelPlayButton: {
    id: "shift.reel-play-button",
    layer: "atom",
    name: "Reel Play Button",
  },
  reelCover: {
    id: "shift.reel-cover",
    layer: "molecule",
    name: "Reel Cover",
  },
  reelStage: {
    id: "shift.reel-stage",
    layer: "organism",
    name: "Reel Stage",
  },
  reelHero: { id: "shift.reel-hero", layer: "molecule", name: "Reel Hero" },
  reelActions: {
    id: "shift.reel-actions",
    layer: "molecule",
    name: "Reel Actions",
  },
  // Deck internals.
  deckTitle: { id: "shift.deck-title", layer: "atom", name: "Deck Title" },
  deckTags: { id: "shift.deck-tags", layer: "atom", name: "Deck Tags" },
  deckArrow: { id: "shift.deck-arrow", layer: "atom", name: "Deck Arrow" },
  deckPlayButton: {
    id: "shift.deck-play-button",
    layer: "atom",
    name: "Deck Play Button",
  },
  deckFavoriteButton: {
    id: "shift.deck-favorite-button",
    layer: "atom",
    name: "Deck Favorite Button",
  },
  deckBleed: {
    id: "shift.deck-bleed",
    layer: "molecule",
    name: "Deck Bleed",
  },
  deckCounter: {
    id: "shift.deck-counter",
    layer: "atom",
    name: "Deck Counter",
  },
  deckCard: { id: "shift.deck-card", layer: "molecule", name: "Deck Card" },
  deckHero: { id: "shift.deck-hero", layer: "molecule", name: "Deck Hero" },
  deckActions: {
    id: "shift.deck-actions",
    layer: "molecule",
    name: "Deck Actions",
  },
  // Lens internals.
  lensRow: { id: "shift.lens-row", layer: "molecule", name: "Lens Row" },
  lensTab: { id: "shift.lens-tab", layer: "atom", name: "Lens Tab" },
  lensSort: { id: "shift.lens-sort", layer: "atom", name: "Lens Sort" },
  // Filter-bar internals.
  filterChip: {
    id: "shift.filter-chip",
    layer: "atom",
    name: "Filter Chip",
  },
  filterToolbar: {
    id: "shift.filter-toolbar",
    layer: "molecule",
    name: "Filter Toolbar",
  },
  // Detail internals.
  detailArt: { id: "shift.detail-art", layer: "atom", name: "Detail Art" },
  detailTitle: {
    id: "shift.detail-title",
    layer: "atom",
    name: "Detail Title",
  },
  detailTags: {
    id: "shift.detail-tags",
    layer: "atom",
    name: "Detail Tags",
  },
  detailSynopsis: {
    id: "shift.detail-synopsis",
    layer: "atom",
    name: "Detail Synopsis",
  },
  detailButton: {
    id: "shift.detail-button",
    layer: "atom",
    name: "Detail Button",
  },
  detailHint: {
    id: "shift.detail-hint",
    layer: "atom",
    name: "Detail Hint",
  },
  detailFavoriteBadge: {
    id: "shift.detail-favorite-badge",
    layer: "atom",
    name: "Detail Favorite Badge",
  },
  detailStats: {
    id: "shift.detail-stats",
    layer: "molecule",
    name: "Detail Stats",
  },
  // Store — remote-catalog search variants (source-agnostic layouts taking an
  // `entries` slot, so they are TEMPLATES) and their shared leaf parts. Models
  // a console store, but every item is ACQUIRED (free), never purchased. These
  // layouts are EXPLORATIONS (takes) — each variant root carries `data-proto` so
  // the design tooling knows what to promote/decompose.
  storeSpotlight: {
    id: "shift.store-spotlight",
    layer: "template",
    name: "Store — Spotlight",
  },
  storeFinder: {
    id: "shift.store-finder",
    layer: "molecule",
    name: "Store Finder",
  },
  storeChip: {
    id: "shift.store-chip",
    layer: "atom",
    name: "Store Source Chip",
  },
  storeSpotlightHero: {
    id: "shift.store-spotlight-hero",
    layer: "organism",
    name: "Store Spotlight Hero",
  },
  storeEmpty: {
    id: "shift.store-empty",
    layer: "atom",
    name: "Store Empty",
  },
  // Store — browse-first variants. Items are NAVIGATION targets (focus → open a
  // detail page where the acquire action lives), not action buttons. Search and
  // filtering share one compact `Store Finder` pill; the filter fans out as an
  // overlay, so opening it never pushes the results down.
  storeBrowse: {
    id: "shift.store-browse",
    layer: "template",
    name: "Store — Browse",
  },
  storeShelves: {
    id: "shift.store-shelves",
    layer: "template",
    name: "Store — Shelves",
  },
  storeIndex: {
    id: "shift.store-index",
    layer: "template",
    name: "Store — Index",
  },
  // Alternate refine presentation: a right-edge side sheet carrying the FULL
  // search + filter control set (maxed out to design around, then cut).
  storeDrawer: {
    id: "shift.store-drawer",
    layer: "template",
    name: "Store — Drawer",
  },
  storePanel: {
    id: "shift.store-panel",
    layer: "organism",
    name: "Store Panel",
  },
  storeBrowseTile: {
    id: "shift.store-browse-tile",
    layer: "molecule",
    name: "Store Browse Tile",
  },
  storeShelf: {
    id: "shift.store-shelf",
    layer: "organism",
    name: "Store Shelf",
  },
  storeIndexRow: {
    id: "shift.store-index-row",
    layer: "molecule",
    name: "Store Index Row",
  },
  // Store — the journey Korri can serve: providers, one search, one entry.
  store: { id: "shift.store", layer: "page", name: "Store" },
  storeDetail: {
    id: "shift.store-detail",
    layer: "template",
    name: "Store Detail",
  },
  storeGet: { id: "shift.store-get", layer: "organism", name: "Store Get" },
  storeProgress: {
    id: "shift.store-progress",
    layer: "atom",
    name: "Store Progress",
  },
  // Home body states (the Home page's non-Ready Data states; tagged so
  // pick-mode can select the body inside the Home page).
  homeLoading: {
    id: "shift.home-loading",
    layer: "organism",
    name: "Home Loading",
  },
  homeEmpty: {
    id: "shift.home-empty",
    layer: "organism",
    name: "Home Empty",
  },
  homeLoadError: {
    id: "shift.home-load-error",
    layer: "organism",
    name: "Home Load Error",
  },
  homeDefect: {
    id: "shift.home-defect",
    layer: "organism",
    name: "Home Defect",
  },
  // Save sync (ADR 0014).
  saveChoice: {
    id: "shift.save-choice",
    layer: "page",
    name: "Split Choice",
  },
  saveSideCard: {
    id: "shift.save-side-card",
    layer: "molecule",
    name: "Split Side Card",
  },
  saveReview: {
    id: "shift.save-review",
    layer: "page",
    name: "Split Review",
  },
  saveHistory: {
    id: "shift.save-history",
    layer: "page",
    name: "Save History",
  },
  saveRow: {
    id: "shift.save-row",
    layer: "molecule",
    name: "Save Row",
  },
  saveNotice: {
    id: "shift.save-notice",
    layer: "molecule",
    name: "Split Notice",
  },
} as const satisfies Record<string, ShiftDesignPart>

export function shiftDesignPartAttrs(
  part: ShiftDesignPart,
  instanceId?: string,
): Record<string, string> {
  return {
    "data-korri-part": part.id,
    "data-korri-layer": part.layer,
    "data-korri-name": part.name,
    ...(instanceId ? { "data-korri-instance-id": instanceId } : {}),
  }
}
