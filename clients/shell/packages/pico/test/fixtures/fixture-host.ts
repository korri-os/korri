/**
 * A host made of nothing.
 *
 * Pico must be developable and testable with no Korri running, so this
 * implements the whole treaty in memory and records what was asked for. If Pico
 * ever needs something this cannot provide, that is the signal the treaty — not
 * the fixture — has to change.
 */
import type {
  SurfaceGameplayOverlayPresentation,
  SurfaceAction,
  SurfaceModel,
} from "../../src/model"
import {
  PICO_ART_BLOCKS,
  PICO_ART_BRAMBLE,
  PICO_ART_CAVERN,
  PICO_ART_COMET,
  PICO_ART_KNIGHT,
  PICO_ART_SHORE,
  PICO_ART_SUMMIT,
  PICO_ART_TOWER,
} from "./sample-art"

/** The actions Korri publishes for a game when it has focus (FocusGame).
 * Hollow Knight carries one of each shape the treaty allows: plain,
 * destructive, and present-but-inert. */
export function fixtureGameActions(gameId: string): readonly SurfaceAction[] {
  if (gameId !== "hollow") return []
  return [
    { id: "verify", label: "Verify files", enabled: true },
    { id: "sd", label: "Move to SD", description: "No card inserted", enabled: false },
    { id: "remove", label: "Remove from device", enabled: true, destructive: true },
  ]
}

/**
 * A representative library: one game with art, one that resumes, one with a
 * real launch choice, and one with no art at all — the four shapes the home
 * screen has to hold at once, so a preview shows the awkward cases rather than
 * a tidy row that proves nothing.
 */
/**
 * A gameplay overlay as korrid publishes one: Korri's own Resume first, then a
 * plugin's group carrying every interaction kind the treaty allows, one of them
 * disabled with a reason, one destructive. Everything the overlay must draw.
 */
export const fixtureOverlay: SurfaceGameplayOverlayPresentation = {
  kind: "gameplay-overlay",
  title: "Hollow Knight",
  controls: [
    {
      id: "resume",
      label: "Continue playing",
      enabled: true,
      destructive: false,
      dismissOnSuccess: true,
      interaction: { kind: "command" },
    },
    {
      id: "quit",
      label: "Quit game",
      description: "Unsaved progress is lost.",
      enabled: true,
      destructive: true,
      dismissOnSuccess: true,
      interaction: { kind: "command" },
    },
  ],
  groups: [
    {
      id: "mgba",
      label: "mGBA",
      controls: [
        {
          id: "save",
          label: "Save state",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: { kind: "command" },
        },
        {
          id: "load",
          label: "Load state",
          enabled: false,
          disabledReason: "No save yet",
          destructive: false,
          dismissOnSuccess: true,
          interaction: { kind: "command" },
        },
        {
          id: "ff",
          label: "Fast forward",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: { kind: "toggle", value: false, trueLabel: "On", falseLabel: "Off" },
        },
        {
          id: "shader",
          label: "Shader",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: {
            kind: "choice",
            value: "none",
            options: [
              { value: "none", label: "None" },
              { value: "crt", label: "CRT" },
              { value: "lcd", label: "LCD" },
            ],
          },
        },
        {
          id: "vol",
          label: "Volume",
          enabled: true,
          destructive: false,
          dismissOnSuccess: false,
          interaction: { kind: "range", value: 80, min: 0, max: 100, step: 10 },
        },
      ],
    },
  ],
}

export const fixtureModel: SurfaceModel = {
  presentation: { kind: "catalog" },
  catalog: {
    _tag: "Ready",
    /* Covers at every shape Korri's platforms use (see fixtures/sample-art.ts),
     * one game with none, some resumable, one playable in two places. */
    games: [
      {
        id: "celeste",
        section: "Continue",
        title: "Celeste Classic",
        subtitle: "PICO-8 · This device",
        coverArtUrl: PICO_ART_SUMMIT,
      },
      {
        id: "hollow",
        section: "Continue",
        title: "Hollow Knight",
        subtitle: "Switch · This device",
        coverArtUrl: PICO_ART_KNIGHT,
        resumable: true,
        playCount: 3,
        totalPlaytimeSeconds: 7_800,
        lastPlayedAt: 1_757_000_000_000,
      },
      {
        id: "tetris",
        section: "zao",
        title: "Tetris",
        subtitle: "GB · zao",
        coverArtUrl: PICO_ART_BLOCKS,
        launchLocations: [
          { id: "local", label: "This device" },
          { id: "zao", label: "zao" },
        ],
      },
      {
        id: "spelunky",
        section: "This device",
        title: "Spelunky",
        subtitle: "PC · This device",
        coverArtUrl: PICO_ART_CAVERN,
      },
      {
        id: "lantern",
        section: "This device",
        title: "Lantern Keep",
        subtitle: "PC · This device",
        coverArtUrl: PICO_ART_TOWER,
        playCount: 12,
        totalPlaytimeSeconds: 41_400,
        lastPlayedAt: 1_756_400_000_000,
      },
      {
        id: "tide",
        section: "zao",
        title: "Tide Pool",
        subtitle: "Mega Drive · zao",
        coverArtUrl: PICO_ART_SHORE,
      },
      {
        id: "comet",
        section: "Continue",
        title: "Comet Courier",
        subtitle: "PC · This device",
        coverArtUrl: PICO_ART_COMET,
        resumable: true,
        playCount: 1,
        totalPlaytimeSeconds: 1_500,
        lastPlayedAt: 1_756_900_000_000,
      },
      {
        id: "bramble",
        section: "This device",
        title: "Bramble Run",
        subtitle: "GBA · This device",
        coverArtUrl: PICO_ART_BRAMBLE,
      },
      {
        id: "petal",
        section: "This device",
        title: "Petal Quest",
        subtitle: "PICO-8 · This device",
      },
    ],
  },
  status: { _tag: "Browsing" },
  clockLabel: "10:24",
  actions: [],
  /* Real shapes from korrid: a fact, an editable name, a two-value choice,
   * a folder scan action, and a destructive folder removal. */
  settings: [
    {
      title: "Device",
      items: [
        {
          id: "device-name",
          label: "Name",
          value: "usu",
          interaction: { kind: "text", maxLength: 64 },
        },
        { id: "software", label: "Software", value: "korrid 0.4.1" },
      ],
    },
    {
      title: "Plugins",
      items: [
        {
          id: "@korri:mgba",
          label: "mGBA",
          value: "On",
          interaction: {
            kind: "choice",
            choices: [
              { value: "true", label: "On" },
              { value: "false", label: "Off" },
            ],
            selected: "true",
          },
        },
      ],
    },
    {
      title: "Games",
      items: [
        {
          id: "game-folder-rescan",
          label: "Rescan game folders",
          value: "1 folder",
          interaction: { kind: "action", actionId: "game-folder-rescan" },
        },
        {
          id: "game-folder:loc-a",
          label: "GBA",
          value: "Registered",
          interaction: {
            kind: "action",
            actionId: "game-folder-remove:loc-a",
            destructive: true,
            confirmation: {
              title: "Remove game folder?",
              message: "Korri will remove games it added from this folder. Edited or hand-authored games stay.",
              confirmLabel: "Remove folder",
            },
          },
        },
      ],
    },
  ],
  settingsStatus: { _tag: "Idle" },
  buildLabel: "pico-dev",
}
