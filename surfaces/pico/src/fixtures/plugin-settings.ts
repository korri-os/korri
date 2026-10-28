/**
 * Settings → Plugins as the portal publishes it, for previews and tests.
 *
 * Generated from clients/portal/src/surface/plugin-repositories.ts
 * (`pluginSettingGroups`) and the states in
 * plugin-repositories.pico-states.ts, by
 * plugin-repositories.pico-fixtures.generate.ts: a surface imports nothing
 * from the portal, so the output is copied, not imported. Regenerate it when
 * the portal's plugin rows change; the portal's tests fail until it is.
 */
import type { SurfaceModel, SurfaceSettingGroup, SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import { fixtureModel } from "./fixture-host"

/** Row and action ids the previews put a status or a question on. */
export const pluginIds = {
  "add": "plugin-repository-add",
  "chiptuneRow": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
  "chiptuneReview": "plugin-review:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
  "chiptuneTrustKey": "plugin-trust:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0:%40ada",
  "chiptuneTrustLib": "plugin-trust:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0:%40lib",
  "minerRow": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9",
  "minerTrust": "plugin-trust:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9:%40mallory",
  "ditherRow": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
  "ditherReview": "plugin-review:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
  "netplay": "plugin-toggle:%40ada%3Anetplay",
  "mgba": "plugin-toggle:%40korri%3Amgba",
  "removeRetro": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
  "adaPublisher": "plugin-publisher:%40ada",
  "revokeAda": "plugin-publisher-revoke:%40ada"
} as const

/** Official repository not set up, nothing added. */
const officialOnlyGroups: readonly SurfaceSettingGroup[] = [
  {
    "title": "Plugins",
    "items": [
      {
        "id": "plugin-repository:official",
        "label": "Official plugins",
        "value": "Not set up yet",
        "description": "Korri's own plugins will appear here."
      },
      {
        "id": "plugin-repository-add",
        "label": "Add a repository",
        "description": "Type the https:// address the plugin maker gave you.",
        "interaction": {
          "kind": "text",
          "placeholder": "https://…",
          "maxLength": 512
        }
      }
    ]
  },
  {
    "title": "Other installed plugins",
    "items": [
      {
        "id": "plugin-toggle:%40korri%3Amgba",
        "label": "mGBA",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      }
    ]
  }
]

/** Two added repositories: one read, one unreachable; installs from a removed one; plugins installed with the image. */
const repositoriesGroups: readonly SurfaceSettingGroup[] = [
  {
    "title": "Plugins",
    "items": [
      {
        "id": "plugin-repository:official",
        "label": "Official plugins",
        "value": "Not set up yet",
        "description": "Korri's own plugins will appear here."
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "github.com/ada/retro-plugins",
        "value": "5 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "broken.example/catalog",
        "value": "Can't reach it"
      },
      {
        "id": "plugin-repository-add",
        "label": "Add a repository",
        "description": "Type the https:// address the plugin maker gave you.",
        "interaction": {
          "kind": "text",
          "placeholder": "https://…",
          "maxLength": 512
        }
      }
    ]
  },
  {
    "title": "github.com/ada/retro-plugins",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Install",
        "description": "Music player for tracker songs",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install 2.1.0",
        "description": "Pick the version to install.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3",
        "label": "CRT look",
        "value": "Install 2.0.3",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3"
        }
      },
      {
        "id": "plugin-toggle:%40ada%3Anetplay",
        "label": "Netplay",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9",
        "label": "Free coins",
        "value": "Install",
        "description": "Publisher @mallory is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
        "label": "Pocket dither",
        "value": "Install",
        "description": "Publisher @pocket is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from github.com/ada/retro-plugins. The plugin you installed from it stays, and still turns on and off under \"Installed from removed repositories\".",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "broken.example/catalog",
    "items": [
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog:status",
        "label": "Can't reach this repository",
        "description": "Could not reach broken.example: the catalog is not valid JSON (line 1, column 7)."
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from broken.example/catalog. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "Installed from removed repositories",
    "items": [
      {
        "id": "plugin-toggle:%40old%3Ashader",
        "label": "Shader pack",
        "value": "Off",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      },
      {
        "id": "plugin-toggle:%40old%3Apocket-dither-and-scanline-filter-collection",
        "label": "@old:pocket-dither-and-scanline-filter-collection",
        "value": "On",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      }
    ]
  },
  {
    "title": "Other installed plugins",
    "items": [
      {
        "id": "plugin-toggle:%40korri%3Amgba",
        "label": "mGBA",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-toggle:%40korri%3Aretroarch",
        "label": "RetroArch",
        "value": "Off",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      }
    ]
  }
]

/** Chiptune checked: its publishers are trusted, so the next press shows the report. */
const reviewingGroups: readonly SurfaceSettingGroup[] = [
  {
    "title": "Plugins",
    "items": [
      {
        "id": "plugin-repository:official",
        "label": "Official plugins",
        "value": "Not set up yet",
        "description": "Korri's own plugins will appear here."
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "github.com/ada/retro-plugins",
        "value": "5 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "broken.example/catalog",
        "value": "Can't reach it"
      },
      {
        "id": "plugin-repository-add",
        "label": "Add a repository",
        "description": "Type the https:// address the plugin maker gave you.",
        "interaction": {
          "kind": "text",
          "placeholder": "https://…",
          "maxLength": 512
        }
      }
    ]
  },
  {
    "title": "github.com/ada/retro-plugins",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Review and turn on",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-review:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
          "confirmation": {
            "title": "Turn on Chiptune?",
            "message": "Publisher @ada, trusted on this device. Key ada-plugins-1 QWRh IHNh bXBs ZSBr.\nWarning: Plays sound over the network to paired speakers.\nOpens network ports: TCP 7000, 7001; UDP 5353.\nRuns in the background: chiptune-daemon.service.\nAlso installs Audio core from @ada (trusted). Warning: Can change the system volume.\nAlso uses PipeWire bridge from @korri (configured). You approved it before.",
            "confirmLabel": "Turn on"
          }
        }
      },
      {
        "id": "plugin-review-publisher:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0:%40ada",
        "label": "Publisher @ada",
        "value": "Trusted",
        "description": "Key ada-plugins-1 QWRh IHNh bXBs ZSBr. You trusted it on this device."
      },
      {
        "id": "plugin-review-publisher:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0:%40korri",
        "label": "Publisher @korri",
        "value": "Configured",
        "description": "Key korri-plugins-1 S29y cmkg c2Ft cGxl. This device's configuration trusts it."
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install 2.1.0",
        "description": "Pick the version to install.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3",
        "label": "CRT look",
        "value": "Install 2.0.3",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3"
        }
      },
      {
        "id": "plugin-toggle:%40ada%3Anetplay",
        "label": "Netplay",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9",
        "label": "Free coins",
        "value": "Install",
        "description": "Publisher @mallory is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
        "label": "Pocket dither",
        "value": "Install",
        "description": "Publisher @pocket is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from github.com/ada/retro-plugins. The plugin you installed from it stays, and still turns on and off under \"Installed from removed repositories\".",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "broken.example/catalog",
    "items": [
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog:status",
        "label": "Can't reach this repository",
        "description": "Could not reach broken.example: the catalog is not valid JSON (line 1, column 7)."
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from broken.example/catalog. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "Installed from removed repositories",
    "items": [
      {
        "id": "plugin-toggle:%40old%3Ashader",
        "label": "Shader pack",
        "value": "Off",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      },
      {
        "id": "plugin-toggle:%40old%3Apocket-dither-and-scanline-filter-collection",
        "label": "@old:pocket-dither-and-scanline-filter-collection",
        "value": "On",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      }
    ]
  },
  {
    "title": "Other installed plugins",
    "items": [
      {
        "id": "plugin-toggle:%40korri%3Amgba",
        "label": "mGBA",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-toggle:%40korri%3Aretroarch",
        "label": "RetroArch",
        "value": "Off",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      }
    ]
  }
]

