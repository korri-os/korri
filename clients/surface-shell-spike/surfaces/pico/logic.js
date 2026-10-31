(() => {
  // ../../../../surfaces/pico/src/pico-detail-view.ts
  function picoStatsFor(game) {
    const stats = [];
    if (game.playCount !== undefined) {
      stats.push({
        figure: String(game.playCount),
        caption: game.playCount === 1 ? "PLAY" : "PLAYS"
      });
    }
    if (game.totalPlaytimeSeconds !== undefined) {
      stats.push({ figure: playtimeLabel(game.totalPlaytimeSeconds), caption: "PLAYED" });
    }
    return stats;
  }
  function picoDownloadLabel(game) {
    const download = game.download;
    if (download === undefined)
      return;
    if (download.progress !== undefined) {
      const percent = picoPercent(download.progress);
      return percent === undefined ? "DOWNLOADING" : `DOWNLOADING ${percent}%`;
    }
    if (download.queued === true)
      return "QUEUED";
    return download.bytes === undefined ? "DOWNLOAD NEEDED" : `${picoBytesLabel(download.bytes)} DOWNLOAD`;
  }
  function picoPercent(progress) {
    if (progress.total === undefined || progress.total <= 0)
      return;
    return Math.max(0, Math.min(100, Math.floor(progress.done / progress.total * 100)));
  }
  function picoBytesLabel(bytes) {
    const scale = (value, unit) => {
      const text = value < 10 ? value.toFixed(1) : String(Math.round(value));
      return `${text.endsWith(".0") ? text.slice(0, -2) : text} ${unit}`;
    };
    if (bytes >= 1e9)
      return scale(bytes / 1e9, "GB");
    if (bytes >= 1e6)
      return scale(bytes / 1e6, "MB");
    if (bytes >= 1000)
      return scale(bytes / 1000, "KB");
    return `${Math.round(bytes)} B`;
  }
  function playtimeLabel(seconds) {
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const rest = minutes % 60;
    if (hours === 0)
      return `${rest}M`;
    return rest === 0 ? `${hours}H` : `${hours}H ${rest}M`;
  }

  // ../../../../surfaces/pico/src/pico-initials.ts
  function picoInitials(title) {
    const words = title.trim().split(/\s+/).filter(Boolean);
    if (words.length === 0)
      return "??";
    if (words.length === 1)
      return (words[0] ?? "").slice(0, 2).toUpperCase();
    return words.slice(0, 2).map((word) => word.charAt(0)).join("").toUpperCase();
  }

  // ../../../../surfaces/pico/src/pico-palette.ts
  var of = (name, hex) => ({
    name,
    hex,
    rgb: [
      Number.parseInt(hex.slice(1, 3), 16),
      Number.parseInt(hex.slice(3, 5), 16),
      Number.parseInt(hex.slice(5, 7), 16)
    ]
  });
  var PICO_PALETTE = [
    of("black", "#000000"),
    of("navy", "#1d2b53"),
    of("maroon", "#7e2553"),
    of("green", "#008751"),
    of("brown", "#ab5236"),
    of("slate", "#5f574f"),
    of("silver", "#c2c3c7"),
    of("white", "#fff1e8"),
    of("red", "#ff004d"),
    of("orange", "#ffa300"),
    of("yellow", "#ffec27"),
    of("lime", "#00e436"),
    of("blue", "#29adff"),
    of("lilac", "#83769c"),
    of("pink", "#ff77a8"),
    of("peach", "#ffccaa")
  ];
  var PICO_SHELL_COLORS = [
    12,
    14,
    9,
    11,
    15,
    6,
    8,
    10,
    3,
    13
  ].map((index) => PICO_PALETTE[index]);
  function picoLuminance(color) {
    const [r, g, b] = color.rgb;
    return 0.299 * r + 0.587 * g + 0.114 * b;
  }

  // ../../../../surfaces/pico/src/pico-label.ts
  function hash(input) {
    let h = 2166136261;
    for (let index = 0;index < input.length; index += 1) {
      h ^= input.charCodeAt(index);
      h = Math.imul(h, 16777619);
    }
    return h >>> 0;
  }
  var INK_FLIP = 140;
  var SYSTEMS = {
    "PICO-8": { system: "pico8", shell: 4 },
    GB: { system: "gb", shell: 5 },
    GBC: { system: "gbc", shell: 7 },
    GBA: { system: "gba", shell: 0 },
    NES: { system: "nes", shell: 6 },
    FC: { system: "nes", shell: 6 },
    SNES: { system: "snes", shell: 9 },
    SFC: { system: "snes", shell: 9 },
    MD: { system: "md", shell: 8 },
    GENESIS: { system: "md", shell: 8 }
  };
  function picoCartLookFor(gameId, subtitle) {
    const label = picoLabelFor(gameId);
    const word = subtitle?.split("·")[0]?.trim().toUpperCase() ?? "";
    const known = SYSTEMS[word];
    const shell = known?.shell ?? label.shell;
    const sticker = label.sticker !== shell ? label.sticker : label.shell;
    const color = PICO_SHELL_COLORS[sticker];
    return {
      system: known?.system ?? "other",
      shell,
      sticker,
      ink: color !== undefined && picoLuminance(color) < INK_FLIP ? "light" : "dark"
    };
  }
  function picoLabelFor(gameId) {
    const h = hash(gameId);
    const shell = h % PICO_SHELL_COLORS.length;
    const offset = 1 + (h >>> 8) % (PICO_SHELL_COLORS.length - 1);
    const sticker = (shell + offset) % PICO_SHELL_COLORS.length;
    const stickerColor = PICO_SHELL_COLORS[sticker];
    return {
      shell,
      sticker,
      ink: stickerColor !== undefined && picoLuminance(stickerColor) < INK_FLIP ? "light" : "dark"
    };
  }

  // ../../../../surfaces/pico/src/pico-home-view.ts
  function picoHomeViewFromCatalog(catalog) {
    const cached = homeViews.get(catalog);
    if (cached !== undefined)
      return cached;
    const view = homeViewFrom(catalog);
    homeViews.set(catalog, view);
    return view;
  }
  var homeViews = new WeakMap;
  function homeViewFrom(catalog) {
    switch (catalog._tag) {
      case "Loading":
        return { _tag: "Loading" };
      case "Error":
        return { _tag: "Failed", message: catalog.message };
      case "Empty":
        return { _tag: "Empty" };
      case "Ready":
        return catalog.games.length === 0 ? { _tag: "Empty" } : {
          _tag: "Shelf",
          games: catalog.games.map((game) => ({
            id: game.id,
            title: game.title,
            ...game.subtitle === undefined ? {} : { subtitle: game.subtitle },
            ...game.coverArtUrl === undefined ? {} : { artUrl: game.coverArtUrl },
            ...game.resumable === undefined ? {} : { resumable: game.resumable },
            ...game.section === undefined ? {} : { section: game.section },
            ...game.lastPlayedAt === undefined ? {} : { lastPlayedAt: game.lastPlayedAt },
            ...game.playCount === undefined ? {} : { playCount: game.playCount },
            ...game.totalPlaytimeSeconds === undefined ? {} : { totalPlaytimeSeconds: game.totalPlaytimeSeconds },
            ...game.download === undefined ? {} : {
              download: {
                ...game.download.bytes === undefined ? {} : { bytes: game.download.bytes },
                ...game.download.queued === true ? { queued: true } : {},
                ...game.download.progress === undefined ? {} : {
                  progress: {
                    done: game.download.progress.done,
                    ...game.download.progress.total === undefined ? {} : { total: game.download.progress.total }
                  }
                }
              }
            },
            ...game.launchLocations === undefined ? {} : {
              locations: game.launchLocations.map((location) => ({
                id: location.id,
                label: location.label
              }))
            }
          }))
        };
    }
  }

  // ../../../../surfaces/pico/src/pico-library-view.ts
  var PICO_ALL_SECTIONS = "ALL";

  // ../../../../surfaces/pico/src/state/navigation.ts
  var STORE_PLACE = { view: "Providers", providerId: undefined, typing: undefined };
  var IDENTITY_FORM = { password: "", secret: "", bunkerUri: "", disposition: "transfer", confirmed: false };
  var INVITE_NOTICES = { noticed: [], showing: [], nextToken: 0 };
  function topLayer(nav) {
    if (nav.settings !== undefined)
      return "Settings";
    if (nav.detail !== undefined)
      return "Detail";
    if (nav.store)
      return "Store";
    if (nav.find)
      return "Find";
    return "Home";
  }
  var SETTINGS = { question: { _tag: "None" }, group: 0, scope: 0 };
  var HOME = { mode: "shelf", selectedGameId: undefined, menu: "Closed", focusOnReturn: "None", aim: undefined };
  function initialNavigation(initial) {
    return {
      home: initial?._tag === "Home" ? { ...HOME, mode: initial.mode ?? "shelf" } : HOME,
      find: initial?._tag === "Find",
      store: false,
      storePlace: STORE_PLACE,
      detail: initial?._tag === "Detail" ? { gameId: initial.gameId, question: { _tag: "None" } } : undefined,
      settings: initial?._tag === "Settings" ? SETTINGS : undefined,
      launch: undefined,
      search: {
        query: "",
        section: initial?._tag === "Find" ? initial.section ?? PICO_ALL_SECTIONS : PICO_ALL_SECTIONS,
        order: initial?._tag === "Find" ? initial.order ?? "korri" : "korri"
      },
      idle: { _tag: "Awake", activity: 0 },
      session: { _tag: "Idle" },
      invites: INVITE_NOTICES
    };
  }

  // ../../../../surfaces/pico/src/pico-fonts.ts
  var PICO_FONT_IDS = [
    "tiny5",
    "tom-thumb",
    "x11-4x6",
    "kenney-mini",
    "m3x6",
    "x11-5x7",
    "spleen-5x8",
    "m5x7",
    "monogram",
    "kenney-pixel"
  ];
  var PICO_FONTS = [
    {
      id: "tiny5",
      name: "Tiny5",
      family: "Pico Tiny5",
      rows: 7,
      capHeight: 5,
      xHeight: 4,
      by: "The Tiny5 Project Authors (Gissio)",
      license: "SIL Open Font License 1.1",
      licenseFile: "tiny5-OFL.txt"
    },
    {
      id: "tom-thumb",
      name: "Tom Thumb",
      family: "Pico Tom Thumb",
      rows: 6,
      capHeight: 5,
      xHeight: 4,
      by: "Brian Swetland and Robey Pointer",
      license: "CC0 1.0 (also offered under MIT and CC BY 3.0)"
    },
    {
      id: "x11-4x6",
      name: "X11 fixed 4x6",
      family: "Pico X11 fixed 4x6",
      rows: 6,
      capHeight: 5,
      xHeight: 4,
      by: "the X Consortium fixed fonts",
      license: "Public domain"
    },
    {
      id: "kenney-mini",
      name: "Kenney Mini",
      family: "Pico Kenney Mini",
      rows: 7,
      capHeight: 5,
      xHeight: 4,
      by: "Kenney (www.kenney.nl)",
      license: "CC0 1.0"
    },
    {
      id: "m3x6",
      name: "m3x6",
      family: "Pico m3x6",
      rows: 8,
      capHeight: 6,
      xHeight: 5,
      by: "Daniel Linssen",
      license: "Free to use with attribution"
    },
    {
      id: "x11-5x7",
      name: "X11 fixed 5x7",
      family: "Pico X11 fixed 5x7",
      rows: 7,
      capHeight: 6,
      xHeight: 4,
      by: "the X Consortium fixed fonts",
      license: "Public domain"
    },
    {
      id: "spleen-5x8",
      name: "Spleen 5x8",
      family: "Pico Spleen 5x8",
      rows: 8,
      capHeight: 6,
      xHeight: 5,
      by: "Frederic Cambus",
      license: "BSD 2-Clause",
      licenseFile: "spleen-LICENSE.txt"
    },
    {
      id: "m5x7",
      name: "m5x7",
      family: "Pico m5x7",
      rows: 9,
      capHeight: 7,
      xHeight: 5,
      by: "Daniel Linssen",
      license: "CC0 1.0 (attribution appreciated)"
    },
    {
      id: "monogram",
      name: "monogram",
      family: "Pico monogram",
      rows: 9,
      capHeight: 7,
      xHeight: 5,
      by: "datagoblin",
      license: "CC0 1.0"
    },
    {
      id: "kenney-pixel",
      name: "Kenney Pixel",
      family: "Pico Kenney Pixel",
      rows: 9,
      capHeight: 7,
      xHeight: 6,
      by: "Kenney (www.kenney.nl)",
      license: "CC0 1.0"
    }
  ];

  // ../../../../surfaces/pico/src/pico-text-draft.ts
  var picoTextDraft = (text) => ({ text, capitals: false, symbols: false });
  function picoDraftType(draft, character, maxLength) {
    return maxLength !== undefined && [...draft.text].length >= maxLength ? draft : { ...draft, text: draft.text + character };
  }
  var picoDraftBackspace = (draft) => ({ ...draft, text: [...draft.text].slice(0, -1).join("") });

  // ../../../../surfaces/pico/src/state/wifi.ts
  var WIFI_SSID_MAX = 32;
  var WIFI_PASSWORD_MAX = 63;
  var NOTHING = { _tag: "Nothing" };
  var passwordEntry = (network) => ({ _tag: "Password", networkId: network.id, ssid: network.ssid, draft: picoTextDraft(""), shown: false });
  var HIDDEN_NAME_ENTRY = { _tag: "HiddenName", draft: picoTextDraft("") };
  function scannedNetworks(korri) {
    const wifi = korri.network?.wifi;
    return wifi?._tag === "Available" ? wifi.networks : [];
  }
  function lastAttempt(korri) {
    const wifi = korri.network?.wifi;
    return wifi?._tag === "Available" ? wifi.attempt : { _tag: "Idle" };
  }
  function onNetwork(korri, ssid) {
    const connection = korri.network?.connection;
    return connection?._tag === "Wifi" && connection.ssid === ssid;
  }
  function pickNetwork(korri, networkId) {
    const network = scannedNetworks(korri).find((candidate) => candidate.id === networkId);
    if (network === undefined || onNetwork(korri, network.ssid) || lastAttempt(korri)._tag === "Connecting")
      return NOTHING;
    return network.secured && !network.known ? { _tag: "Edit", entry: passwordEntry(network) } : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: network.id } };
  }
  function submitEntry(entry) {
    switch (entry._tag) {
      case "HiddenName": {
        const ssid = entry.draft.text.trim();
        return ssid === "" ? NOTHING : { _tag: "Edit", entry: { _tag: "HiddenPassword", ssid, draft: picoTextDraft(""), shown: false } };
      }
      case "HiddenPassword":
        return {
          _tag: "Send",
          request: entry.draft.text === "" ? { _tag: "ConnectHiddenWifi", ssid: entry.ssid } : { _tag: "ConnectHiddenWifi", ssid: entry.ssid, password: entry.draft.text }
        };
      case "Password":
        return entry.draft.text === "" ? NOTHING : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: entry.networkId, password: entry.draft.text } };
    }
  }
  var entryMaxLength = (entry) => entry._tag === "HiddenName" ? WIFI_SSID_MAX : WIFI_PASSWORD_MAX;
  var editEntry = (entry, change) => ({ ...entry, draft: change(entry.draft) });
  var toggleShown = (entry) => entry._tag === "HiddenName" ? entry : { ...entry, shown: !entry.shown };

  // ../../../../surfaces/pico/src/pico-wifi-view.ts
  var PICO_SIGNAL_BAR_ABOVE = [5, 30, 55, 80];
  function picoSignalBars(strengthPercent) {
    return PICO_SIGNAL_BAR_ABOVE.filter((floor) => strengthPercent > floor).length;
  }
  function picoNetworksFrom(korri) {
    const connection = korri.network?.connection;
    return scannedNetworks(korri).map((network) => ({
      id: network.id,
      ssid: network.ssid,
      bars: picoSignalBars(network.strengthPercent),
      secured: network.secured,
      connected: connection?._tag === "Wifi" && connection.ssid === network.ssid
    }));
  }
  function picoConnectionLabel(connection) {
    switch (connection._tag) {
      case "Offline":
        return "Offline";
      case "Wired":
        return "Cable";
      case "Wifi":
        return connection.ssid;
    }
  }

  // ../../../../surfaces/pico/src/pico-settings-view.ts
  function picoTextSettingFrom(settings, settingId) {
    for (const group of settings.groups) {
      for (const row of group.rows) {
        if (row.id === settingId && row.control.kind === "text") {
          return { group: group.title, row };
        }
      }
    }
    return;
  }
  var PICO_IDENTITY_BACKUP_ACTION = "identity:backup";
  var PICO_IDENTITY_SWITCH_LOCAL_ACTION = "identity:switch:local";
  var PICO_IDENTITY_SWITCH_NIP46_ACTION = "identity:switch:nip46";
  var picoIdentityRetiredExportAction = (key) => `identity:retired:export:${key}`;
  var picoIdentityRetiredDeleteAction = (key) => `identity:retired:delete:${key}`;
  function picoIdentityRetiredKey(action, operation) {
    const prefix = `identity:retired:${operation}:`;
    return action?.startsWith(prefix) === true ? action.slice(prefix.length) : undefined;
  }
  var PICO_FONT_SETTING = "pico:font";
  var PICO_SEGMENTS_MAX = 4;
  var PICO_SEGMENTS_CHARS = 24;
  var PICO_WIFI_SCAN_ACTION = "pico:wifi:scan";
  var PICO_WIFI_HIDDEN_ACTION = "pico:wifi:hidden";
  var WIFI_NETWORK_PREFIX = "pico:wifi:network:";
  var picoWifiNetworkAction = (networkId) => `${WIFI_NETWORK_PREFIX}${networkId}`;
  function picoWifiNetworkId(actionId) {
    return actionId.startsWith(WIFI_NETWORK_PREFIX) ? actionId.slice(WIFI_NETWORK_PREFIX.length) : undefined;
  }
  function wifiGroup(model) {
    const network = model.network;
    if (network === undefined)
      return;
    const wifi = network.wifi;
    const attempt = wifi._tag === "Available" ? wifi.attempt : undefined;
    const connection = {
      id: "pico:wifi:connection",
      label: "Connection",
      value: attempt?._tag === "Connecting" ? `Joining ${attempt.ssid}…` : picoConnectionLabel(network.connection),
      control: { kind: "fact" },
      state: attempt?._tag === "Failed" ? { failed: attempt.message } : "idle"
    };
    if (wifi._tag === "NoAdapter") {
      return {
        title: "WI-FI",
        rows: [connection, { id: "pico:wifi:adapter", label: "Wi-Fi", value: "None on this device", control: { kind: "fact" }, state: "idle" }]
      };
    }
    const networks = picoNetworksFrom(model);
    return {
      title: "WI-FI",
      rows: [
        connection,
        {
          id: PICO_WIFI_SCAN_ACTION,
          label: "Scan for networks",
          value: wifi.scanning ? "Scanning…" : `${networks.length} found`,
          control: { kind: "action", actionId: PICO_WIFI_SCAN_ACTION, destructive: false },
          state: "idle"
        },
        ...networks.map((item) => ({
          id: picoWifiNetworkAction(item.id),
          label: item.ssid,
          control: { kind: "network", actionId: picoWifiNetworkAction(item.id), network: item },
          state: "idle"
        })),
        {
          id: PICO_WIFI_HIDDEN_ACTION,
          label: "Hidden network",
          description: "Join a network that does not show its name.",
          control: { kind: "action", actionId: PICO_WIFI_HIDDEN_ACTION, destructive: false },
          state: "idle"
        }
      ]
    };
  }
  function picoGroup(font) {
    const at = Math.max(0, PICO_FONTS.findIndex((candidate) => candidate.id === font));
    const current = PICO_FONTS[at];
    const next = PICO_FONTS[(at + 1) % PICO_FONTS.length];
    return {
      title: "PICO",
      rows: [{
        id: PICO_FONT_SETTING,
        label: "Font",
        description: "Kept on this device. Changes at once.",
        control: { kind: "step", next: next?.id ?? font, current: (current?.name ?? font).toUpperCase() },
        state: "idle"
      }]
    };
  }
  function picoSettingsViewFromModel(model, pico) {
    const identity = model.identityManagement;
    const identityRows = identity ? [
      ...identity.localBackupAvailable ? [{
        id: PICO_IDENTITY_BACKUP_ACTION,
        label: "Back up current identity",
        description: "Encrypted NIP-49 text and QR code",
        control: { kind: "action", actionId: PICO_IDENTITY_BACKUP_ACTION, destructive: false },
        state: "idle"
      }] : [],
      {
        id: PICO_IDENTITY_SWITCH_LOCAL_ACTION,
        label: "Switch from backup",
        control: { kind: "action", actionId: PICO_IDENTITY_SWITCH_LOCAL_ACTION, destructive: true },
        state: "idle"
      },
      {
        id: PICO_IDENTITY_SWITCH_NIP46_ACTION,
        label: "Switch to NIP-46",
        control: { kind: "action", actionId: PICO_IDENTITY_SWITCH_NIP46_ACTION, destructive: true },
        state: "idle"
      },
      ...identity.retiredPublicKeys.flatMap((publicKey) => {
        const short = `${publicKey.slice(0, 8)}…${publicKey.slice(-8)}`;
        return [{
          id: picoIdentityRetiredExportAction(publicKey),
          label: `Back up retired ${short}`,
          control: { kind: "action", actionId: picoIdentityRetiredExportAction(publicKey), destructive: false },
          state: "idle"
        }, {
          id: picoIdentityRetiredDeleteAction(publicKey),
          label: `Delete retired ${short}`,
          control: { kind: "action", actionId: picoIdentityRetiredDeleteAction(publicKey), destructive: true },
          state: "idle"
        }];
      })
    ] : [];
    const wifi = wifiGroup(model);
    return {
      groups: [
        ...model.settings.map((group) => ({
          title: group.title.toUpperCase(),
          rows: group.items.map((item) => picoSettingRowFrom(item, model))
        })),
        ...wifi === undefined ? [] : [wifi],
        ...identityRows.length ? [{ title: "IDENTITY", rows: identityRows }] : [],
        ...pico === undefined ? [] : [picoGroup(pico.font)]
      ],
      ...model.buildLabel === undefined ? {} : { buildLabel: model.buildLabel }
    };
  }
  function picoNumberRange(range) {
    if (range.min !== undefined && range.max !== undefined)
      return `${range.min} to ${range.max}`;
    if (range.min !== undefined)
      return `At least ${range.min}`;
    if (range.max !== undefined)
      return `At most ${range.max}`;
    return;
  }
  function picoNumberFits(text, range) {
    const trimmed = text.trim();
    if (trimmed === "" || !/^-?\d*\.?\d+$/.test(trimmed))
      return false;
    const value = Number(trimmed);
    if (range.step === 1 && !Number.isInteger(value))
      return false;
    return (range.min === undefined || value >= range.min) && (range.max === undefined || value <= range.max);
  }
  function picoSettingRowFrom(item, model) {
    const status = model.settingsStatus;
    const state = status._tag === "Saving" && status.settingId === item.id ? "saving" : status._tag === "Problem" && status.settingId === item.id ? { problem: status.message } : "idle";
    return {
      id: item.id,
      label: item.label,
      ...item.value === undefined ? {} : { value: item.value },
      ...item.description === undefined ? {} : { description: item.description },
      ...item.source === undefined ? {} : { source: item.source },
      ...item.scope === "device" ? { deviceOnly: true } : {},
      control: controlFor(item),
      state
    };
  }
  function controlFor(item) {
    const interaction = item.interaction;
    if (interaction === undefined)
      return { kind: "fact" };
    switch (interaction.kind) {
      case "action":
        return {
          kind: "action",
          actionId: interaction.actionId,
          destructive: interaction.destructive === true,
          ...interaction.confirmation === undefined ? {} : {
            confirmation: { ...interaction.confirmation, destructive: interaction.destructive === true }
          }
        };
      case "choice": {
        const choices = interaction.choices;
        const found = choices.findIndex((choice) => choice.value === interaction.selected);
        if (found < 0)
          return { kind: "pick", options: choices.map(({ value, label }) => ({ value, label })) };
        const current = found;
        const nextChoice = choices[(current + 1) % choices.length];
        const width = choices.reduce((sum, choice) => sum + choice.label.length + 1, 0);
        if (choices.length > PICO_SEGMENTS_MAX || width > PICO_SEGMENTS_CHARS) {
          return { kind: "step", next: nextChoice?.value ?? "", current: choices[current]?.label ?? "" };
        }
        return {
          kind: "cycle",
          next: nextChoice?.value ?? "",
          options: choices.map(({ value, label }) => ({ value, label })),
          current
        };
      }
      case "number":
        return {
          kind: "text",
          sensitive: false,
          initial: interaction.value === undefined ? "" : String(interaction.value),
          numeric: {
            ...interaction.min === undefined ? {} : { min: interaction.min },
            ...interaction.max === undefined ? {} : { max: interaction.max },
            ...interaction.step === undefined ? {} : { step: interaction.step }
          },
          placeholder: picoNumberRange(interaction) ?? "Number",
          maxLength: 24,
          ...interaction.clearable === true && interaction.value !== undefined ? { clearLabel: "Inherit" } : {}
        };
      case "text":
        return {
          kind: "text",
          sensitive: false,
          ...interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder },
          ...interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength },
          ...interaction.allowEmpty === true ? { allowEmpty: true } : {}
        };
      case "sensitiveText":
        return {
          kind: "text",
          sensitive: true,
          ...interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder },
          ...interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength },
          ...interaction.clearLabel === undefined ? {} : { clearLabel: interaction.clearLabel }
        };
    }
  }

  // ../../../../surfaces/pico/src/pico-launch-view.ts
  var picoLaunchOpen = (model) => model.launchSettings !== undefined && model.launchSettings._tag !== "Closed";
  function picoLaunchViewFromModel(model) {
    const launch = model.launchSettings;
    if (launch === undefined || launch._tag === "Closed")
      return;
    const close = launch.actions.find((action) => action.id.endsWith(":cancel"));
    const layers = [];
    for (const place of launch.places) {
      const rows = place.groups.flatMap((group) => group.items.map((item, index) => {
        const row = picoSettingRowFrom(item, model);
        return index === 0 ? { ...row, section: group.title.toUpperCase() } : row;
      }));
      const last = layers.at(-1);
      const scope = { placeId: place.id, label: place.scope.label, kind: place.scope.kind, rows };
      if (last !== undefined && last.id === place.layer.id)
        last.scopes.push(scope);
      else
        layers.push({ id: place.layer.id, title: place.layer.title, scopes: [scope] });
    }
    return {
      ...launch.gameTitle === undefined ? {} : { gameTitle: launch.gameTitle },
      phase: launch._tag,
      ...launch._tag === "Conflict" || launch._tag === "Error" ? { message: launch.message } : {},
      notes: launch.notes,
      actions: launch.actions,
      ...close === undefined ? {} : { closeActionId: close.id },
      layers,
      settings: {
        groups: layers.map((layer) => ({ title: layer.title.toUpperCase(), rows: layer.scopes.flatMap((scope) => scope.rows) }))
      }
    };
  }

  // ../../../../surfaces/pico/src/pico-screen-view.ts
  function picoScreenViewFromModel(model) {
    const status = model.status;
    switch (status._tag) {
      case "Busy": {
        const cart = cartFor(model, status.gameId);
        return {
          _tag: "Busy",
          kicker: status.kicker,
          ...status.detail === undefined ? {} : { detail: status.detail },
          ...status.progress === undefined ? {} : { progress: status.progress },
          ...status.actions === undefined ? {} : { actions: status.actions },
          ...cart === undefined ? {} : { cart }
        };
      }
      case "Confirm": {
        const cart = cartFor(model, status.gameId);
        return {
          _tag: "Confirm",
          kicker: status.kicker,
          ...status.detail === undefined ? {} : { detail: status.detail },
          actions: status.actions,
          ...status.declineActionId === undefined ? {} : { declineActionId: status.declineActionId },
          ...cart === undefined ? {} : { cart }
        };
      }
      case "ChooseSave": {
        const cart = cartFor(model, status.gameId);
        return {
          _tag: "Confirm",
          kicker: status.kicker,
          ...status.detail === undefined ? {} : { detail: status.detail },
          actions: picoChooseSaveActions(status),
          ...cart === undefined ? {} : { cart }
        };
      }
      case "Running": {
        const cart = cartFor(model, status.gameId);
        return {
          _tag: "Running",
          kicker: status.kicker,
          ...cart === undefined ? {} : { gameTitle: cart.title, cart }
        };
      }
      case "Problem":
        return {
          _tag: "Problem",
          kicker: status.kicker,
          reason: status.reason,
          canRetry: status.canRetry,
          ...status.gameTitle === undefined ? {} : { gameTitle: status.gameTitle }
        };
      case "Paused": {
        const cart = cartFor(model, status.gameId);
        return {
          _tag: "Paused",
          kicker: status.kicker,
          ...status.detail === undefined ? {} : { detail: status.detail },
          actions: status.actions,
          ...cart === undefined ? {} : { cart }
        };
      }
      case "Ended":
        return { _tag: "Ended", kicker: status.kicker, reason: status.reason };
      case "Browsing":
        return picoHomeViewFromCatalog(model.catalog);
    }
  }
  var MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
  var two = (n) => String(n).padStart(2, "0");
  function picoSaveTime(at) {
    const date = new Date(at);
    if (Number.isNaN(date.getTime()))
      return "--";
    return `${MONTHS[date.getMonth()] ?? ""} ${date.getDate()} ${two(date.getHours())}:${two(date.getMinutes())}`;
  }
  function picoChooseSaveActions(status) {
    return [
      ...status.sides.map((side) => ({
        id: side.actionId,
        label: `${side.device}, ${picoSaveTime(side.endedAt)}`,
        enabled: true
      })),
      ...status.actions
    ];
  }
  function cartFor(model, gameId) {
    if (gameId === undefined)
      return;
    if (model.catalog._tag !== "Ready")
      return;
    const game = model.catalog.games.find((candidate) => candidate.id === gameId);
    if (game === undefined)
      return;
    return {
      id: game.id,
      title: game.title,
      ...game.coverArtUrl === undefined ? {} : { artUrl: game.coverArtUrl }
    };
  }

  // ../../../../surfaces/pico/src/state/shown.ts
  function shownScreen(nav, korri) {
    const view = picoScreenViewFromModel(korri);
    const browsing = korri.status._tag === "Browsing";
    const game = nav.detail !== undefined && view._tag === "Shelf" && korri.catalog._tag === "Ready" ? korri.catalog.games.find((candidate) => candidate.id === nav.detail?.gameId) : undefined;
    if (nav.settings !== undefined && browsing)
      return { _tag: "Settings", settings: nav.settings };
    if (nav.store && browsing && game === undefined && korri.store !== undefined) {
      return { _tag: "Store", place: nav.storePlace, store: korri.store };
    }
    if (nav.find && browsing && game === undefined)
      return { _tag: "Find", search: nav.search };
    if (nav.detail !== undefined && game !== undefined)
      return { _tag: "Detail", detail: nav.detail, game };
    return { _tag: "Home", home: nav.home, view };
  }
  var runnerOpen = (korri) => korri.runnerChoice !== undefined && korri.runnerChoice._tag !== "Closed";
  var launchOpen = picoLaunchOpen;
  function canAttract(nav, korri) {
    const shown = shownScreen(nav, korri);
    return !runnerOpen(korri) && !launchOpen(korri) && shown._tag === "Home" && shown.view._tag === "Shelf" && nav.home.menu === "Closed" && nav.detail === undefined;
  }
  var invitesNoticeable = (korri) => korri.presentation.kind === "catalog" && korri.onboarding === undefined;
  var attractShowing = (nav, korri) => nav.idle._tag === "Attracting" && canAttract(nav, korri);
  var identityAction = (nav) => nav.settings?.question._tag === "Identity" ? nav.settings.question.actionId : null;

  // ../../../../surfaces/pico/src/pico-font-preference.ts
  function isPicoFont(value) {
    return PICO_FONT_IDS.includes(value);
  }

  // ../../../../surfaces/pico/src/pico-invite-view.ts
  var PICO_INVITE_NOTICE_MS = 8000;
  function picoInviteActionPublished(model, actionId) {
    return (model.invites ?? []).some((invite) => invite.actions.some((action) => action.id === actionId && action.enabled));
  }

  // ../../../../surfaces/pico/src/pico-session-return.ts
  function picoSessionReturnOnPlay(previous, gameId, catalog) {
    if (previous._tag === "Watching" && previous.viewingId === gameId)
      return previous;
    if (catalog._tag !== "Ready")
      return { _tag: "Idle" };
    return catalog.games.some((game) => game.id === gameId && game.resumable === true) ? { _tag: "Watching", viewingId: gameId, sessionId: gameId } : { _tag: "AwaitingSession", viewingId: gameId, catalog };
  }
  function picoSessionReturnFromModel(previous, viewingId, model) {
    const idle = { _tag: "Idle" };
    if (viewingId === undefined)
      return idle;
    const state = "viewingId" in previous && previous.viewingId === viewingId ? previous : idle;
    const { catalog, status } = model;
    if (status._tag === "Problem" && state._tag === "AwaitingSession")
      return idle;
    if (catalog._tag !== "Ready" || status._tag !== "Browsing")
      return state;
    const resumable = catalog.games.filter((game) => game.resumable === true);
    if (state._tag === "Watching") {
      if (catalog.games.some((game) => game.id === state.sessionId))
        return state;
      return resumable.length === 0 ? { _tag: "ReturnToLibrary" } : idle;
    }
    if (state._tag === "AwaitingSession") {
      if (catalog === state.catalog)
        return state;
      const added = resumable.filter((game) => !state.catalog.games.some((old) => old.id === game.id));
      const session = added.length === 1 ? added[0] : undefined;
      return session === undefined ? idle : { _tag: "Watching", viewingId, sessionId: session.id };
    }
    return resumable.some((game) => game.id === viewingId) ? { _tag: "Watching", viewingId, sessionId: viewingId } : idle;
  }

  // ../../../../surfaces/pico/src/pico-store-view.ts
  var PICO_STORE_QUERY_MAX = 64;

  // ../../../../surfaces/pico/src/state/messages.ts
  var isHostButton = (message) => message._tag === "PressedBack" || message._tag === "PressedSystem" || message._tag === "PressedOptions" || message._tag === "PressedMenu";

  // ../../../../surfaces/pico/src/state/requests.ts
  var stay = (model) => ({ model, requests: [] });
  var ask = (model, ...requests) => ({ model, requests });

  // ../../../../surfaces/pico/src/state/update.ts
  function update(nav, message, korri) {
    const result = step(settleLaunch(endNotices(nav, message), korri), message, korri);
    const settled = settleIdle(settleEditor(settleLaunch(settleSession(result.model, korri), korri), korri), korri);
    const identity = settleIdentity(nav, settled, korri);
    const invites = settleInvites(identity.model, korri);
    return { model: invites.model, requests: [...result.requests, ...identity.requests, ...invites.requests] };
  }
  function step(nav, message, korri) {
    if (runnerOpen(korri) && isHostButton(message)) {
      return message._tag === "PressedBack" ? ask(nav, { _tag: "RunAction", actionId: "runner:cancel" }) : stay(nav);
    }
    if (launchOpen(korri) && isHostButton(message)) {
      if (message._tag !== "PressedBack")
        return stay(nav);
      if (nav.launch !== undefined && nav.launch.question._tag !== "None")
        return stay(backInSettings(nav, nav.launch));
      const close = picoLaunchViewFromModel(korri)?.closeActionId;
      return close === undefined ? stay(nav) : ask(nav, { _tag: "RunAction", actionId: close });
    }
    if (isHostButton(message) || message._tag === "PressedScreen" || message._tag === "MovedFocus") {
      const waking = attractShowing(nav, korri);
      const activity = nav.idle.activity + 1;
      nav = { ...nav, idle: { _tag: "Awake", activity } };
      if (waking || !isHostButton(message))
        return stay(nav);
    }
    if (isHostButton(message) && korri.status._tag !== "Browsing") {
      return message._tag === "PressedBack" && (korri.status._tag === "Problem" || korri.status._tag === "Ended") ? ask(nav, { _tag: "DismissProblem" }) : stay(nav);
    }
    switch (message._tag) {
      case "PressedBack":
        return stay(back(nav));
      case "PressedSystem":
        return stay(nav.settings !== undefined ? { ...nav, settings: undefined } : { ...nav, settings: SETTINGS, home: closeMenu(nav.home) });
      case "PressedOptions": {
        const top = topLayer(nav);
        if (top === "Find")
          return stay({ ...nav, find: false });
        if (top === "Home")
          return stay({ ...nav, find: true, home: closeMenu(nav.home) });
        return stay(nav);
      }
      case "PressedMenu":
        return stay(topLayer(nav) === "Home" ? cycleMode(nav) : nav);
      case "KorriPublished":
        return stay(nav);
      case "IdleElapsed":
        return stay(nav.idle._tag === "Awake" && canAttract(nav, korri) ? { ...nav, idle: { _tag: "Attracting", activity: nav.idle.activity } } : nav);
      case "ChoseRunnerAction":
        return ask(nav, { _tag: "RunAction", actionId: message.actionId });
      case "PressedLaunchAction":
        return picoLaunchViewFromModel(korri)?.actions.some((action) => action.id === message.actionId && action.enabled) === true ? ask(nav, { _tag: "RunAction", actionId: message.actionId }) : stay(nav);
      case "ToggledHomeMenu":
        return stay({ ...nav, home: { ...nav.home, menu: nav.home.menu === "Open" ? "Closed" : "Open" } });
      case "ChoseMenuFind":
        return stay({ ...nav, find: true, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } });
      case "ChoseMenuSettings":
        return stay({ ...nav, settings: SETTINGS, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } });
      case "ChoseMenuView":
        return stay(cycleMode(nav));
      case "ChoseMenuStore":
        return korri.store === undefined ? stay(nav) : ask({ ...nav, store: true, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } }, { _tag: "OpenStore" });
      case "AimedMenu":
        return stay(nav.home.aim === message.label ? nav : { ...nav, home: { ...nav.home, aim: message.label } });
      case "ReturnedFocus":
        return stay({ ...nav, home: { ...nav.home, focusOnReturn: "None" } });
      case "SelectedCart":
        return stay({ ...nav, home: { ...nav.home, selectedGameId: message.gameId } });
      case "OpenedGame": {
        const detail = { gameId: message.gameId, question: { _tag: "None" } };
        return stay(topLayer(nav) === "Home" ? { ...nav, detail, home: { ...closeMenu(nav.home), focusOnReturn: nav.home.mode === "shelf" ? "Cart" : "None" } } : { ...nav, detail });
      }
      case "PressedRetry":
        return ask(nav, picoScreenViewFromModel(korri)._tag === "Problem" ? { _tag: "Retry" } : { _tag: "Reload" });
      case "PressedDismiss":
        return ask(nav, { _tag: "DismissProblem" });
      case "PressedStatusAction": {
        const view = picoScreenViewFromModel(korri);
        return (view._tag === "Busy" || view._tag === "Confirm" || view._tag === "Paused") && (view.actions ?? []).some((action) => action.id === message.actionId && action.enabled) ? ask(nav, { _tag: "RunAction", actionId: message.actionId }) : stay(nav);
      }
      case "PressedInviteAction":
        return korri.status._tag === "Browsing" && picoInviteActionPublished(korri, message.actionId) ? ask(nav, { _tag: "RunAction", actionId: message.actionId }) : stay(nav);
      case "LostInviteFocus":
        return stay(topLayer(nav) === "Home" && nav.home.mode === "shelf" ? { ...nav, home: { ...nav.home, focusOnReturn: "Cart" } } : nav);
      case "TypedCharacter":
        return stay({ ...nav, search: { ...nav.search, query: nav.search.query + message.character } });
      case "PressedBackspace":
        return stay({ ...nav, search: { ...nav.search, query: nav.search.query.slice(0, -1) } });
      case "ClearedQuery":
        return stay({ ...nav, search: { ...nav.search, query: "" } });
      case "ChoseSection":
        return stay({ ...nav, search: { ...nav.search, section: message.section } });
      case "ChoseOrder":
        return stay({ ...nav, search: { ...nav.search, order: message.order } });
      case "ChoseStoreProvider": {
        const providers = korri.store?.providers;
        if (providers?._tag !== "Ready" || !providers.providers.some((provider) => provider.id === message.providerId))
          return stay(nav);
        const search = korri.store?.search;
        const searched = search !== undefined && search._tag !== "Idle" && search.providerId === message.providerId;
        return stay(withStore(nav, {
          view: "Results",
          providerId: message.providerId,
          typing: searched ? undefined : picoTextDraft("")
        }));
      }
      case "OpenedStoreSearch": {
        const search = korri.store?.search;
        const query = search !== undefined && search._tag !== "Idle" && search.providerId === nav.storePlace.providerId ? search.query : "";
        return nav.storePlace.view === "Results" ? stay(withStore(nav, { ...nav.storePlace, typing: picoTextDraft(query) })) : stay(nav);
      }
      case "ClosedStoreSearch":
        return stay(withStore(nav, { ...nav.storePlace, typing: undefined }));
      case "SubmittedStoreSearch": {
        const { providerId, typing } = nav.storePlace;
        if (providerId === undefined || typing === undefined)
          return stay(nav);
        return ask(withStore(nav, { ...nav.storePlace, typing: undefined }), { _tag: "SearchStore", providerId, query: typing.text.trim() });
      }
      case "OpenedStoreEntry": {
        const search = korri.store?.search;
        if (search?._tag !== "Ready" || !search.entries.some((entry) => entry.id === message.entryId))
          return stay(nav);
        return ask(withStore(nav, { ...nav.storePlace, view: "Entry", typing: undefined }), { _tag: "OpenStoreEntry", entryId: message.entryId });
      }
      case "PressedStoreAction":
        return storeActionPublished(korri, message.actionId) ? ask(nav, { _tag: "RunAction", actionId: message.actionId }) : stay(nav);
      case "PressedStorePlay": {
        const details = korri.store?.details;
        if (details?._tag !== "Ready" || details.get._tag !== "InLibrary" || details.get.gameId !== message.gameId)
          return stay(nav);
        const view = picoScreenViewFromModel(korri);
        const game = view._tag === "Shelf" ? view.games.find((candidate) => candidate.id === message.gameId) : undefined;
        if (game === undefined)
          return stay(nav);
        if (game.locations !== undefined && game.locations.length > 0) {
          return stay({ ...nav, detail: { gameId: game.id, question: { _tag: "ChoosingLocation", game } } });
        }
        return ask({ ...nav, session: picoSessionReturnOnPlay(nav.session, game.id, korri.catalog) }, { _tag: "LaunchGame", gameId: game.id });
      }
      case "PressedPlay": {
        const detail = nav.detail;
        if (detail === undefined)
          return stay(nav);
        const view = picoScreenViewFromModel(korri);
        const game = view._tag === "Shelf" ? view.games.find((candidate) => candidate.id === detail.gameId) : undefined;
        if (game === undefined)
          return stay(nav);
        if (game.locations !== undefined && game.locations.length > 0) {
          return stay({ ...nav, detail: { ...detail, question: { _tag: "ChoosingLocation", game } } });
        }
        return ask({ ...nav, session: picoSessionReturnOnPlay(nav.session, game.id, korri.catalog) }, { _tag: "LaunchGame", gameId: game.id });
      }
      case "ChoseLocation": {
        const detail = nav.detail;
        if (detail?.question._tag !== "ChoosingLocation")
          return stay(nav);
        const gameId = detail.question.game.id;
        return ask({
          ...nav,
          detail: { ...detail, question: { _tag: "None" } },
          session: picoSessionReturnOnPlay(nav.session, gameId, korri.catalog)
        }, { _tag: "LaunchGame", gameId, locationId: message.locationId });
      }
      case "PressedGameAction": {
        const detail = nav.detail;
        if (detail === undefined)
          return stay(nav);
        return message.action.destructive === true ? stay({ ...nav, detail: { ...detail, question: { _tag: "ConfirmingAction", action: message.action } } }) : ask(nav, { _tag: "RunGameAction", gameId: detail.gameId, actionId: message.action.id });
      }
      case "ConfirmedGameAction": {
        const detail = nav.detail;
        if (detail?.question._tag !== "ConfirmingAction")
          return stay(nav);
        return ask({ ...nav, detail: { ...detail, question: { _tag: "None" } } }, { _tag: "RunGameAction", gameId: detail.gameId, actionId: detail.question.action.id });
      }
      case "CancelledGameAction":
        return stay(nav.detail === undefined ? nav : { ...nav, detail: { ...nav.detail, question: { _tag: "None" } } });
      case "ChoseSettingsGroup":
        if (nav.launch !== undefined)
          return stay({ ...nav, launch: { ...nav.launch, group: message.group } });
        return stay(nav.settings === undefined ? nav : { ...nav, settings: { ...nav.settings, group: message.group } });
      case "PressedSettingAction": {
        if (message.actionId.startsWith("identity:")) {
          return stay(withQuestion(nav, { _tag: "Identity", actionId: message.actionId, form: IDENTITY_FORM, qr: undefined }));
        }
        if (message.actionId === PICO_WIFI_SCAN_ACTION) {
          const wifi = korri.network?.wifi;
          return wifi?._tag === "Available" && !wifi.scanning ? ask(nav, { _tag: "ScanWifi" }) : stay(nav);
        }
        if (message.actionId === PICO_WIFI_HIDDEN_ACTION)
          return stay(withQuestion(nav, { _tag: "Wifi", entry: HIDDEN_NAME_ENTRY }));
        const networkId = picoWifiNetworkId(message.actionId);
        if (networkId !== undefined)
          return joinFromSettings(nav, pickNetwork(korri, networkId));
        return ask(nav, { _tag: "RunAction", actionId: message.actionId });
      }
      case "AskedSettingConfirmation":
        return stay(withQuestion(nav, { _tag: "ConfirmingAction", actionId: message.actionId, confirmation: message.confirmation }));
      case "ConfirmedSettingAction": {
        const question = activeSettings(nav)?.question;
        if (question?._tag !== "ConfirmingAction")
          return stay(nav);
        return ask(withQuestion(nav, { _tag: "None" }), { _tag: "RunAction", actionId: question.actionId });
      }
      case "ChoseLaunchScope":
        return stay(nav.launch === undefined ? nav : { ...nav, launch: { ...nav.launch, scope: message.scope } });
      case "OpenedPicker":
        return pickRow(korri, message.settingId) === undefined ? stay(nav) : stay(withQuestion(nav, { _tag: "PickingChoice", settingId: message.settingId }));
      case "PickedChoice": {
        const question = activeSettings(nav)?.question;
        if (question?._tag !== "PickingChoice")
          return stay(nav);
        const row = pickRow(korri, question.settingId);
        if (row === undefined || !row.control.options.some((option) => option.value === message.value)) {
          return stay(withQuestion(nav, { _tag: "None" }));
        }
        return ask(withQuestion(nav, { _tag: "None" }), { _tag: "ChangeSetting", settingId: question.settingId, value: message.value });
      }
      case "CancelledPicker":
      case "CancelledSettingAction":
      case "ClosedEditor":
      case "ClosedIdentity":
        return stay(withQuestion(nav, { _tag: "None" }));
      case "ChangedSetting":
        if (message.settingId === PICO_FONT_SETTING) {
          return isPicoFont(message.value) ? ask(nav, { _tag: "ChooseFont", font: message.value }) : stay(nav);
        }
        return ask(nav, { _tag: "ChangeSetting", settingId: message.settingId, value: message.value });
      case "DismissedSettingsProblem":
        return ask(nav, { _tag: "DismissSettingsProblem" });
      case "OpenedEditor": {
        const row = textRow(korri, message.settingId);
        if (row === undefined)
          return stay(nav);
        const draft = picoTextDraft(row.control.sensitive ? "" : row.control.initial ?? row.value ?? "");
        return stay(withQuestion(nav, { _tag: "EditingText", settingId: row.id, draft, sawSaving: false }));
      }
      case "AskedClear": {
        const editor = openEditor(nav);
        return editor?._tag === "EditingText" && !saving(korri, editor) ? stay(withQuestion(nav, { ...editor, _tag: "ConfirmingClear" })) : stay(nav);
      }
      case "CancelledClear": {
        const editor = openEditor(nav);
        return editor?._tag === "ConfirmingClear" ? stay(withQuestion(nav, { ...editor, _tag: "EditingText" })) : stay(nav);
      }
      case "ConfirmedClear": {
        const editor = openEditor(nav);
        if (editor?._tag !== "ConfirmingClear")
          return stay(nav);
        return ask(withQuestion(nav, { ...editor, _tag: "EditingText" }), { _tag: "ChangeSetting", settingId: editor.settingId, value: "" });
      }
      case "TypedText":
        return stay(editDraft(nav, korri, (draft, maxLength) => picoDraftType(draft, message.character, maxLength)));
      case "PressedTextBackspace":
        return stay(editDraft(nav, korri, (draft) => picoDraftBackspace(draft)));
      case "ClearedText":
        return stay(editDraft(nav, korri, (draft) => ({ ...draft, text: "" })));
      case "ToggledCapitals":
        return stay(editDraft(nav, korri, (draft) => ({ ...draft, capitals: !draft.capitals })));
      case "ToggledSymbols":
        return stay(editDraft(nav, korri, (draft) => ({ ...draft, symbols: !draft.symbols })));
      case "ToggledShown": {
        const entry = wifiEntry(nav);
        return entry === undefined ? stay(nav) : stay(withQuestion(nav, { _tag: "Wifi", entry: toggleShown(entry) }));
      }
      case "SubmittedWifi": {
        const entry = wifiEntry(nav);
        return entry === undefined ? stay(nav) : joinFromSettings(nav, submitEntry(entry));
      }
      case "SavedText": {
        const editor = openEditor(nav);
        if (editor?._tag !== "EditingText" || saving(korri, editor))
          return stay(nav);
        if (editor.draft.text.trim() === "" && textRow(korri, editor.settingId)?.control.allowEmpty !== true)
          return stay(nav);
        const numeric = textRow(korri, editor.settingId)?.control.numeric;
        if (numeric !== undefined && !picoNumberFits(editor.draft.text, numeric))
          return stay(nav);
        return ask(nav, { _tag: "ChangeSetting", settingId: editor.settingId, value: editor.draft.text });
      }
      case "EditedIdentity":
        return stay(editIdentity(nav, (form) => ({ ...form, [message.field]: message.value })));
      case "ChoseIdentityDisposition":
        return stay(editIdentity(nav, (form) => ({ ...form, disposition: message.disposition })));
      case "CheckedIdentityConfirmation":
        return stay(editIdentity(nav, (form) => ({ ...form, confirmed: message.confirmed })));
      case "SubmittedIdentity": {
        const question = nav.settings?.question;
        if (question?._tag !== "Identity" || korri.identityManagement?.status._tag === "Working")
          return stay(nav);
        const request = identityRequest(question.actionId, question.form);
        return request === undefined ? stay(nav) : ask(nav, request);
      }
      case "RenderedQr": {
        const question = nav.settings?.question;
        return question?._tag === "Identity" && question.qr?.text === message.text ? stay(withQuestion(nav, { ...question, qr: { text: message.text, dataUrl: message.dataUrl } })) : stay(nav);
      }
      case "Waited":
        return stay(nav.invites.showing.some((notice) => notice.token === message.token) ? { ...nav, invites: { ...nav.invites, showing: nav.invites.showing.filter((notice) => notice.token !== message.token) } } : nav);
    }
  }
  function endNotices(nav, message) {
    const pressed = isHostButton(message) || message._tag === "PressedScreen";
    return pressed && nav.invites.showing.length > 0 ? { ...nav, invites: { ...nav.invites, showing: [] } } : nav;
  }
  function settleInvites(nav, korri) {
    const standing = korri.invites ?? [];
    const showing = nav.invites.showing.filter((notice) => standing.some((invite) => invite.id === notice.inviteId));
    const fresh = invitesNoticeable(korri) ? standing.filter((invite) => !nav.invites.noticed.includes(invite.id)) : [];
    if (fresh.length === 0 && showing.length === nav.invites.showing.length)
      return stay(nav);
    const added = fresh.map((invite, index) => ({ inviteId: invite.id, token: nav.invites.nextToken + index }));
    return {
      model: {
        ...nav,
        invites: {
          noticed: [...nav.invites.noticed, ...fresh.map((invite) => invite.id)],
          showing: [...[...added].reverse(), ...showing],
          nextToken: nav.invites.nextToken + added.length
        }
      },
      requests: added.map((notice) => ({ _tag: "Wait", ms: PICO_INVITE_NOTICE_MS, token: notice.token }))
    };
  }
  function back(nav) {
    switch (topLayer(nav)) {
      case "Settings":
        return nav.settings === undefined ? nav : backInSettings(nav, nav.settings);
      case "Detail":
        if (nav.detail === undefined)
          return nav;
        return nav.detail.question._tag !== "None" ? { ...nav, detail: { ...nav.detail, question: { _tag: "None" } } } : { ...nav, detail: undefined };
      case "Store":
        return backInStore(nav, nav.storePlace);
      case "Find":
        return { ...nav, find: false };
      case "Home":
        return nav.home.menu === "Open" ? { ...nav, home: closeMenu(nav.home) } : nav;
    }
  }
  function backInStore(nav, place) {
    if (place.typing !== undefined)
      return withStore(nav, { ...place, typing: undefined });
    switch (place.view) {
      case "Entry":
        return withStore(nav, { ...place, view: "Results" });
      case "Results":
        return withStore(nav, { ...place, view: "Providers" });
      case "Providers":
        return { ...nav, store: false };
    }
  }
  function backInSettings(nav, settings) {
    const question = settings.question;
    switch (question._tag) {
      case "ConfirmingClear":
        return withQuestion(nav, { ...question, _tag: "EditingText" });
      case "ConfirmingAction":
      case "EditingText":
      case "PickingChoice":
      case "Identity":
      case "Wifi":
        return withQuestion(nav, { _tag: "None" });
      case "None":
        return { ...nav, settings: undefined };
    }
  }
  function settleSession(nav, korri) {
    const session = picoSessionReturnFromModel(nav.session, nav.detail?.gameId, korri);
    if (session === nav.session || session._tag === "Idle" && nav.session._tag === "Idle")
      return nav;
    if (session._tag !== "ReturnToLibrary")
      return { ...nav, session };
    return {
      ...nav,
      home: { ...nav.home, mode: "shelf", menu: "Closed", focusOnReturn: "None" },
      find: false,
      store: false,
      detail: undefined,
      settings: undefined,
      session: { _tag: "Idle" }
    };
  }
  function settleEditor(nav, korri) {
    const editor = openEditor(nav);
    if (editor === undefined)
      return nav;
    const state = textRow(korri, editor.settingId)?.state;
    if (state === "saving")
      return editor.sawSaving ? nav : withQuestion(nav, { ...editor, sawSaving: true });
    if (!editor.sawSaving)
      return nav;
    return withQuestion(nav, state === "idle" ? { _tag: "None" } : { ...editor, sawSaving: false });
  }
  function settleIdentity(before, nav, korri) {
    const requests = [];
    if (identityAction(before) !== identityAction(nav))
      requests.push({ _tag: "DismissIdentityStatus" });
    const question = nav.settings?.question;
    if (question?._tag !== "Identity")
      return { model: nav, requests };
    const status = korri.identityManagement?.status;
    const text = status?._tag === "BackupReady" ? status.encryptedSecret : undefined;
    if (text === question.qr?.text)
      return { model: nav, requests };
    if (text === undefined)
      return { model: withQuestion(nav, { ...question, qr: undefined }), requests };
    requests.push({ _tag: "RenderQr", text });
    return { model: withQuestion(nav, { ...question, qr: { text, dataUrl: undefined } }), requests };
  }
  function identityRequest(actionId, form) {
    const exportKey = picoIdentityRetiredKey(actionId, "export");
    const deleteKey = picoIdentityRetiredKey(actionId, "delete");
    if (actionId === PICO_IDENTITY_BACKUP_ACTION || exportKey !== undefined) {
      return form.password.trim() === "" ? undefined : { _tag: "ExportIdentityBackup", password: form.password, retiredPublicKey: exportKey };
    }
    if (actionId === PICO_IDENTITY_SWITCH_LOCAL_ACTION) {
      return !form.confirmed || form.secret.trim() === "" || form.password.trim() === "" ? undefined : {
        _tag: "SwitchIdentityFromBackup",
        encryptedSecret: form.secret,
        password: form.password,
        disposition: form.disposition,
        trustLossConfirmed: form.confirmed
      };
    }
    if (actionId === PICO_IDENTITY_SWITCH_NIP46_ACTION) {
      return !form.confirmed || form.bunkerUri.trim() === "" ? undefined : {
        _tag: "SwitchIdentityToNip46",
        bunkerUri: form.bunkerUri,
        disposition: form.disposition,
        trustLossConfirmed: form.confirmed
      };
    }
    if (deleteKey !== undefined) {
      return form.confirmed ? { _tag: "DeleteRetiredIdentity", publicKey: deleteKey, backupConfirmed: form.confirmed } : undefined;
    }
    return;
  }
  var pickRow = (korri, settingId) => {
    const groups = [...picoLaunchViewFromModel(korri)?.settings.groups ?? [], ...picoSettingsViewFromModel(korri).groups];
    const row = groups.flatMap((group) => group.rows).find((candidate) => candidate.id === settingId);
    return row?.control.kind === "pick" ? row : undefined;
  };
  var textRow = (korri, settingId) => {
    const launch = picoLaunchViewFromModel(korri)?.settings;
    return (launch === undefined ? undefined : picoTextSettingFrom(launch, settingId)?.row) ?? picoTextSettingFrom(picoSettingsViewFromModel(korri), settingId)?.row;
  };
  function settleLaunch(nav, korri) {
    const open = launchOpen(korri);
    if (open && nav.launch === undefined)
      return { ...nav, launch: SETTINGS };
    if (!open && nav.launch !== undefined)
      return { ...nav, launch: undefined };
    return nav;
  }
  var activeSettings = (nav) => nav.launch ?? nav.settings;
  var openEditor = (nav) => {
    const question = activeSettings(nav)?.question;
    return question?._tag === "EditingText" || question?._tag === "ConfirmingClear" ? question : undefined;
  };
  var saving = (korri, editor) => textRow(korri, editor.settingId)?.state === "saving";
  function editDraft(nav, korri, change) {
    const entry = wifiEntry(nav);
    if (entry !== undefined) {
      return withQuestion(nav, { _tag: "Wifi", entry: editEntry(entry, (draft) => change(draft, entryMaxLength(entry))) });
    }
    const typing = nav.storePlace.typing;
    if (topLayer(nav) === "Store" && typing !== undefined) {
      return withStore(nav, { ...nav.storePlace, typing: change(typing, PICO_STORE_QUERY_MAX) });
    }
    const editor = openEditor(nav);
    if (editor?._tag !== "EditingText")
      return nav;
    const row = textRow(korri, editor.settingId);
    if (row === undefined || row.state === "saving")
      return nav;
    return withQuestion(nav, { ...editor, draft: change(editor.draft, row.control.maxLength) });
  }
  var wifiEntry = (nav) => {
    const question = nav.settings?.question;
    return question?._tag === "Wifi" ? question.entry : undefined;
  };
  function joinFromSettings(nav, next) {
    switch (next._tag) {
      case "Edit":
        return stay(withQuestion(nav, { _tag: "Wifi", entry: next.entry }));
      case "Send":
        return ask(withQuestion(nav, { _tag: "None" }), next.request);
      case "Nothing":
        return stay(nav);
    }
  }
  function editIdentity(nav, change) {
    const question = nav.settings?.question;
    return question?._tag === "Identity" ? withQuestion(nav, { ...question, form: change(question.form) }) : nav;
  }
  function settleIdle(nav, korri) {
    return nav.idle._tag === "Attracting" && !canAttract(nav, korri) ? { ...nav, idle: { _tag: "Awake", activity: nav.idle.activity } } : nav;
  }
  var withStore = (nav, place) => ({ ...nav, storePlace: place });
  function storeActionPublished(korri, actionId) {
    const store = korri.store;
    if (store === undefined)
      return false;
    const lists = [
      store.providers._tag === "Problem" ? store.providers.actions : [],
      store.search._tag === "Problem" ? store.search.actions : [],
      store.details._tag === "Problem" ? store.details.actions : [],
      store.details._tag === "Ready" && "actions" in store.details.get ? store.details.get.actions : []
    ];
    return lists.some((actions) => actions.some((action) => action.id === actionId && action.enabled));
  }
  var closeMenu = (home) => home.menu === "Closed" ? home : { ...home, menu: "Closed" };
  var withQuestion = (nav, question) => nav.launch !== undefined ? { ...nav, launch: { ...nav.launch, question } } : nav.settings === undefined ? nav : { ...nav, settings: { ...nav.settings, question } };
  var cycleMode = (nav) => ({
    ...nav,
    home: { ...nav.home, mode: nav.home.mode === "shelf" ? "grid" : nav.home.mode === "grid" ? "hero" : "shelf" }
  });

  // logic.ts
  var SHELF_HINTS = [
    { hintKey: "a", label: "PLAY" },
    { hintKey: "b", label: "BACK" }
  ];
  function shelfReadout(games) {
    const resumable = games.filter((game) => game.resumable === true).length;
    const carts = `${games.length} ${games.length === 1 ? "CART" : "CARTS"}`;
    return resumable === 0 ? carts : `${carts} · ${resumable} RESUMABLE`;
  }
  function shelfGames(nav, model) {
    const shown = shownScreen(nav, model);
    return shown._tag === "Home" && shown.view._tag === "Shelf" ? shown.view.games : [];
  }
  function chosenIndex(nav, games) {
    const found = games.findIndex((game) => game.id === nav.home.selectedGameId);
    return found === -1 ? 0 : found;
  }
  function step2(nav, message, model) {
    return update(nav, message, model).model;
  }
  function update2(state, message, model) {
    if (message._tag !== "Pressed")
      return state;
    const nav = state.nav;
    switch (message.action) {
      case "left":
      case "right": {
        const games = shelfGames(nav, model);
        if (games.length === 0)
          return state;
        const from = chosenIndex(nav, games);
        const to = Math.max(0, Math.min(games.length - 1, from + (message.action === "left" ? -1 : 1)));
        const moved = step2(nav, { _tag: "MovedFocus" }, model);
        const target = games[to];
        if (to === from || target === undefined)
          return { nav: moved, cursorOnShelf: true };
        return { nav: step2(moved, { _tag: "SelectedCart", gameId: target.id }, model), cursorOnShelf: true };
      }
      case "back":
        return { ...state, nav: step2(nav, { _tag: "PressedBack" }, model) };
      default:
        return state;
    }
  }
  function cartView(game, hero) {
    const look = picoCartLookFor(game.id, game.subtitle);
    const progress = game.resumable === true ? "resume" : picoStatsFor(game).length === 0 ? "new" : "played";
    const art = game.artUrl === undefined || game.artUrl === "" ? undefined : game.artUrl;
    return {
      id: game.id,
      title: game.title,
      ...art === undefined ? {} : { art: { $image: art } },
      hasArt: art !== undefined,
      initials: picoInitials(game.title),
      system: look.system,
      shell: look.shell,
      sticker: look.sticker,
      stickerInkLight: look.ink === "light",
      progress,
      hero
    };
  }
  var BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  function pngRatio(url) {
    const comma = url.indexOf(",");
    if (!url.startsWith("data:image/png;base64,") || comma === -1)
      return;
    const bytes = [];
    let bits = 0;
    let value = 0;
    for (const char of url.slice(comma + 1, comma + 1 + 32)) {
      const digit = BASE64.indexOf(char);
      if (digit === -1)
        break;
      value = value << 6 | digit;
      bits += 6;
      if (bits >= 8) {
        bits -= 8;
        bytes.push(value >> bits & 255);
      }
    }
    if (bytes.length < 24)
      return;
    const word = (at) => ((bytes[at] ?? 0) << 24 | (bytes[at + 1] ?? 0) << 16 | (bytes[at + 2] ?? 0) << 8 | (bytes[at + 3] ?? 0)) >>> 0;
    const width = word(16);
    const height = word(20);
    return width > 0 && height > 0 ? width / height : undefined;
  }
  function narrowestArt(games) {
    return games.reduce((narrowest, game) => {
      const ratio = game.artUrl === undefined || game.artUrl === "" ? 1 : pngRatio(game.artUrl) ?? 0;
      return Math.min(narrowest, ratio);
    }, games.length === 0 ? 1 : Number.POSITIVE_INFINITY);
  }
  function view(state, model) {
    const games = shelfGames(state.nav, model);
    const index = chosenIndex(state.nav, games);
    const chosen = games[index];
    const stats = chosen === undefined ? [] : picoStatsFor(chosen);
    return {
      place: "korri",
      label: "LIBRARY",
      clockLabel: model.clockLabel ?? "",
      showClock: model.clockLabel !== undefined,
      readout: shelfReadout(games),
      hints: SHELF_HINTS.map((hint) => ({ key: hint.hintKey, label: hint.label })),
      menuLabel: "MENU",
      hasShelf: chosen !== undefined,
      selectedIndex: index,
      total: games.length,
      cursorOnShelf: state.cursorOnShelf,
      narrowestArt: narrowestArt(games),
      carts: games.map((game, at) => cartView(game, at === index)),
      stage: chosen === undefined ? { title: "", subtitle: "", kicker: "", resumable: false, download: "", stats: [], cart: null } : {
        title: chosen.title,
        subtitle: chosen.subtitle ?? "",
        kicker: chosen.section?.toUpperCase() ?? "",
        resumable: chosen.resumable === true,
        download: picoDownloadLabel(chosen) ?? "",
        stats,
        cart: cartView(chosen, true)
      }
    };
  }
  globalThis.surface = {
    init: (_model) => ({ nav: initialNavigation(), cursorOnShelf: false }),
    update: update2,
    view
  };
})();
