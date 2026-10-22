/**
 * Settings → Plugins as the portal publishes it, for previews and tests.
 *
 * Generated once from clients/portal/src/surface/plugin-repositories.ts
 * (`pluginSettingGroups`) with that module's fixtures, then pasted here: a
 * surface imports nothing from the portal, so the output is copied, not
 * imported. Regenerate it when the portal's plugin rows change.
 */
import type { SurfaceModel, SurfaceSettingGroup, SurfaceSettingsStatus } from "@contracts/surface/korri-surface"
import { fixtureModel } from "./fixture-host"

/** Row and action ids the previews put a status or a question on. */
export const pluginIds = {
  "add": "plugin-repository-add",
  "chiptuneRow": "plugin-release:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
  "chiptuneReview": "plugin-review:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins:%40ada%3Achiptune:1.0.0",
  "netplay": "plugin-toggle:%40ada%3Anetplay",
  "mgba": "plugin-toggle:%40korri%3Amgba",
  "removeRetro": "plugin-repository-remove:https%3A%2F%2Fgithub.com%2Fada%2Fretro-plugins"
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
        "value": "4 plugins"
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
        "value": "Not trusted",
        "description": "Publisher @mallory is not trusted on this device"
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

/** Chiptune inspected: its row offers Review and turn on. */
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
        "value": "4 plugins"
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
            "message": "Warning: Plays sound over the network to paired speakers.\nOpens network ports: TCP 7000, 7001; UDP 5353.\nRuns in the background: chiptune-daemon.service.\nAlso installs Audio core. Warning: Can change the system volume.\nAlso uses PipeWire bridge. You approved it before.",
            "confirmLabel": "Turn on"
          }
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
        "value": "Not trusted",
        "description": "Publisher @mallory is not trusted on this device"
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
        "value": "4 plugins"
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
        "value": "Not trusted",
        "description": "Publisher @mallory is not trusted on this device"
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

/** The official repository and seven added ones, some with very long addresses. */
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
        "value": "4 plugins"
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
        "value": "Not trusted",
        "description": "Publisher @mallory is not trusted on this device"
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
  many: withPlugins(manyGroups),
} satisfies Record<string, SurfaceModel>