/** Netplay's approval went stale. */
const needsReviewGroups: readonly SurfaceSettingGroup[] = [
  {
    "title": "Plugins",
    "items": [
      {
        "id": "plugin-repository:official",
        "label": "Official plugins",
        "value": "Not set up yet",
        "description": "Korri's own plugins will appear here."
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "github.com/ada/retro-plugins",
        "value": "5 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "broken.example/catalog",
        "value": "Can't reach it"
      },
      {
        "id": "plugin-repository-add",
        "label": "Add a repository",
        "description": "Type the https:// address the plugin maker gave you.",
        "interaction": {
          "kind": "text",
          "placeholder": "https://…",
          "maxLength": 512
        }
      }
    ]
  },
  {
    "title": "github.com/ada/retro-plugins",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Install",
        "description": "Music player for tracker songs",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install 2.1.0",
        "description": "Pick the version to install.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3",
        "label": "CRT look",
        "value": "Install 2.0.3",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3"
        }
      },
      {
        "id": "plugin-toggle:%40ada%3Anetplay",
        "label": "Netplay",
        "value": "Needs review",
        "description": "It changed since you approved it. Check it again to turn it on.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Anetplay:0.4.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9",
        "label": "Free coins",
        "value": "Install",
        "description": "Publisher @mallory is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
        "label": "Pocket dither",
        "value": "Install",
        "description": "Publisher @pocket is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from github.com/ada/retro-plugins. The plugin you installed from it stays, and still turns on and off under \"Installed from removed repositories\".",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "broken.example/catalog",
    "items": [
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog:status",
        "label": "Can't reach this repository",
        "description": "Could not reach broken.example: the catalog is not valid JSON (line 1, column 7)."
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from broken.example/catalog. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "Installed from removed repositories",
    "items": [
      {
        "id": "plugin-toggle:%40old%3Ashader",
        "label": "Shader pack",
        "value": "Off",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      },
      {
        "id": "plugin-toggle:%40old%3Apocket-dither-and-scanline-filter-collection",
        "label": "@old:pocket-dither-and-scanline-filter-collection",
        "value": "On",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      }
    ]
  },
  {
    "title": "Other installed plugins",
    "items": [
      {
        "id": "plugin-toggle:%40korri%3Amgba",
        "label": "mGBA",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-toggle:%40korri%3Aretroarch",
        "label": "RetroArch",
        "value": "Off",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      }
    ]
  }
]

/** Free coins checked: its publisher is not trusted yet, so the next press trusts it. */
const trustNewGroups: readonly SurfaceSettingGroup[] = [
  {
    "title": "Plugins",
    "items": [
      {
        "id": "plugin-repository:official",
        "label": "Official plugins",
        "value": "Not set up yet",
        "description": "Korri's own plugins will appear here."
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "github.com/ada/retro-plugins",
        "value": "5 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "broken.example/catalog",
        "value": "Can't reach it"
      },
      {
        "id": "plugin-repository-add",
        "label": "Add a repository",
        "description": "Type the https:// address the plugin maker gave you.",
        "interaction": {
          "kind": "text",
          "placeholder": "https://…",
          "maxLength": 512
        }
      }
    ]
  },
  {
    "title": "github.com/ada/retro-plugins",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Install",
        "description": "Music player for tracker songs",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install 2.1.0",
        "description": "Pick the version to install.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3",
        "label": "CRT look",
        "value": "Install 2.0.3",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3"
        }
      },
      {
        "id": "plugin-toggle:%40ada%3Anetplay",
        "label": "Netplay",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9",
        "label": "Free coins",
        "value": "Trust @mallory",
        "description": "@mallory is not trusted yet. Check its key, then trust it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-trust:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9:%40mallory",
          "confirmation": {
            "title": "Trust @mallory?",
            "message": "Free coins comes from @mallory. This device has not trusted it before.\nKey mallory-1 TWFs bG9y eSBz YW1w.\nTrust it only if the plugin maker gave you this key.\nLater plugins from @mallory must have the same key.",
            "confirmLabel": "Trust"
          }
        }
      },
      {
        "id": "plugin-review-publisher:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9:%40mallory",
        "label": "Publisher @mallory",
        "value": "Not trusted yet",
        "description": "Key mallory-1 TWFs bG9y eSBz YW1w. Check it with the plugin maker before you trust it."
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
        "label": "Pocket dither",
        "value": "Install",
        "description": "Publisher @pocket is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from github.com/ada/retro-plugins. The plugin you installed from it stays, and still turns on and off under \"Installed from removed repositories\".",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "broken.example/catalog",
    "items": [
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog:status",
        "label": "Can't reach this repository",
        "description": "Could not reach broken.example: the catalog is not valid JSON (line 1, column 7)."
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from broken.example/catalog. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "Installed from removed repositories",
    "items": [
      {
        "id": "plugin-toggle:%40old%3Ashader",
        "label": "Shader pack",
        "value": "Off",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      },
      {
        "id": "plugin-toggle:%40old%3Apocket-dither-and-scanline-filter-collection",
        "label": "@old:pocket-dither-and-scanline-filter-collection",
        "value": "On",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      }
    ]
  },
  {
    "title": "Other installed plugins",
    "items": [
      {
        "id": "plugin-toggle:%40korri%3Amgba",
        "label": "mGBA",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-toggle:%40korri%3Aretroarch",
        "label": "RetroArch",
        "value": "Off",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      }
    ]
  }
]

/** Chiptune checked: signed by another key than the one this device trusts. */
const keyChangedGroups: readonly SurfaceSettingGroup[] = [
  {
    "title": "Plugins",
    "items": [
      {
        "id": "plugin-repository:official",
        "label": "Official plugins",
        "value": "Not set up yet",
        "description": "Korri's own plugins will appear here."
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "github.com/ada/retro-plugins",
        "value": "5 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "broken.example/catalog",
        "value": "Can't reach it"
      },
      {
        "id": "plugin-repository-add",
        "label": "Add a repository",
        "description": "Type the https:// address the plugin maker gave you.",
        "interaction": {
          "kind": "text",
          "placeholder": "https://…",
          "maxLength": 512
        }
      }
    ]
  },
  {
    "title": "github.com/ada/retro-plugins",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Trust the new key",
        "description": "The publisher key for @ada changed.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-trust:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0:%40ada",
          "destructive": true,
          "confirmation": {
            "title": "Trust the new key for @ada?",
            "message": "The publisher key for @ada changed.\nBefore: ada-plugins-1 QWRh IHNh bXBs ZSBr.\nNow: ada-plugins-2 QWRh IG5l dyBz YW1w.\nThis can mean the author replaced their key, or that someone else is publishing as them.\nTrust the new key only if the author told you about the change.",
            "confirmLabel": "Trust the new key"
          }
        }
      },
      {
        "id": "plugin-review-publisher:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0:%40ada",
        "label": "Publisher @ada",
        "value": "Key changed",
        "description": "Before ada-plugins-1 QWRh IHNh bXBs ZSBr. Now ada-plugins-2 QWRh IG5l dyBz YW1w."
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install 2.1.0",
        "description": "Pick the version to install.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3",
        "label": "CRT look",
        "value": "Install 2.0.3",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3"
        }
      },
      {
        "id": "plugin-toggle:%40ada%3Anetplay",
        "label": "Netplay",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9",
        "label": "Free coins",
        "value": "Install",
        "description": "Publisher @mallory is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
        "label": "Pocket dither",
        "value": "Install",
        "description": "Publisher @pocket is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from github.com/ada/retro-plugins. The plugin you installed from it stays, and still turns on and off under \"Installed from removed repositories\".",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "broken.example/catalog",
    "items": [
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog:status",
        "label": "Can't reach this repository",
        "description": "Could not reach broken.example: the catalog is not valid JSON (line 1, column 7)."
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from broken.example/catalog. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "Installed from removed repositories",
    "items": [
      {
        "id": "plugin-toggle:%40old%3Ashader",
        "label": "Shader pack",
        "value": "Off",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      },
      {
        "id": "plugin-toggle:%40old%3Apocket-dither-and-scanline-filter-collection",
        "label": "@old:pocket-dither-and-scanline-filter-collection",
        "value": "On",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      }
    ]
  },
  {
    "title": "Other installed plugins",
    "items": [
      {
        "id": "plugin-toggle:%40korri%3Amgba",
        "label": "mGBA",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-toggle:%40korri%3Aretroarch",
        "label": "RetroArch",
        "value": "Off",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      }
    ]
  }
]

/** Chiptune checked: it brings a plugin from a publisher not trusted yet. */
const bringsNewGroups: readonly SurfaceSettingGroup[] = [
  {
    "title": "Plugins",
    "items": [
      {
        "id": "plugin-repository:official",
        "label": "Official plugins",
        "value": "Not set up yet",
        "description": "Korri's own plugins will appear here."
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "github.com/ada/retro-plugins",
        "value": "5 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "broken.example/catalog",
        "value": "Can't reach it"
      },
      {
        "id": "plugin-repository-add",
        "label": "Add a repository",
        "description": "Type the https:// address the plugin maker gave you.",
        "interaction": {
          "kind": "text",
          "placeholder": "https://…",
          "maxLength": 512
        }
      }
    ]
  },
  {
    "title": "github.com/ada/retro-plugins",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Trust @lib",
        "description": "@lib is not trusted yet. Check its key, then trust it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-trust:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0:%40lib",
          "confirmation": {
            "title": "Trust @lib?",
            "message": "Chiptune needs Audio core from @lib. This device has not trusted it before.\nKey lib-1 TGli IHNh bXBs ZSBr.\nTrust it only if the plugin maker gave you this key.\nLater plugins from @lib must have the same key.",
            "confirmLabel": "Trust"
          }
        }
      },
      {
        "id": "plugin-review-publisher:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0:%40ada",
        "label": "Publisher @ada",
        "value": "Trusted",
        "description": "Key ada-plugins-1 QWRh IHNh bXBs ZSBr. You trusted it on this device."
      },
      {
        "id": "plugin-review-publisher:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0:%40lib",
        "label": "Publisher @lib",
        "value": "Not trusted yet",
        "description": "Key lib-1 TGli IHNh bXBs ZSBr. Check it with the plugin maker before you trust it."
      },
      {
        "id": "plugin-review-publisher:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0:%40korri",
        "label": "Publisher @korri",
        "value": "Configured",
        "description": "Key korri-plugins-1 S29y cmkg c2Ft cGxl. This device's configuration trusts it."
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install 2.1.0",
        "description": "Pick the version to install.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3",
        "label": "CRT look",
        "value": "Install 2.0.3",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3"
        }
      },
      {
        "id": "plugin-toggle:%40ada%3Anetplay",
        "label": "Netplay",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9",
        "label": "Free coins",
        "value": "Install",
        "description": "Publisher @mallory is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
        "label": "Pocket dither",
        "value": "Install",
        "description": "Publisher @pocket is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from github.com/ada/retro-plugins. The plugin you installed from it stays, and still turns on and off under \"Installed from removed repositories\".",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "broken.example/catalog",
    "items": [
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog:status",
        "label": "Can't reach this repository",
        "description": "Could not reach broken.example: the catalog is not valid JSON (line 1, column 7)."
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from broken.example/catalog. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "Installed from removed repositories",
    "items": [
      {
        "id": "plugin-toggle:%40old%3Ashader",
        "label": "Shader pack",
        "value": "Off",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      },
      {
        "id": "plugin-toggle:%40old%3Apocket-dither-and-scanline-filter-collection",
        "label": "@old:pocket-dither-and-scanline-filter-collection",
        "value": "On",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      }
    ]
  },
  {
    "title": "Other installed plugins",
    "items": [
      {
        "id": "plugin-toggle:%40korri%3Amgba",
        "label": "mGBA",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-toggle:%40korri%3Aretroarch",
        "label": "RetroArch",
        "value": "Off",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      }
    ]
  }
]

/** Pocket dither checked: not signed, so the approval opens with the disclosure. */
const unsignedGroups: readonly SurfaceSettingGroup[] = [
  {
    "title": "Plugins",
    "items": [
      {
        "id": "plugin-repository:official",
        "label": "Official plugins",
        "value": "Not set up yet",
        "description": "Korri's own plugins will appear here."
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "github.com/ada/retro-plugins",
        "value": "5 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "broken.example/catalog",
        "value": "Can't reach it"
      },
      {
        "id": "plugin-repository-add",
        "label": "Add a repository",
        "description": "Type the https:// address the plugin maker gave you.",
        "interaction": {
          "kind": "text",
          "placeholder": "https://…",
          "maxLength": 512
        }
      }
    ]
  },
  {
    "title": "github.com/ada/retro-plugins",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Install",
        "description": "Music player for tracker songs",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install 2.1.0",
        "description": "Pick the version to install.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3",
        "label": "CRT look",
        "value": "Install 2.0.3",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3"
        }
      },
      {
        "id": "plugin-toggle:%40ada%3Anetplay",
        "label": "Netplay",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9",
        "label": "Free coins",
        "value": "Install",
        "description": "Publisher @mallory is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
        "label": "Pocket dither",
        "value": "Review and turn on",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-review:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
          "destructive": true,
          "confirmation": {
            "title": "Turn on Pocket dither?",
            "message": "This plugin is not signed. Korri cannot confirm who made it, or that updates come from the same person. Install it only if you trust the address you got it from.\nPublisher @pocket, not signed.\nIt asks for no network ports and runs nothing in the background.",
            "confirmLabel": "Turn on"
          }
        }
      },
      {
        "id": "plugin-review-publisher:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0:%40pocket",
        "label": "Publisher @pocket",
        "value": "Not signed",
        "description": "This plugin is not signed. Korri cannot confirm who made it, or that updates come from the same person. Install it only if you trust the address you got it from."
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from github.com/ada/retro-plugins. The plugin you installed from it stays, and still turns on and off under \"Installed from removed repositories\".",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "broken.example/catalog",
    "items": [
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog:status",
        "label": "Can't reach this repository",
        "description": "Could not reach broken.example: the catalog is not valid JSON (line 1, column 7)."
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from broken.example/catalog. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "Installed from removed repositories",
    "items": [
      {
        "id": "plugin-toggle:%40old%3Ashader",
        "label": "Shader pack",
        "value": "Off",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      },
      {
        "id": "plugin-toggle:%40old%3Apocket-dither-and-scanline-filter-collection",
        "label": "@old:pocket-dither-and-scanline-filter-collection",
        "value": "On",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      }
    ]
  },
  {
    "title": "Other installed plugins",
    "items": [
      {
        "id": "plugin-toggle:%40korri%3Amgba",
        "label": "mGBA",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-toggle:%40korri%3Aretroarch",
        "label": "RetroArch",
        "value": "Off",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      }
    ]
  }
]

/** The publishers this device trusts: one configured, two trusted by its owner. */
const publishersGroups: readonly SurfaceSettingGroup[] = [
  {
    "title": "Plugins",
    "items": [
      {
        "id": "plugin-repository:official",
        "label": "Official plugins",
        "value": "Not set up yet",
        "description": "Korri's own plugins will appear here."
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "github.com/ada/retro-plugins",
        "value": "5 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "broken.example/catalog",
        "value": "Can't reach it"
      },
      {
        "id": "plugin-repository-add",
        "label": "Add a repository",
        "description": "Type the https:// address the plugin maker gave you.",
        "interaction": {
          "kind": "text",
          "placeholder": "https://…",
          "maxLength": 512
        }
      }
    ]
  },
  {
    "title": "github.com/ada/retro-plugins",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Install",
        "description": "Music player for tracker songs",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install 2.1.0",
        "description": "Pick the version to install.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3",
        "label": "CRT look",
        "value": "Install 2.0.3",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3"
        }
      },
      {
        "id": "plugin-toggle:%40ada%3Anetplay",
        "label": "Netplay",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9",
        "label": "Free coins",
        "value": "Install",
        "description": "Publisher @mallory is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
        "label": "Pocket dither",
        "value": "Install",
        "description": "Publisher @pocket is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from github.com/ada/retro-plugins. The plugin you installed from it stays, and still turns on and off under \"Installed from removed repositories\".",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "broken.example/catalog",
    "items": [
      {
        "id": "plugin-repository:https%3A%2F%2Fbroken.example%2Fcatalog:status",
        "label": "Can't reach this repository",
        "description": "Could not reach broken.example: the catalog is not valid JSON (line 1, column 7)."
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fbroken.example%2Fcatalog",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from broken.example/catalog. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "Installed from removed repositories",
    "items": [
      {
        "id": "plugin-toggle:%40old%3Ashader",
        "label": "Shader pack",
        "value": "Off",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      },
      {
        "id": "plugin-toggle:%40old%3Apocket-dither-and-scanline-filter-collection",
        "label": "@old:pocket-dither-and-scanline-filter-collection",
        "value": "On",
        "description": "From old.example/plugins",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      }
    ]
  },
  {
    "title": "Other installed plugins",
    "items": [
      {
        "id": "plugin-toggle:%40korri%3Amgba",
        "label": "mGBA",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-toggle:%40korri%3Aretroarch",
        "label": "RetroArch",
        "value": "Off",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      }
    ]
  },
  {
    "title": "Publishers",
    "items": [
      {
        "id": "plugin-publisher:%40korri",
        "label": "@korri",
        "value": "Configured",
        "description": "This device's configuration trusts it. Key korri-plugins-1 S29y cmkg c2Ft cGxl. 2 plugins installed from it."
      },
      {
        "id": "plugin-publisher:%40ada",
        "label": "@ada",
        "value": "Revoke",
        "description": "Trusted on this device. Key ada-plugins-1 QWRh IHNh bXBs ZSBr. 1 plugin installed from it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-publisher-revoke:%40ada",
          "destructive": true,
          "confirmation": {
            "title": "Revoke @ada?",
            "message": "Plugins from @ada stop starting on this device: Netplay.\nYou can still turn them off and remove them.\nTo use them again, you check and trust the key of @ada again.",
            "confirmLabel": "Revoke"
          }
        }
      },
      {
        "id": "plugin-publisher:%40pixel-collective-handheld",
        "label": "@pixel-collective-handheld",
        "value": "Revoke",
        "description": "Trusted on this device. Key pixel-collective-2026 UGl4 ZWwg Y29s bGVj. No plugins installed from it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-publisher-revoke:%40pixel-collective-handheld",
          "destructive": true,
          "confirmation": {
            "title": "Revoke @pixel-collective-handheld?",
            "message": "This device stops trusting @pixel-collective-handheld.\nTo install from @pixel-collective-handheld again, you check and trust its key again.",
            "confirmLabel": "Revoke"
          }
        }
      }
    ]
  }
]

/** Seven added repositories with long names, and more publishers than fit. */
const manyGroups: readonly SurfaceSettingGroup[] = [
  {
    "title": "Plugins",
    "items": [
      {
        "id": "plugin-repository:official",
        "label": "Official plugins",
        "value": "2 plugins",
        "description": "Always here. It can't be removed."
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "github.com/ada/retro-plugins",
        "value": "5 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fcodeberg.org%2Fpixel-collective%2Fkorri-handheld-plugins-for-small-screens",
        "label": "codeberg.org/pixel-collective/korri-handheld-plugins-for-small-screens",
        "value": "2 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fplugins.example.org",
        "label": "plugins.example.org",
        "value": "Checking…"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fraw.githubusercontent.com%2Fsomeone%2Faveryveryverylongrepositorynamewithoutanybreakpoints%2Fmain%2Fcatalog.json",
        "label": "raw.githubusercontent.com/someone/averyveryverylongrepositorynamewithoutanybreakpoints/main/catalog.json",
        "value": "1 plugin"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fgitlab.com%2Fmira%2Fkorri",
        "label": "gitlab.com/mira/korri",
        "value": "Can't reach it"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fcdn.example.net%2Fk%2Fplugins%2Fv1",
        "label": "cdn.example.net/k/plugins/v1",
        "value": "2 plugins"
      },
      {
        "id": "plugin-repository:https%3A%2F%2Fhomebrew.example%2Fkorri",
        "label": "homebrew.example/korri",
        "value": "1 plugin"
      },
      {
        "id": "plugin-repository-add",
        "label": "Add a repository",
        "description": "Type the https:// address the plugin maker gave you.",
        "interaction": {
          "kind": "text",
          "placeholder": "https://…",
          "maxLength": 512
        }
      }
    ]
  },
  {
    "title": "Official plugins",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fplugins.korri.example%2Fofficial:%40korri%3Atailscale:1.76.1",
        "label": "Tailscale",
        "value": "Install",
        "description": "Reach this device from your other devices",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fplugins.korri.example%2Fofficial:%40korri%3Atailscale:1.76.1"
        }
      },
      {
        "id": "plugin-toggle:%40korri%3Asyncthing",
        "label": "Syncthing",
        "value": "On",
        "description": "Keep saves in step with your other devices",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      }
    ]
  },
  {
    "title": "github.com/ada/retro-plugins",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Install",
        "description": "Music player for tracker songs",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install 2.1.0",
        "description": "Pick the version to install.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3",
        "label": "CRT look",
        "value": "Install 2.0.3",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Acrt:2.0.3"
        }
      },
      {
        "id": "plugin-toggle:%40ada%3Anetplay",
        "label": "Netplay",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9",
        "label": "Free coins",
        "value": "Install",
        "description": "Publisher @mallory is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40mallory%3Aminer:9.9.9"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0",
        "label": "Pocket dither",
        "value": "Install",
        "description": "Publisher @pocket is not trusted yet.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40pocket%3Adither:0.1.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from github.com/ada/retro-plugins. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "codeberg.org/pixel-collective/korri-handheld-plugins-for-small-screens",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fcodeberg.org%2Fpixel-collective%2Fkorri-handheld-plugins-for-small-screens:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Install",
        "description": "Music player for tracker songs",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fcodeberg.org%2Fpixel-collective%2Fkorri-handheld-plugins-for-small-screens:%40ada%3Achiptune:1.0.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fcodeberg.org%2Fpixel-collective%2Fkorri-handheld-plugins-for-small-screens:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fcodeberg.org%2Fpixel-collective%2Fkorri-handheld-plugins-for-small-screens:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fcodeberg.org%2Fpixel-collective%2Fkorri-handheld-plugins-for-small-screens",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fcodeberg.org%2Fpixel-collective%2Fkorri-handheld-plugins-for-small-screens",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from codeberg.org/pixel-collective/korri-handheld-plugins-for-small-screens. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "plugins.example.org",
    "items": [
      {
        "id": "plugin-repository:https%3A%2F%2Fplugins.example.org:status",
        "label": "Checking for plugins…"
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fplugins.example.org",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fplugins.example.org",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from plugins.example.org. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "raw.githubusercontent.com/someone/averyveryverylongrepositorynamewithoutanybreakpoints/main/catalog.json",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fraw.githubusercontent.com%2Fsomeone%2Faveryveryverylongrepositorynamewithoutanybreakpoints%2Fmain%2Fcatalog.json:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Install",
        "description": "Music player for tracker songs",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fraw.githubusercontent.com%2Fsomeone%2Faveryveryverylongrepositorynamewithoutanybreakpoints%2Fmain%2Fcatalog.json:%40ada%3Achiptune:1.0.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fraw.githubusercontent.com%2Fsomeone%2Faveryveryverylongrepositorynamewithoutanybreakpoints%2Fmain%2Fcatalog.json",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fraw.githubusercontent.com%2Fsomeone%2Faveryveryverylongrepositorynamewithoutanybreakpoints%2Fmain%2Fcatalog.json",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from raw.githubusercontent.com/someone/averyveryverylongrepositorynamewithoutanybreakpoints/main/catalog.json. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "gitlab.com/mira/korri",
    "items": [
      {
        "id": "plugin-repository:https%3A%2F%2Fgitlab.com%2Fmira%2Fkorri:status",
        "label": "Can't reach this repository",
        "description": "Could not reach gitlab.com"
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fgitlab.com%2Fmira%2Fkorri",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fgitlab.com%2Fmira%2Fkorri",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from gitlab.com/mira/korri. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "cdn.example.net/k/plugins/v1",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fcdn.example.net%2Fk%2Fplugins%2Fv1:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Install",
        "description": "Music player for tracker songs",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fcdn.example.net%2Fk%2Fplugins%2Fv1:%40ada%3Achiptune:1.0.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fcdn.example.net%2Fk%2Fplugins%2Fv1:%40ada%3Acrt:2.1.0",
        "label": "CRT look",
        "value": "Install 2.1.0",
        "description": "Pick the version to install.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fcdn.example.net%2Fk%2Fplugins%2Fv1:%40ada%3Acrt:2.1.0"
        }
      },
      {
        "id": "plugin-release:https%3A%2F%2Fcdn.example.net%2Fk%2Fplugins%2Fv1:%40ada%3Acrt:2.0.3",
        "label": "CRT look",
        "value": "Install 2.0.3",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fcdn.example.net%2Fk%2Fplugins%2Fv1:%40ada%3Acrt:2.0.3"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fcdn.example.net%2Fk%2Fplugins%2Fv1",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fcdn.example.net%2Fk%2Fplugins%2Fv1",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from cdn.example.net/k/plugins/v1. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "homebrew.example/korri",
    "items": [
      {
        "id": "plugin-release:https%3A%2F%2Fhomebrew.example%2Fkorri:%40ada%3Achiptune:1.0.0",
        "label": "Chiptune",
        "value": "Install",
        "description": "Music player for tracker songs",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-inspect:https%3A%2F%2Fhomebrew.example%2Fkorri:%40ada%3Achiptune:1.0.0"
        }
      },
      {
        "id": "plugin-repository-remove:https%3A%2F%2Fhomebrew.example%2Fkorri",
        "label": "Remove this repository",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-repository-remove:https%3A%2F%2Fhomebrew.example%2Fkorri",
          "destructive": true,
          "confirmation": {
            "title": "Remove this repository?",
            "message": "Korri stops showing plugins from homebrew.example/korri. You can add it again later.",
            "confirmLabel": "Remove"
          }
        }
      }
    ]
  },
  {
    "title": "Other installed plugins",
    "items": [
      {
        "id": "plugin-toggle:%40korri%3Amgba",
        "label": "mGBA",
        "value": "On",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "true"
        }
      },
      {
        "id": "plugin-toggle:%40korri%3Aretroarch",
        "label": "RetroArch",
        "value": "Off",
        "interaction": {
          "kind": "choice",
          "choices": [
            {
              "value": "true",
              "label": "On"
            },
            {
              "value": "false",
              "label": "Off"
            }
          ],
          "selected": "false"
        }
      }
    ]
  },
  {
    "title": "Publishers",
    "items": [
      {
        "id": "plugin-publisher:%40korri",
        "label": "@korri",
        "value": "Configured",
        "description": "This device's configuration trusts it. Key korri-plugins-1 S29y cmkg c2Ft cGxl. 2 plugins installed from it."
      },
      {
        "id": "plugin-publisher:%40ada",
        "label": "@ada",
        "value": "Revoke",
        "description": "Trusted on this device. Key ada-plugins-1 QWRh IHNh bXBs ZSBr. 1 plugin installed from it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-publisher-revoke:%40ada",
          "destructive": true,
          "confirmation": {
            "title": "Revoke @ada?",
            "message": "Plugins from @ada stop starting on this device: Netplay.\nYou can still turn them off and remove them.\nTo use them again, you check and trust the key of @ada again.",
            "confirmLabel": "Revoke"
          }
        }
      },
      {
        "id": "plugin-publisher:%40pixel-collective-handheld",
        "label": "@pixel-collective-handheld",
        "value": "Revoke",
        "description": "Trusted on this device. Key pixel-collective-2026 UGl4 ZWwg Y29s bGVj. No plugins installed from it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-publisher-revoke:%40pixel-collective-handheld",
          "destructive": true,
          "confirmation": {
            "title": "Revoke @pixel-collective-handheld?",
            "message": "This device stops trusting @pixel-collective-handheld.\nTo install from @pixel-collective-handheld again, you check and trust its key again.",
            "confirmLabel": "Revoke"
          }
        }
      },
      {
        "id": "plugin-publisher:%40averyveryverylongnamespacewithoutanybreakpointsatall",
        "label": "@averyveryverylongnamespacewithoutanybreakpointsatall",
        "value": "Revoke",
        "description": "Trusted on this device. Key averyveryverylongkeylabelwithoutanybreakpoints-2026 QXZl cnlW ZXJ5 TG9u. 1 plugin installed from it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-publisher-revoke:%40averyveryverylongnamespacewithoutanybreakpointsatall",
          "destructive": true,
          "confirmation": {
            "title": "Revoke @averyveryverylongnamespacewithoutanybreakpointsatall?",
            "message": "Plugins from @averyveryverylongnamespacewithoutanybreakpointsatall stop starting on this device: @averyveryverylongnamespacewithoutanybreakpointsatall:x.\nYou can still turn them off and remove them.\nTo use them again, you check and trust the key of @averyveryverylongnamespacewithoutanybreakpointsatall again.",
            "confirmLabel": "Revoke"
          }
        }
      },
      {
        "id": "plugin-publisher:%40mira",
        "label": "@mira",
        "value": "Revoke",
        "description": "Trusted on this device. Key mira-1 S2V5 IG5v. No plugins installed from it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-publisher-revoke:%40mira",
          "destructive": true,
          "confirmation": {
            "title": "Revoke @mira?",
            "message": "This device stops trusting @mira.\nTo install from @mira again, you check and trust its key again.",
            "confirmLabel": "Revoke"
          }
        }
      },
      {
        "id": "plugin-publisher:%40homebrew-hub",
        "label": "@homebrew-hub",
        "value": "Revoke",
        "description": "Trusted on this device. Key homebrew-hub-1 S2V5 IG5v . 1 plugin installed from it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-publisher-revoke:%40homebrew-hub",
          "destructive": true,
          "confirmation": {
            "title": "Revoke @homebrew-hub?",
            "message": "Plugins from @homebrew-hub stop starting on this device: @homebrew-hub:plugin.\nYou can still turn them off and remove them.\nTo use them again, you check and trust the key of @homebrew-hub again.",
            "confirmLabel": "Revoke"
          }
        }
      },
      {
        "id": "plugin-publisher:%40retro-friends",
        "label": "@retro-friends",
        "value": "Revoke",
        "description": "Trusted on this device. Key retro-friends-1 S2V5 IG5v d. No plugins installed from it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-publisher-revoke:%40retro-friends",
          "destructive": true,
          "confirmation": {
            "title": "Revoke @retro-friends?",
            "message": "This device stops trusting @retro-friends.\nTo install from @retro-friends again, you check and trust its key again.",
            "confirmLabel": "Revoke"
          }
        }
      },
      {
        "id": "plugin-publisher:%40scanline",
        "label": "@scanline",
        "value": "Revoke",
        "description": "Trusted on this device. Key scanline-1 S2V5 IG5v dC. 1 plugin installed from it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-publisher-revoke:%40scanline",
          "destructive": true,
          "confirmation": {
            "title": "Revoke @scanline?",
            "message": "Plugins from @scanline stop starting on this device: @scanline:plugin.\nYou can still turn them off and remove them.\nTo use them again, you check and trust the key of @scanline again.",
            "confirmLabel": "Revoke"
          }
        }
      },
      {
        "id": "plugin-publisher:%40tiny",
        "label": "@tiny",
        "value": "Revoke",
        "description": "Trusted on this device. Key tiny-1 S2V5 IG5v dCB. No plugins installed from it.",
        "interaction": {
          "kind": "action",
          "actionId": "plugin-publisher-revoke:%40tiny",
          "destructive": true,
          "confirmation": {
            "title": "Revoke @tiny?",
            "message": "This device stops trusting @tiny.\nTo install from @tiny again, you check and trust its key again.",
            "confirmLabel": "Revoke"
          }
        }
      }
    ]
  }
]

const deviceGroup = fixtureModel.settings.find(group => group.title === "Device")

/** A settings model with Device first, then the plugin groups, as on a device. */
const withPlugins = (groups: readonly SurfaceSettingGroup[], settingsStatus: SurfaceSettingsStatus = { _tag: "Idle" }): SurfaceModel => ({
  ...fixtureModel,
  settings: [...(deviceGroup === undefined ? [] : [deviceGroup]), ...groups],
  settingsStatus,
})

export const pluginModels = {
  officialOnly: withPlugins(officialOnlyGroups),
  adding: withPlugins(officialOnlyGroups, { _tag: "Saving", settingId: pluginIds.add }),
  invalidAddress: withPlugins(officialOnlyGroups, { _tag: "Problem", settingId: pluginIds.add, message: "Use an https:// address" }),
  unreachableAddress: withPlugins(officialOnlyGroups, {
    _tag: "Problem",
    settingId: pluginIds.add,
    message: "Could not reach plugins.example.org: no catalog at that address (HTTP 404). Nothing was saved.",
  }),
  alreadyAdded: withPlugins(repositoriesGroups, { _tag: "Problem", settingId: pluginIds.add, message: "Already added" }),
  repositories: withPlugins(repositoriesGroups),
  inspecting: withPlugins(repositoriesGroups, { _tag: "Saving", settingId: pluginIds.chiptuneRow }),
  reviewing: withPlugins(reviewingGroups),
  installing: withPlugins(reviewingGroups, { _tag: "Saving", settingId: pluginIds.chiptuneRow }),
  needsReview: withPlugins(needsReviewGroups),
  toggleRefused: withPlugins(repositoriesGroups, {
    _tag: "Problem",
    settingId: pluginIds.mgba,
    message: "mGBA could not be turned off: a game is running.",
  }),
  trustNew: withPlugins(trustNewGroups),
  trusting: withPlugins(trustNewGroups, { _tag: "Saving", settingId: pluginIds.minerRow }),
  trustRefused: withPlugins(trustNewGroups, {
    _tag: "Problem",
    settingId: pluginIds.minerRow,
    message: "remove the unsigned @mallory plugins first: @mallory:coins",
  }),
  keyChanged: withPlugins(keyChangedGroups),
  bringsNew: withPlugins(bringsNewGroups),
  unsigned: withPlugins(unsignedGroups),
  publishers: withPlugins(publishersGroups),
  revoking: withPlugins(publishersGroups, { _tag: "Saving", settingId: pluginIds.adaPublisher }),
  many: withPlugins(manyGroups),
} satisfies Record<string, SurfaceModel>
