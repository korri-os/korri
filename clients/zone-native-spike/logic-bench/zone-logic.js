(() => {
  // ../../../surfaces/zone/src/plan/look.ts
  var ZONE_LOOKS = ["modern", "blend", "retro"];
  var ZONE_LOOK_WORDS = {
    modern: "MODERN",
    blend: "BLEND",
    retro: "RETRO"
  };
  function zoneLookStep(look, direction) {
    const at = ZONE_LOOKS.indexOf(look);
    return ZONE_LOOKS[(at + direction + ZONE_LOOKS.length) % ZONE_LOOKS.length];
  }

  // ../../../surfaces/zone/src/plan/options.ts
  var ZONE_OPTIONS_SYSTEM = "zone:system";
  var ZONE_OPTIONS_WIFI = "zone:wifi";
  var ZONE_OPTIONS_DISMISS = "zone:dismiss";
  var ZONE_WIFI_CONNECTION = "zone:wifi:connection";
  var ZONE_SYSTEM_BUILD = "zone:system-build";
  var ZONE_SYSTEM_LOOK = "zone:system-look";
  var ZONE_WIFI_ADAPTER = "zone:wifi:adapter";
  var ZONE_WIFI_SCAN = "zone:wifi:scan";
  var ZONE_WIFI_HIDDEN = "zone:wifi:hidden";
  var zoneSystemRowId = (actionId) => `zone:system:${actionId}`;
  var zoneNetworkRowId = (networkId) => `zone:wifi:network:${networkId}`;
  function zoneConnectionValue(connection) {
    switch (connection._tag) {
      case "Offline":
        return "OFFLINE";
      case "Wired":
        return "WIRED";
      case "Wifi":
        return connection.strengthPercent === undefined ? connection.ssid : `${connection.ssid} ${connection.strengthPercent}%`;
    }
  }
  function zoneWifiWords(network) {
    return [network.secured ? "LOCKED" : "OPEN", ...network.known ? ["SAVED"] : []];
  }
  function zoneNetworkValue(network) {
    return [`${network.strengthPercent}%`, ...zoneWifiWords(network)].join(" ");
  }
  var isPathLabel = (label) => /^[/~]/.test(label);
  function zoneNumberRange(range) {
    if (range.min !== undefined && range.max !== undefined)
      return `${range.min} to ${range.max}`;
    if (range.min !== undefined)
      return `At least ${range.min}`;
    if (range.max !== undefined)
      return `At most ${range.max}`;
    return;
  }
  function zoneNumberFits(draft, range) {
    const text = draft.trim();
    if (!/^-?\d*\.?\d+$/.test(text))
      return false;
    const value = Number(text);
    if (range.step === 1 && !Number.isInteger(value))
      return false;
    return (range.min === undefined || value >= range.min) && (range.max === undefined || value <= range.max);
  }
  function zoneNumberStep(interaction, direction) {
    const step = interaction.step ?? 1;
    const from = interaction.value ?? (direction > 0 ? interaction.min ?? 0 : interaction.max ?? 0);
    const next = interaction.value === undefined ? from : from + direction * step;
    const held = Math.min(interaction.max ?? Infinity, Math.max(interaction.min ?? -Infinity, next));
    return held === interaction.value ? undefined : Number(held.toFixed(6));
  }
  var isPick = (item) => item.interaction?.kind === "choice" && item.interaction.choices.length > 0 && choiceIndex(item) < 0;
  function choiceIndex(item) {
    if (item.interaction?.kind !== "choice")
      return -1;
    const selected = item.interaction.selected;
    return item.interaction.choices.findIndex((choice) => choice.value === selected);
  }
  function settingPress(item) {
    const interaction = item.interaction;
    if (interaction === undefined)
      return { _tag: "Nothing" };
    switch (interaction.kind) {
      case "choice":
        return interaction.choices.length === 0 ? { _tag: "Nothing" } : { _tag: "Step", settingId: item.id };
      case "number": {
        const range = zoneNumberRange(interaction);
        return {
          _tag: "Edit",
          settingId: item.id,
          label: range === undefined ? item.label : `${item.label} (${range})`,
          draft: interaction.value === undefined ? "" : String(interaction.value),
          secret: false,
          maxLength: 24,
          placeholder: range ?? "Number",
          ...interaction.clearable === true && interaction.value !== undefined ? { clearLabel: "Inherit" } : {},
          numeric: {
            ...interaction.min === undefined ? {} : { min: interaction.min },
            ...interaction.max === undefined ? {} : { max: interaction.max },
            ...interaction.step === undefined ? {} : { step: interaction.step },
            clearable: interaction.clearable === true
          }
        };
      }
      case "text":
        return {
          _tag: "Edit",
          settingId: item.id,
          label: item.label,
          draft: item.value ?? "",
          secret: false,
          ...interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength },
          ...interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder }
        };
      case "sensitiveText":
        return {
          _tag: "Edit",
          settingId: item.id,
          label: item.label,
          draft: "",
          secret: true,
          ...interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength },
          ...interaction.clearLabel === undefined ? {} : { clearLabel: interaction.clearLabel },
          ...interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder }
        };
      case "action": {
        const confirm = interaction.confirmation !== undefined ? {
          headline: interaction.confirmation.title,
          korriHeadline: true,
          lines: [interaction.confirmation.message],
          confirmLabel: interaction.confirmation.confirmLabel
        } : interaction.destructive === true ? { headline: item.label, lines: [], confirmLabel: "Yes" } : undefined;
        return confirm === undefined ? { _tag: "Run", actionId: interaction.actionId } : { _tag: "Run", actionId: interaction.actionId, confirm };
      }
    }
  }
  var DISMISS_ITEM = {
    id: ZONE_OPTIONS_DISMISS,
    label: "Dismiss",
    value: "OK",
    interaction: { kind: "action", actionId: ZONE_OPTIONS_DISMISS }
  };
  function settingsGroups(slices) {
    const status = slices.settingsStatus;
    return slices.settings.map((group, index) => {
      const key = `setting:${index}`;
      const rows = group.items.flatMap((item) => {
        const row = {
          id: item.id,
          group: key,
          item,
          press: settingPress(item),
          keepCase: { value: true, ...isPathLabel(item.label) ? { label: true } : {} }
        };
        return status._tag === "Problem" && status.settingId === item.id ? [row, { id: ZONE_OPTIONS_DISMISS, group: key, item: DISMISS_ITEM, press: { _tag: "DismissProblem" } }] : [row];
      });
      return { key, title: group.title, rows, status, verbatim: false };
    });
  }
  function systemRow(action) {
    const id = zoneSystemRowId(action.id);
    const description = action.description === undefined ? {} : { description: action.description };
    if (!action.enabled) {
      return {
        id,
        group: ZONE_OPTIONS_SYSTEM,
        item: { id, label: action.label, value: "Go", ...description, interaction: { kind: "action", actionId: action.id } },
        press: { _tag: "Nothing" },
        disabled: true
      };
    }
    const destructive = action.destructive === true;
    return {
      id,
      group: ZONE_OPTIONS_SYSTEM,
      item: {
        id,
        label: action.label,
        value: "Go",
        ...description,
        interaction: { kind: "action", actionId: action.id, ...destructive ? { destructive } : {} }
      },
      press: destructive ? {
        _tag: "Run",
        actionId: action.id,
        confirm: {
          headline: action.label,
          lines: action.description === undefined ? [] : [action.description],
          confirmLabel: "Yes"
        }
      } : { _tag: "Run", actionId: action.id }
    };
  }
  function buildRow(build) {
    return {
      id: ZONE_SYSTEM_BUILD,
      group: ZONE_OPTIONS_SYSTEM,
      item: { id: ZONE_SYSTEM_BUILD, label: "Build", value: build },
      press: { _tag: "Nothing" },
      keepCase: { value: true }
    };
  }
  function lookRow(look) {
    return {
      id: ZONE_SYSTEM_LOOK,
      group: ZONE_OPTIONS_SYSTEM,
      item: {
        id: ZONE_SYSTEM_LOOK,
        label: "Look",
        value: ZONE_LOOK_WORDS[look],
        interaction: { kind: "choice", choices: ZONE_LOOKS.map((id) => ({ value: id, label: ZONE_LOOK_WORDS[id] })), selected: look }
      },
      press: { _tag: "Look" }
    };
  }
  function systemGroup(slices) {
    const build = slices.buildLabel === undefined || slices.buildLabel === "" ? [] : [buildRow(slices.buildLabel)];
    const look = slices.look === undefined ? [] : [lookRow(slices.look)];
    if (slices.actions.length === 0 && build.length === 0 && look.length === 0)
      return [];
    return [{
      key: ZONE_OPTIONS_SYSTEM,
      title: "System",
      rows: [...slices.actions.map(systemRow), ...look, ...build],
      status: { _tag: "Idle" },
      verbatim: false
    }];
  }
  var wifiRow = (id, item, press) => ({ id, group: ZONE_OPTIONS_WIFI, item: { id, ...item }, press });
  function wifiGroup(slices) {
    const network = slices.network;
    if (network === undefined)
      return [];
    const ssid = network.connection._tag === "Wifi" ? { description: network.connection.ssid } : {};
    const connection = {
      ...wifiRow(ZONE_WIFI_CONNECTION, { label: "CONNECTION", value: zoneConnectionValue(network.connection), ...ssid }, { _tag: "Nothing" }),
      signal: { _tag: "Connection", connection: network.connection }
    };
    const wifi = network.wifi;
    if (wifi._tag === "NoAdapter") {
      return [{
        key: ZONE_OPTIONS_WIFI,
        title: "WI-FI",
        rows: [connection, wifiRow(ZONE_WIFI_ADAPTER, { label: "NO WI-FI ADAPTER" }, { _tag: "Nothing" })],
        status: { _tag: "Idle" },
        verbatim: true
      }];
    }
    const networks = [...wifi.networks].sort((a, b) => b.strengthPercent - a.strengthPercent);
    const attempt = wifi.attempt;
    const networkRows = networks.map((item) => ({
      keepCase: { label: true },
      ...attempt._tag === "Connecting" && attempt.ssid === item.ssid ? { ownDescription: true } : {},
      ...wifiRow(zoneNetworkRowId(item.id), {
        label: item.ssid,
        value: zoneNetworkValue(item),
        ...attempt._tag === "Connecting" && attempt.ssid === item.ssid ? { description: "CONNECTING" } : {},
        interaction: { kind: "action", actionId: zoneNetworkRowId(item.id) }
      }, { _tag: "Join", networkId: item.id }),
      signal: { _tag: "Network", strengthPercent: item.strengthPercent, words: zoneWifiWords(item) }
    }));
    const rows = [
      connection,
      wifiRow(ZONE_WIFI_SCAN, {
        label: "SCAN",
        value: wifi.scanning ? "SCANNING…" : `${networks.length} FOUND`,
        interaction: { kind: "action", actionId: ZONE_WIFI_SCAN }
      }, { _tag: "Scan" }),
      ...networkRows,
      wifiRow(ZONE_WIFI_HIDDEN, { label: "HIDDEN NETWORK", value: "JOIN", interaction: { kind: "action", actionId: ZONE_WIFI_HIDDEN } }, { _tag: "JoinHidden" })
    ];
    const rowFor = (ssid2) => {
      const listed = networks.find((item) => item.ssid === ssid2);
      return listed === undefined ? ZONE_WIFI_HIDDEN : zoneNetworkRowId(listed.id);
    };
    const status = attempt._tag === "Connecting" ? { _tag: "Saving", settingId: rowFor(attempt.ssid) } : attempt._tag === "Failed" ? { _tag: "Problem", settingId: rowFor(attempt.ssid), message: attempt.message } : { _tag: "Idle" };
    return [{ key: ZONE_OPTIONS_WIFI, title: "WI-FI", rows, status, verbatim: true }];
  }
  function optionsPlan(slices) {
    const groups = [...settingsGroups(slices), ...systemGroup(slices), ...wifiGroup(slices)];
    return { groups, rows: groups.flatMap((group) => group.rows) };
  }
  function focusedRow(plan, focused) {
    if (focused === undefined)
      return;
    if (focused === ZONE_OPTIONS_SYSTEM) {
      return plan.rows.find((row) => row.group === ZONE_OPTIONS_SYSTEM) ?? plan.rows[0];
    }
    return plan.rows.find((row) => row.id === focused);
  }
  var WIFI_SSID_MAX = 32;
  var WIFI_PASSWORD_MAX = 63;
  function entryHeading(entry) {
    switch (entry._tag) {
      case "Setting":
        return {
          title: entry.label,
          secret: entry.secret,
          ...entry.numeric === undefined ? {} : { numeric: entry.numeric },
          ...entry.maxLength === undefined ? {} : { maxLength: entry.maxLength },
          ...entry.clearLabel === undefined ? {} : { clearLabel: entry.clearLabel },
          ...entry.placeholder === undefined ? {} : { placeholder: entry.placeholder }
        };
      case "WifiPassword":
        return { title: "Password", name: entry.ssid, secret: true, maxLength: WIFI_PASSWORD_MAX };
      case "HiddenSsid":
        return { title: "Network name", secret: false, maxLength: WIFI_SSID_MAX };
      case "HiddenPassword":
        return { title: "Password", name: entry.ssid, secret: true, maxLength: WIFI_PASSWORD_MAX };
    }
  }

  // ../../../surfaces/zone/src/state/requests.ts
  var stay = (model) => ({ model, requests: [] });
  var ask = (model, ...requests) => ({ model, requests });

  // ../../../surfaces/zone/src/state/identity.ts
  var IDENTITY_START = { card: 0, flow: { _tag: "idle" } };
  var IDENTITY_BACKUP_CARD = 0;
  var IDENTITY_SWITCH_BACKUP_CARD = 1;
  var IDENTITY_SWITCH_BUNKER_CARD = 2;
  var IDENTITY_FIXED_CARDS = 3;
  var IDENTITY_BUNKER_PREFIX = "bunker://";
  function identityCardCount(identity) {
    return identity === undefined ? 1 : IDENTITY_FIXED_CARDS + identity.retiredPublicKeys.length;
  }
  function retiredKeyAt(identity, index) {
    if (identity === undefined || index < IDENTITY_FIXED_CARDS)
      return;
    return identity.retiredPublicKeys[index - IDENTITY_FIXED_CARDS];
  }
  var draftOf = (text, layer, focusedKey) => ({ text, caret: text.length, layer, focusedKey });
  var IDLE = { _tag: "idle" };
  var cleanSecret = (text) => text.replace(/\s+/g, "");
  function update(model, message, identity) {
    const result = step(model, message, identity);
    return { model: settle(result.model, identity), requests: result.requests };
  }
  function step(model, message, identity) {
    const status = identity?.status._tag ?? "Idle";
    const { flow } = model;
    switch (message._tag) {
      case "KorriPublished":
        return stay(model);
      case "FocusedSaveCard":
        if (flow._tag === "showSecret") {
          return stay(model.card === message.index && !flow.onSlab ? model : { card: message.index, flow: { _tag: "showSecret", onSlab: false } });
        }
        if (flow._tag === "retired")
          return stay({ card: message.index, flow: IDLE });
        return flow._tag === "idle" ? stay(model.card === message.index ? model : { ...model, card: message.index }) : stay(model);
      case "FocusedRetiredAction":
        if (retiredKeyAt(identity, model.card) === undefined)
          return stay(model);
        if (flow._tag === "retired")
          return stay(flow.action === message.action ? model : { ...model, flow: { _tag: "retired", action: message.action } });
        return flow._tag === "idle" ? stay({ ...model, flow: { _tag: "retired", action: message.action } }) : stay(model);
      case "ChoseRetiredAction":
        return choseRetired(model, message.action, identity);
      case "FocusedSecret":
        return flow._tag === "showSecret" && !flow.onSlab || flow._tag === "idle" && status === "BackupReady" ? stay({ ...model, flow: { _tag: "showSecret", onSlab: true } }) : stay(model);
      case "ChoseSaveCard":
        return chose(model, message.index, identity);
      case "FocusedKey":
        return editDraft(model, (draft) => draft.focusedKey === message.key ? draft : { ...draft, focusedKey: message.key });
      case "TypedChar":
        return editDraft(model, (draft) => insert(draft, message.char, message.char));
      case "TypedSpace":
        return editDraft(model, (draft) => insert(draft, " ", "space"));
      case "DeletedChar":
        return editDraft(model, (draft) => draft.caret === 0 ? { ...draft, focusedKey: "delete" } : {
          ...draft,
          text: draft.text.slice(0, draft.caret - 1) + draft.text.slice(draft.caret),
          caret: draft.caret - 1,
          focusedKey: "delete"
        });
      case "SwitchedLayer":
        return editDraft(model, (draft) => ({ ...draft, layer: message.layer, focusedKey: `layer:${message.layer}` }));
      case "PressedDone":
        return done(model, status);
      case "FocusedChoice":
        switch (flow._tag) {
          case "disposition":
          case "trust":
          case "confirmDelete":
          case "confirmBackedUp":
            return stay(flow.focus === message.index ? model : { ...model, flow: { ...flow, focus: message.index } });
          default:
            return stay(model);
        }
      case "ChoseDisposition":
        return flow._tag === "disposition" ? stay({ ...model, flow: { _tag: "trust", source: flow.source, disposition: message.disposition, focus: 0 } }) : stay(model);
      case "Answered":
        return answered(model, message.yes, status);
      case "DismissedStatus":
        return dismissed(model, status);
      case "PressedBack":
        return back(model, status);
    }
  }
  function chose(model, index, identity) {
    if (identity === undefined)
      return stay(model);
    const status = identity.status._tag;
    if (status === "Working" || status === "Problem" || status === "Switched")
      return stay(model);
    if (model.flow._tag !== "idle" && model.flow._tag !== "showSecret" && model.flow._tag !== "retired")
      return stay(model);
    let flow;
    if (index === IDENTITY_BACKUP_CARD) {
      flow = { _tag: "password", for: { kind: "export" }, draft: draftOf("", "letters", "q") };
    } else if (index === IDENTITY_SWITCH_BACKUP_CARD) {
      flow = { _tag: "secret", draft: draftOf("", "letters", "q") };
    } else if (index === IDENTITY_SWITCH_BUNKER_CARD) {
      flow = { _tag: "bunker", draft: draftOf(IDENTITY_BUNKER_PREFIX, "uri", "1") };
    } else {
      if (retiredKeyAt(identity, index) === undefined)
        return stay(model);
      flow = { _tag: "retired", action: "backup" };
    }
    const next = { card: index, flow };
    return status === "BackupReady" ? ask(next, { _tag: "DismissIdentityStatus" }) : stay(next);
  }
  function choseRetired(model, action, identity) {
    const publicKey = retiredKeyAt(identity, model.card);
    if (identity === undefined || publicKey === undefined)
      return stay(model);
    const status = identity.status._tag;
    if (status === "Working" || status === "Problem" || status === "Switched")
      return stay(model);
    if (model.flow._tag !== "idle" && model.flow._tag !== "retired" && model.flow._tag !== "showSecret")
      return stay(model);
    const flow = action === "backup" ? { _tag: "password", for: { kind: "export", retiredPublicKey: publicKey }, draft: draftOf("", "letters", "q") } : { _tag: "confirmDelete", publicKey, focus: 0 };
    const next = { ...model, flow };
    return status === "BackupReady" ? ask(next, { _tag: "DismissIdentityStatus" }) : stay(next);
  }
  function insert(draft, char, focusedKey) {
    return {
      ...draft,
      text: draft.text.slice(0, draft.caret) + char + draft.text.slice(draft.caret),
      caret: draft.caret + char.length,
      focusedKey
    };
  }
  function editDraft(model, edit) {
    const { flow } = model;
    switch (flow._tag) {
      case "password":
      case "secret":
      case "bunker": {
        const draft = edit(flow.draft);
        return stay(draft === flow.draft ? model : { ...model, flow: { ...flow, draft } });
      }
      default:
        return stay(model);
    }
  }
  function done(model, status) {
    const { flow } = model;
    switch (flow._tag) {
      case "password": {
        const password = flow.draft.text;
        if (password.trim() === "")
          return stay(model);
        if (flow.for.kind === "export") {
          if (status === "Working")
            return stay(model);
          const { retiredPublicKey } = flow.for;
          return ask({ ...model, flow: { _tag: "showSecret", onSlab: false } }, retiredPublicKey === undefined ? { _tag: "ExportIdentityBackup", password } : { _tag: "ExportIdentityBackup", password, retiredPublicKey });
        }
        return stay({
          ...model,
          flow: { _tag: "disposition", source: { kind: "backup", encryptedSecret: flow.for.encryptedSecret, password }, focus: 0 }
        });
      }
      case "secret": {
        const encryptedSecret = cleanSecret(flow.draft.text);
        if (encryptedSecret === "")
          return stay(model);
        return stay({
          ...model,
          flow: { _tag: "password", for: { kind: "switch", encryptedSecret }, draft: draftOf("", "letters", "q") }
        });
      }
      case "bunker": {
        const bunkerUri = flow.draft.text.trim();
        if (bunkerUri === "" || bunkerUri === IDENTITY_BUNKER_PREFIX)
          return stay(model);
        return stay({ ...model, flow: { _tag: "disposition", source: { kind: "bunker", bunkerUri }, focus: 0 } });
      }
      default:
        return stay(model);
    }
  }
  function answered(model, yes, status) {
    const { flow } = model;
    switch (flow._tag) {
      case "trust": {
        if (!yes)
          return stay({ ...model, flow: IDLE });
        if (status === "Working")
          return stay(model);
        const idle = { ...model, flow: IDLE };
        return flow.source.kind === "backup" ? ask(idle, {
          _tag: "SwitchIdentityFromBackup",
          encryptedSecret: flow.source.encryptedSecret,
          password: flow.source.password,
          disposition: flow.disposition,
          trustLossConfirmed: true
        }) : ask(idle, {
          _tag: "SwitchIdentityToNip46",
          bunkerUri: flow.source.bunkerUri,
          disposition: flow.disposition,
          trustLossConfirmed: true
        });
      }
      case "confirmDelete":
        return stay({ ...model, flow: yes ? { _tag: "confirmBackedUp", publicKey: flow.publicKey, focus: 0 } : IDLE });
      case "confirmBackedUp": {
        if (!yes)
          return stay({ ...model, flow: IDLE });
        if (status === "Working")
          return stay(model);
        return ask({ ...model, flow: IDLE }, { _tag: "DeleteRetiredIdentity", publicKey: flow.publicKey, backupConfirmed: true });
      }
      default:
        return stay(model);
    }
  }
  function dismissed(model, status) {
    if (status === "Idle" || status === "Working")
      return stay(model);
    const next = model.flow._tag === "showSecret" ? { ...model, flow: IDLE } : model;
    return ask(next, { _tag: "DismissIdentityStatus" });
  }
  function back(model, status) {
    if (status === "Problem" || status === "Switched" || status === "BackupReady")
      return dismissed(model, status);
    const { flow } = model;
    switch (flow._tag) {
      case "idle":
        return stay(model);
      case "showSecret":
      case "secret":
      case "bunker":
      case "retired":
        return stay({ ...model, flow: IDLE });
      case "confirmDelete":
        return stay({ ...model, flow: { _tag: "retired", action: "delete" } });
      case "password":
        return stay({
          ...model,
          flow: flow.for.kind === "switch" ? { _tag: "secret", draft: draftOf(flow.for.encryptedSecret, "letters", "done") } : flow.for.retiredPublicKey === undefined ? IDLE : { _tag: "retired", action: "backup" }
        });
      case "disposition":
        return stay({
          ...model,
          flow: flow.source.kind === "backup" ? {
            _tag: "password",
            for: { kind: "switch", encryptedSecret: flow.source.encryptedSecret },
            draft: draftOf(flow.source.password, "letters", "done")
          } : { _tag: "bunker", draft: draftOf(flow.source.bunkerUri, "uri", "done") }
        });
      case "trust":
        return stay({
          ...model,
          flow: { _tag: "disposition", source: flow.source, focus: flow.disposition === "transfer" ? 0 : 1 }
        });
      case "confirmBackedUp":
        return stay({ ...model, flow: { _tag: "confirmDelete", publicKey: flow.publicKey, focus: 0 } });
    }
  }
  function settle(model, identity) {
    if (identity === undefined) {
      return model.card === 0 && model.flow._tag === "idle" ? model : IDENTITY_START;
    }
    const last = identityCardCount(identity) - 1;
    const card = Math.max(0, Math.min(model.card, last));
    const { flow } = model;
    const gone = (flow._tag === "confirmDelete" || flow._tag === "confirmBackedUp") && !identity.retiredPublicKeys.includes(flow.publicKey) || flow._tag === "password" && flow.for.kind === "export" && flow.for.retiredPublicKey !== undefined && !identity.retiredPublicKeys.includes(flow.for.retiredPublicKey) || flow._tag === "retired" && retiredKeyAt(identity, card) === undefined;
    if (card === model.card && !gone)
      return model;
    return { card, flow: gone ? IDLE : flow };
  }

  // ../../../surfaces/zone/src/state/look.ts
  function updateLook(look, message) {
    switch (message._tag) {
      case "SteppedLook": {
        const next = zoneLookStep(look, message.direction);
        return next === look ? stay(look) : ask(next, { _tag: "RememberLook", look: next });
      }
    }
  }

  // ../../../surfaces/zone/src/state/games.ts
  var ZONE_FIND_START = { draft: "", caret: 0, layer: "letters", key: "q" };
  var ZONE_FIND_MAX = 40;
  function catalogGames(catalog) {
    return catalog._tag === "Ready" ? catalog.games : [];
  }
  function visibleGames(catalog, draft) {
    const games = catalogGames(catalog);
    const needle = draft.trim().toLocaleLowerCase();
    return needle === "" ? games : games.filter((game) => game.title.toLocaleLowerCase().includes(needle));
  }
  function launchChoices(game) {
    const verb = game.resumable === true ? "continue" : "play";
    const locations = game.launchLocations ?? [];
    if (locations.length === 0)
      return [{ id: verb, verb }];
    if (locations.length === 1)
      return [{ id: `${verb}:${locations[0].id}`, verb, locationId: locations[0].id }];
    return locations.map((location, index) => ({
      id: `${verb}:${location.id}`,
      verb,
      place: index === 0 ? { here: true } : { here: false, label: location.label },
      locationId: location.id
    }));
  }
  function pressFindKey(find, key) {
    switch (key.kind) {
      case "char":
        return insert2(find, key.char);
      case "space":
        return insert2(find, " ");
      case "delete":
        return deleteBack(find);
      case "layer":
        return { ...find, layer: key.layer };
      case "done":
        return find;
    }
  }
  function deleteBack(find) {
    if (find.caret === 0)
      return find;
    return {
      ...find,
      draft: find.draft.slice(0, find.caret - 1) + find.draft.slice(find.caret),
      caret: find.caret - 1
    };
  }
  function insert2(find, text) {
    if (find.draft.length + text.length > ZONE_FIND_MAX)
      return find;
    return {
      ...find,
      draft: find.draft.slice(0, find.caret) + text + find.draft.slice(find.caret),
      caret: find.caret + text.length
    };
  }

  // ../../../surfaces/zone/src/state/tally.ts
  var gameOf = (korri, gameId) => catalogGames(korri.catalog).find((game) => game.id === gameId);
  var zoneActOf = (game) => (game.playCount ?? 0) + 1;
  function zoneLaunchOpen(korri, gameId) {
    const game = gameOf(korri, gameId);
    if (game === undefined || game.title === "")
      return;
    return {
      gameId,
      title: game.title,
      ...game.subtitle === undefined ? {} : { system: game.subtitle },
      act: zoneActOf(game),
      seen: false,
      leaving: false
    };
  }
  var names = (status, gameId) => {
    switch (status._tag) {
      case "Busy":
      case "Confirm":
      case "Running":
        return status.gameId === undefined || status.gameId === gameId;
      case "ChooseSave":
        return status.gameId === gameId;
      case "Browsing":
      case "Problem":
      case "Paused":
      case "Ended":
        return false;
    }
  };
  function settleLaunch(launch, status) {
    if (launch === undefined || launch.leaving)
      return launch;
    if (names(status, launch.gameId))
      return launch.seen ? launch : { ...launch, seen: true };
    if (launch.seen || status._tag === "Problem")
      return { ...launch, leaving: true };
    return launch;
  }
  var dropLaunch = (launch) => launch.leaving ? launch : { ...launch, leaving: true };
  function settleSession(session, korri) {
    const status = korri.status;
    if (status._tag === "Browsing")
      return;
    if (status._tag !== "Running" || status.gameId === undefined)
      return session;
    if (session?.gameId === status.gameId)
      return session;
    const game = gameOf(korri, status.gameId);
    if (game === undefined)
      return;
    return {
      gameId: game.id,
      ...game.playCount === undefined ? {} : { playCount: game.playCount },
      ...game.totalPlaytimeSeconds === undefined ? {} : { seconds: game.totalPlaytimeSeconds }
    };
  }
  function zoneTallyOf(session, korri) {
    if (session === undefined || korri.status._tag !== "Browsing")
      return;
    const game = gameOf(korri, session.gameId);
    if (game === undefined || game.playCount === undefined)
      return;
    if (game.playCount !== (session.playCount ?? 0) + 1)
      return;
    const before = session.seconds ?? (session.playCount === undefined ? 0 : undefined);
    const now = game.totalPlaytimeSeconds;
    const rise = now === undefined || before === undefined ? undefined : now - before;
    return {
      gameId: game.id,
      title: game.title,
      ...rise === undefined || rise <= 0 ? {} : { seconds: rise },
      plays: game.playCount,
      ...now === undefined ? {} : { total: now },
      counted: false,
      leaving: false
    };
  }
  var pressTally = (tally) => tally.counted ? { ...tally, leaving: true } : { ...tally, counted: true };

  // ../../../surfaces/zone/src/state/navigation.ts
  var ZONE_OPTS_START = { row: 0, asking: undefined, answer: 0 };
  var ZONE_START = {
    tab: "games",
    tabCursor: "games",
    zone: "cards",
    card: 0,
    action: 0,
    layers: [],
    optionsGroup: undefined,
    find: ZONE_FIND_START,
    opts: ZONE_OPTS_START,
    idleCard: 0,
    launch: undefined,
    session: undefined,
    tally: undefined
  };
  var topLayer = (nav) => nav.layers.at(-1) ?? "none";
  var OWN_LAYERS = ["gameopts", "keyboard", "idle"];
  function cardCount(korri) {
    return catalogGames(korri.catalog).length;
  }
  var bandGames = (nav, korri) => visibleGames(korri.catalog, nav.find.draft);
  var focusedGame = (nav, korri) => bandGames(nav, korri)[nav.card];
  var unpressed = (message) => message._tag === "KorriPublished" || message._tag === "IdleElapsed" || message._tag === "IdleTicked" || message._tag === "TallyCounted" || message._tag === "TallyElapsed" || message._tag === "TallyLeft" || message._tag === "TitleCardLeft" || message._tag === "LandedOptions";
  function update2(nav, message, korri) {
    const result = step2(nav, message, korri);
    return { model: settle2(result.model, korri), requests: result.requests };
  }
  function step2(nav, message, korri) {
    if (message._tag === "TitleCardLeft")
      return nav.launch?.leaving === true ? stay({ ...nav, launch: undefined }) : stay(nav);
    const tally = nav.tally;
    if (message._tag === "TallyLeft")
      return tally?.leaving === true ? stay({ ...nav, tally: undefined }) : stay(nav);
    if (tally !== undefined && !tally.leaving) {
      if (message._tag === "TallyCounted")
        return stay({ ...nav, tally: { ...tally, counted: true } });
      if (message._tag === "TallyElapsed")
        return stay({ ...nav, tally: { ...tally, leaving: true } });
      if (!unpressed(message))
        return stay({ ...nav, tally: pressTally(tally) });
    }
    const launch = nav.launch;
    if (launch !== undefined && !launch.leaving && !unpressed(message)) {
      if (launch.seen)
        return stay(nav);
      return step2({ ...nav, launch: dropLaunch(launch) }, message, korri);
    }
    if (topLayer(nav) === "idle") {
      if (message._tag === "IdleTicked")
        return stay({ ...nav, idleCard: (nav.idleCard + 1) % Math.max(1, cardCount(korri)) });
      return message._tag === "IdleElapsed" || message._tag === "KorriPublished" ? stay(nav) : stay(pop(nav));
    }
    switch (message._tag) {
      case "PressedBack":
        return back2(nav, korri);
      case "PressedOptions":
      case "ChoseGameOpts":
        return topLayer(nav) === "none" && nav.tab === "games" && bandGames(nav, korri).length > 0 ? stay(push({ ...nav, opts: ZONE_OPTS_START }, "gameopts")) : stay(nav);
      case "PressedMenu":
        if (topLayer(nav) !== "none")
          return stay(nav);
        if (nav.tab === "games" && nav.zone === "tabs") {
          return stay({ ...nav, zone: cardCount(korri) === 0 && korri.store !== undefined ? "get" : "find" });
        }
        if (nav.tab === "games" && nav.zone === "find" && korri.store !== undefined)
          return stay({ ...nav, zone: "get" });
        return stay({ ...nav, zone: "tabs", tabCursor: nav.tab });
      case "PressedSystem":
        return topLayer(nav) === "none" ? stay({ ...nav, tab: "options", tabCursor: "options", zone: "cards", optionsGroup: "system" }) : stay(nav);
      case "FocusedPage":
        return nav.tab === "games" || nav.zone !== "tabs" ? stay(nav) : stay({ ...nav, zone: "cards" });
      case "LandedOptions":
        return nav.optionsGroup === undefined ? stay(nav) : stay({ ...nav, optionsGroup: undefined });
      case "ShowedGame": {
        const shown = bandGames(nav, korri).some((game) => game.id === message.gameId) ? nav : { ...nav, find: ZONE_FIND_START };
        const card = bandGames(shown, korri).findIndex((game) => game.id === message.gameId);
        return card < 0 ? stay(nav) : stay({
          ...shown,
          tab: "games",
          tabCursor: "games",
          zone: "cards",
          card,
          action: 0,
          launch: zoneLaunchOpen(korri, message.gameId)
        });
      }
      case "KorriPublished":
        return stay(nav);
      case "IdleElapsed": {
        if (nav.layers.length > 0 || nav.launch !== undefined || nav.tally !== undefined)
          return stay(nav);
        const shown = focusedGame(nav, korri);
        const start = shown === undefined ? 0 : Math.max(0, catalogGames(korri.catalog).indexOf(shown));
        return stay(push({ ...nav, idleCard: start }, "idle"));
      }
      case "IdleTicked":
      case "Woke":
      case "PressedTally":
      case "TallyCounted":
      case "TallyElapsed":
        return stay(nav);
      case "FocusedTab":
        return stay({ ...nav, zone: "tabs", tabCursor: message.tab });
      case "ChoseTab":
        return stay({ ...nav, tab: message.tab, tabCursor: message.tab, zone: message.tab === "games" ? "cards" : "tabs" });
      case "FocusedCard":
        return stay({ ...nav, zone: "cards", card: clampCard(message.index, nav, korri), action: 0 });
      case "ChoseCard": {
        const card = clampCard(message.index, nav, korri);
        const game = bandGames(nav, korri)[card];
        const choices = game === undefined ? [] : launchChoices(game);
        if (game !== undefined && choices.length === 1) {
          return ask({ ...nav, zone: "cards", card, launch: zoneLaunchOpen(korri, game.id) }, launchGame(game.id, choices[0].locationId));
        }
        return stay({ ...nav, zone: "actions", card, action: 0 });
      }
      case "FocusedAction":
        return stay({ ...nav, zone: "actions", action: Math.max(0, message.index) });
      case "ChoseLaunch":
        return ask({ ...nav, launch: zoneLaunchOpen(korri, message.gameId) }, launchGame(message.gameId, message.locationId));
      case "FocusedFind":
        return stay({ ...nav, zone: "find" });
      case "FocusedGetGames":
        return stay({ ...nav, zone: "get" });
      case "ChoseGetGames":
        return topLayer(nav) === "none" && nav.tab === "games" ? stay({ ...nav, zone: "get" }) : stay(nav);
      case "ChoseFind":
        return topLayer(nav) === "none" && nav.tab === "games" ? stay(push({ ...nav, zone: "find", find: { ...nav.find, caret: nav.find.draft.length } }, "keyboard")) : stay(nav);
      case "FocusedKey":
        return topLayer(nav) === "keyboard" ? stay({ ...nav, find: { ...nav.find, key: message.id } }) : stay(nav);
      case "PressedKey": {
        if (topLayer(nav) !== "keyboard")
          return stay(nav);
        if (message.key.kind === "done")
          return stay({ ...pop(nav), zone: "cards" });
        return stay(refilter(nav, pressFindKey(nav.find, message.key), korri));
      }
      case "FocusedGameOpt":
        return topLayer(nav) === "gameopts" && nav.opts.asking === undefined ? stay({ ...nav, opts: { ...nav.opts, row: Math.max(0, message.index) } }) : stay(nav);
      case "ChoseGameAction": {
        if (topLayer(nav) !== "gameopts" || !message.enabled)
          return stay(nav);
        if (message.destructive) {
          return stay({ ...nav, opts: { ...nav.opts, asking: { gameId: message.gameId, actionId: message.actionId }, answer: 0 } });
        }
        return ask(closeOpts(nav), { _tag: "RunGameAction", gameId: message.gameId, actionId: message.actionId });
      }
      case "FocusedAnswer":
        return nav.opts.asking === undefined ? stay(nav) : stay({ ...nav, opts: { ...nav.opts, answer: message.index === 1 ? 1 : 0 } });
      case "AnsweredGameAction": {
        const asking = nav.opts.asking;
        if (topLayer(nav) !== "gameopts" || asking === undefined)
          return stay(nav);
        return message.yes ? ask(closeOpts(nav), { _tag: "RunGameAction", gameId: asking.gameId, actionId: asking.actionId }) : stay({ ...nav, opts: { ...nav.opts, asking: undefined, answer: 0 } });
      }
      case "ChoseReload":
        return korri.catalog._tag === "Error" ? ask(nav, { _tag: "Reload" }) : stay(nav);
    }
  }
  var launchGame = (gameId, locationId) => locationId === undefined ? { _tag: "LaunchGame", gameId } : { _tag: "LaunchGame", gameId, locationId };
  var closeOpts = (nav) => ({ ...nav, layers: nav.layers.filter((layer) => layer !== "gameopts"), opts: ZONE_OPTS_START });
  function refilter(nav, find, korri) {
    if (find.draft === nav.find.draft)
      return { ...nav, find };
    const was = focusedGame(nav, korri);
    const now = visibleGames(korri.catalog, find.draft);
    const card = was === undefined ? 0 : Math.max(0, now.indexOf(was));
    return { ...nav, find, card, action: 0 };
  }
  function back2(nav, korri) {
    switch (topLayer(nav)) {
      case "overlay":
        return ask(nav, { _tag: "DismissGameplayOverlay" });
      case "problem":
        return ask(nav, { _tag: "DismissProblem" });
      case "busy":
        return stay(nav);
      case "confirm":
        return stay(nav);
      case "onboarding":
        return stay(nav);
      case "gameopts":
        return nav.opts.asking === undefined ? stay(closeOpts(nav)) : stay({ ...nav, opts: { ...nav.opts, asking: undefined, answer: 0 } });
      case "keyboard":
        return nav.find.draft === "" ? stay({ ...pop(nav), zone: "cards", find: ZONE_FIND_START }) : stay(refilter(nav, deleteBack(nav.find.caret === 0 ? { ...nav.find, caret: nav.find.draft.length } : nav.find), korri));
      case "idle":
        return stay(pop(nav));
      case "none":
        if (nav.zone !== "cards" && nav.tab === "games")
          return stay({ ...nav, zone: "cards" });
        if (nav.tab !== "games") {
          return stay({ ...nav, tab: "games", tabCursor: "games", zone: "cards", optionsGroup: undefined });
        }
        if (nav.find.draft !== "")
          return stay({ ...refilter(nav, { ...nav.find, draft: "", caret: 0 }, korri), find: ZONE_FIND_START });
        return stay(nav);
    }
  }
  var push = (nav, layer) => ({ ...nav, layers: [...nav.layers, layer] });
  var pop = (nav) => ({ ...nav, layers: nav.layers.slice(0, -1) });
  function clampCard(index, nav, korri) {
    return Math.min(Math.max(0, index), Math.max(0, bandGames(nav, korri).length - 1));
  }
  function settle2(nav, korri) {
    return settleLayers(settleMoments(nav, korri), korri);
  }
  function settleMoments(nav, korri) {
    const launch = settleLaunch(nav.launch, korri.status);
    const ended = nav.tab === "games" && !nav.layers.some((layer) => layer === "onboarding" || OWN_LAYERS.includes(layer)) ? zoneTallyOf(nav.session, korri) : undefined;
    const session = settleSession(nav.session, korri);
    const tally = ended ?? nav.tally;
    return launch === nav.launch && session === nav.session && tally === nav.tally ? nav : { ...nav, launch, session, tally };
  }
  function settleLayers(nav, korri) {
    const own = nav.layers.filter((layer) => OWN_LAYERS.includes(layer));
    const status = korri.status._tag;
    const layers = [
      ...korri.onboarding === undefined ? [] : ["onboarding"],
      ...own.filter((layer) => layer !== "idle"),
      ...status === "Busy" ? ["busy"] : [],
      ...status === "Problem" ? ["problem"] : [],
      ...status === "Confirm" ? ["confirm"] : [],
      ...korri.presentation.kind === "gameplay-overlay" ? ["overlay"] : [],
      ...own.includes("idle") && own.length === 1 && status === "Browsing" && nav.launch === undefined && nav.tally === undefined && korri.presentation.kind === "catalog" && korri.onboarding === undefined ? ["idle"] : []
    ];
    const card = clampCard(nav.card, nav, korri);
    const same = card === nav.card && layers.length === nav.layers.length && layers.every((layer, index) => layer === nav.layers[index]);
    return same ? nav : { ...nav, card, layers };
  }

  // ../../../surfaces/zone/src/state/onboarding.ts
  var ZONE_ONBOARDING_STEPS = ["controllers", "wifi", "name", "identity"];
  var TARGET = {
    next: "next",
    finish: "finish",
    save: "save",
    connection: "connection",
    scan: "scan",
    skip: "skip",
    name: "name"
  };
  var networkTarget = (networkId) => `net:${networkId}`;
  var seatTarget = (controllerId) => `seat:${controllerId}`;
  var ZONE_ONBOARDING_START = {
    step: "controllers",
    focus: TARGET.next,
    entry: undefined,
    started: false,
    finishing: false,
    join: undefined
  };
  var DEVICE_NAME_SETTING_ID = "device-name";
  function deviceNameSetting(korri) {
    return korri.settings.flatMap((group) => group.items).find((item) => item.id === DEVICE_NAME_SETTING_ID);
  }
  function editableName(korri) {
    const setting = deviceNameSetting(korri);
    return setting?.interaction?.kind === "text" ? setting : undefined;
  }
  var anyHeard = (korri) => (korri.onboarding?.controllers ?? []).some((controller) => controller.heard);
  function wifiSettled(korri) {
    return korri.network === undefined || korri.network.connection._tag !== "Offline";
  }
  function canGoOn(step3, korri) {
    switch (step3) {
      case "controllers":
        return anyHeard(korri);
      case "wifi":
        return wifiSettled(korri);
      case "name":
        return true;
      case "identity":
        return false;
    }
  }
  function onboardingTargets(model, korri) {
    switch (model.step) {
      case "controllers":
        return [
          ...(korri.onboarding?.controllers ?? []).filter((controller) => controller.heard).map((controller) => seatTarget(controller.id)),
          TARGET.next
        ];
      case "wifi": {
        const network = korri.network;
        const wifi = network?.wifi;
        const networks = wifi?._tag === "Available" ? wifi.networks.map((item) => networkTarget(item.id)) : [];
        return [
          ...network === undefined ? [] : [TARGET.connection],
          ...networks,
          ...wifi?._tag === "Available" ? [TARGET.scan] : [],
          TARGET.skip,
          TARGET.next
        ];
      }
      case "name":
        return [TARGET.name, TARGET.next];
      case "identity":
        return [TARGET.save, TARGET.finish];
    }
  }
  function firstTarget(model, korri) {
    switch (model.step) {
      case "controllers":
        return TARGET.next;
      case "wifi": {
        const wifi = korri.network?.wifi;
        const first = wifi?._tag === "Available" ? wifi.networks[0] : undefined;
        return first !== undefined ? networkTarget(first.id) : wifi?._tag === "Available" ? TARGET.scan : TARGET.skip;
      }
      case "name":
        return TARGET.name;
      case "identity":
        return model.started ? TARGET.finish : TARGET.save;
    }
  }
  var go = (model, step3, korri) => {
    const moved = { ...model, step: step3, entry: undefined };
    return { ...moved, focus: firstTarget(moved, korri) };
  };
  var nextStep = (step3) => ZONE_ONBOARDING_STEPS[Math.min(ZONE_ONBOARDING_STEPS.indexOf(step3) + 1, ZONE_ONBOARDING_STEPS.length - 1)] ?? step3;
  var previousStep = (step3) => ZONE_ONBOARDING_STEPS[Math.max(ZONE_ONBOARDING_STEPS.indexOf(step3) - 1, 0)] ?? step3;
  function entryKeyId(key) {
    switch (key.kind) {
      case "char":
        return key.char;
      case "space":
      case "delete":
      case "done":
        return key.kind;
      case "layer":
        return `layer:${key.layer}`;
    }
  }
  function updateOnboarding(model, message, korri) {
    const result = model.finishing ? stay(model) : step3(model, message, korri);
    return { model: settle3(result.model, korri), requests: result.requests };
  }
  function step3(model, message, korri) {
    switch (message._tag) {
      case "KorriPublished":
        return stay(model);
      case "PressedBack":
        if (model.entry !== undefined)
          return stay({ ...model, entry: undefined, focus: TARGET.name });
        return model.step === "controllers" ? stay(model) : stay(go(model, previousStep(model.step), korri));
      case "FocusedTarget":
        return model.entry !== undefined ? stay(model) : stay({ ...model, focus: message.target });
      case "ChoseNext":
        return canGoOn(model.step, korri) ? stay(go(model, nextStep(model.step), korri)) : stay(model);
      case "ChoseSkip":
        return model.step === "wifi" ? stay(go(model, "name", korri)) : stay(model);
      case "ChoseScan":
        return model.step === "wifi" && korri.network?.wifi._tag === "Available" ? ask({ ...model, focus: TARGET.scan }, { _tag: "ScanWifi" }) : stay(model);
      case "OnboardingJoin":
        return chooseNetwork(model, message.networkId, korri);
      case "OpenedName": {
        const setting = editableName(korri);
        if (model.step !== "name" || setting === undefined)
          return stay(model);
        const draft = setting.value ?? "";
        return stay({ ...model, focus: TARGET.name, entry: { draft, caret: draft.length, layer: "letters", focusedKey: "done" } });
      }
      case "PressedKey":
        return pressKey(model, message.key, korri);
      case "FocusedKey":
        return model.entry === undefined || model.entry.focusedKey === message.id ? stay(model) : stay({ ...model, entry: { ...model.entry, focusedKey: message.id } });
      case "ChoseStart":
        return model.step === "identity" ? stay({ ...model, started: true, focus: TARGET.finish }) : stay(model);
      case "ChoseFinish":
        return model.step === "identity" && model.started ? ask({ ...model, finishing: true, focus: TARGET.finish }, { _tag: "FinishOnboarding" }) : stay(model);
    }
  }
  function chooseNetwork(model, networkId, korri) {
    const wifi = korri.network?.wifi;
    const network = wifi?._tag === "Available" ? wifi.networks.find((item) => item.id === networkId) : undefined;
    if (model.step !== "wifi" || network === undefined)
      return stay(model);
    const focused = { ...model, focus: networkTarget(network.id) };
    return network.known || !network.secured ? ask({ ...focused, join: undefined }, { _tag: "ConnectWifi", networkId: network.id }) : stay({ ...focused, join: network.id });
  }
  function pressKey(model, key, korri) {
    const entry = model.entry;
    const setting = editableName(korri);
    if (entry === undefined || setting === undefined)
      return stay(model);
    const lit = { ...entry, focusedKey: entryKeyId(key) };
    const maxLength = setting.interaction?.kind === "text" ? setting.interaction.maxLength : undefined;
    const { draft, caret } = entry;
    switch (key.kind) {
      case "char":
      case "space": {
        if (maxLength !== undefined && draft.length >= maxLength)
          return stay({ ...model, entry: lit });
        const char = key.kind === "space" ? " " : key.char;
        return stay({ ...model, entry: { ...lit, draft: draft.slice(0, caret) + char + draft.slice(caret), caret: caret + 1 } });
      }
      case "delete":
        return caret === 0 ? stay({ ...model, entry: lit }) : stay({ ...model, entry: { ...lit, draft: draft.slice(0, caret - 1) + draft.slice(caret), caret: caret - 1 } });
      case "layer":
        return stay({ ...model, entry: { ...lit, layer: key.layer } });
      case "done": {
        const closed = { ...model, entry: undefined, focus: TARGET.name };
        return draft === (setting.value ?? "") ? stay(closed) : ask(closed, { _tag: "ChangeSetting", settingId: setting.id, value: draft });
      }
    }
  }
  function settle3(model, korri) {
    const entry = model.entry !== undefined && editableName(korri) === undefined ? undefined : model.entry;
    const focus = onboardingTargets(model, korri).includes(model.focus) ? model.focus : firstTarget(model, korri);
    return entry === model.entry && focus === model.focus ? model : { ...model, entry, focus };
  }

  // ../../../surfaces/zone/src/state/wifi.ts
  var NOTHING = { _tag: "Nothing" };
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
    return network.secured && !network.known ? { _tag: "Type", entry: { _tag: "WifiPassword", networkId: network.id, ssid: network.ssid } } : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: network.id } };
  }
  function scan(korri) {
    const wifi = korri.network?.wifi;
    return wifi?._tag === "Available" && !wifi.scanning ? { _tag: "Send", request: { _tag: "ScanWifi" } } : NOTHING;
  }
  var joinHidden = () => ({ _tag: "Type", entry: { _tag: "HiddenSsid" } });
  function submitWifi(entry, draft) {
    switch (entry._tag) {
      case "HiddenSsid": {
        const ssid = draft.trim();
        return ssid === "" ? NOTHING : { _tag: "Type", entry: { _tag: "HiddenPassword", ssid } };
      }
      case "HiddenPassword":
        return {
          _tag: "Send",
          request: draft === "" ? { _tag: "ConnectHiddenWifi", ssid: entry.ssid } : { _tag: "ConnectHiddenWifi", ssid: entry.ssid, password: draft }
        };
      case "WifiPassword":
        return draft === "" ? NOTHING : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: entry.networkId, password: draft } };
      case "Setting":
        return NOTHING;
    }
  }
  function rejectedEntry(previous, korri) {
    const attempt = lastAttempt(korri);
    if (attempt._tag !== "Failed" || !attempt.passwordRejected || sameAttempt(attempt, previous))
      return;
    const network = scannedNetworks(korri).find((candidate) => candidate.ssid === attempt.ssid);
    return network === undefined ? { _tag: "HiddenPassword", ssid: attempt.ssid } : { _tag: "WifiPassword", networkId: network.id, ssid: network.ssid };
  }
  function sameAttempt(a, b) {
    switch (a._tag) {
      case "Idle":
        return b._tag === "Idle";
      case "Connecting":
        return b._tag === "Connecting" && a.ssid === b.ssid;
      case "Failed":
        return b._tag === "Failed" && a.ssid === b.ssid && a.message === b.message && a.passwordRejected === b.passwordRejected;
    }
  }

  // ../../../surfaces/zone/src/state/options.ts
  var OPTIONS_START = {
    focused: undefined,
    at: 0,
    keyboard: undefined,
    dialog: undefined,
    attempt: { _tag: "Idle" },
    pick: undefined
  };
  var optionsStart = (korri) => ({ ...OPTIONS_START, attempt: lastAttempt(korri) });
  function optionsLayer(state) {
    return state.keyboard !== undefined ? "keyboard" : state.dialog !== undefined ? "dialog" : "none";
  }
  function focusSystemGroup(state) {
    return optionsLayer(state) === "none" ? { ...state, focused: ZONE_OPTIONS_SYSTEM } : state;
  }
  function update3(state, message, korri) {
    const plan = optionsPlan(korri);
    const result = step4(settleFocus(state, plan), message, korri, plan);
    return result.model === state && result.requests.length === 0 ? stay(state) : result;
  }
  function step4(state, message, korri, plan) {
    switch (message._tag) {
      case "KorriPublished":
        return stay(published(state, korri, plan));
      case "PressedBack":
        if (state.keyboard !== undefined)
          return stay({ ...state, keyboard: undefined });
        if (state.dialog !== undefined)
          return stay({ ...state, dialog: undefined });
        return stay(state);
      case "PressedMenu":
        return state.keyboard === undefined ? stay(state) : commit(state, state.keyboard);
      case "FocusedRow": {
        const at = plan.rows.findIndex((row) => row.id === message.rowId);
        return at < 0 || state.keyboard !== undefined || state.dialog !== undefined ? stay(state) : stay({ ...state, focused: message.rowId, at });
      }
      case "PressedRow":
        return state.keyboard !== undefined || state.dialog !== undefined ? stay(state) : press(state, message.rowId, korri, plan);
      case "SteppedRow":
        return state.keyboard !== undefined || state.dialog !== undefined ? stay(state) : stepChoice(state, message.rowId, message.direction, korri, plan);
      case "FocusedKey":
        return state.keyboard === undefined ? stay(state) : stay({ ...state, keyboard: { ...state.keyboard, focusedKey: message.keyId } });
      case "PressedKey":
        return state.keyboard === undefined ? stay(state) : key(state, state.keyboard, message.key);
      case "PressedClear": {
        const entry = state.keyboard?.entry;
        return entry?._tag === "Setting" && (entry.secret || entry.numeric?.clearable === true) && entry.clearLabel !== undefined ? ask({ ...state, keyboard: undefined }, { _tag: "ChangeSetting", settingId: entry.settingId, value: "" }) : stay(state);
      }
      case "FocusedDialogButton":
        return state.dialog === undefined ? stay(state) : stay({ ...state, dialog: { ...state.dialog, focused: message.index } });
      case "PressedDialogButton": {
        const dialog = state.dialog;
        if (dialog === undefined)
          return stay(state);
        return message.index === 1 ? ask({ ...state, dialog: undefined }, { _tag: "RunAction", actionId: dialog.actionId }) : stay({ ...state, dialog: undefined });
      }
    }
  }
  function settleFocus(state, plan) {
    if (state.focused === undefined)
      return state;
    const row = focusedRow(plan, state.focused);
    if (row !== undefined) {
      const at = plan.rows.indexOf(row);
      return row.id === state.focused && at === state.at ? state : { ...state, focused: row.id, at };
    }
    const fallback = plan.rows[Math.min(state.at, plan.rows.length - 1)];
    return fallback === undefined ? { ...state, focused: undefined, at: 0 } : { ...state, focused: fallback.id, at: Math.min(state.at, plan.rows.length - 1) };
  }
  function published(state, korri, plan) {
    const attempt = lastAttempt(korri);
    const reopen = state.keyboard === undefined ? rejectedEntry(state.attempt, korri) : undefined;
    let next = sameAttempt(state.attempt, attempt) ? state : { ...state, attempt };
    if (reopen !== undefined) {
      const rowId = reopen._tag === "WifiPassword" ? zoneNetworkRowId(reopen.networkId) : ZONE_WIFI_HIDDEN;
      const at = plan.rows.findIndex((row) => row.id === rowId);
      next = { ...next, keyboard: openKeyboard(reopen, ""), dialog: undefined, ...at < 0 ? {} : { focused: rowId, at } };
    }
    const entry = next.keyboard?.entry;
    if (entry?._tag === "Setting") {
      const kind = plan.rows.find((row) => row.id === entry.settingId)?.item.interaction?.kind;
      if (kind !== "text" && kind !== "sensitiveText" && kind !== "number")
        next = { ...next, keyboard: undefined };
    }
    const dialog = next.dialog;
    if (dialog !== undefined && !plan.rows.some((row) => row.press._tag === "Run" && row.press.actionId === dialog.actionId)) {
      next = { ...next, dialog: undefined };
    }
    return next;
  }
  var openKeyboard = (entry, draft) => ({ entry, draft, caret: draft.length, layer: "letters" });
  function wifi(state, next) {
    switch (next._tag) {
      case "Nothing":
        return stay(state);
      case "Send":
        return ask({ ...state, keyboard: undefined }, next.request);
      case "Type":
        return stay({ ...state, keyboard: openKeyboard(next.entry, "") });
    }
  }
  function press(state, rowId, korri, plan) {
    const at = plan.rows.findIndex((row2) => row2.id === rowId);
    const row = plan.rows[at];
    if (row === undefined)
      return stay(state);
    const here = { ...state, focused: row.id, at };
    const action = row.press;
    switch (action._tag) {
      case "Nothing":
        return stay(here);
      case "Step": {
        if (!isPick(row.item))
          return stepChoice(here, rowId, 1, korri, plan);
        const interaction = row.item.interaction;
        const at2 = state.pick?.rowId === rowId ? state.pick.at : 0;
        const choice = interaction?.kind === "choice" ? interaction.choices[at2] : undefined;
        if (choice === undefined || saving(korri, row.id))
          return stay(here);
        return ask({ ...here, pick: undefined }, { _tag: "ChangeSetting", settingId: row.id, value: choice.value });
      }
      case "Edit": {
        if (saving(korri, row.id))
          return stay(here);
        const entry = {
          _tag: "Setting",
          settingId: action.settingId,
          label: action.label,
          secret: action.secret,
          ...action.maxLength === undefined ? {} : { maxLength: action.maxLength },
          ...action.clearLabel === undefined ? {} : { clearLabel: action.clearLabel },
          ...action.placeholder === undefined ? {} : { placeholder: action.placeholder },
          ...action.numeric === undefined ? {} : { numeric: action.numeric }
        };
        return stay({ ...here, keyboard: openKeyboard(entry, action.draft) });
      }
      case "Run":
        if (saving(korri, row.id))
          return stay(here);
        return action.confirm === undefined ? ask(here, { _tag: "RunAction", actionId: action.actionId }) : stay({ ...here, dialog: { actionId: action.actionId, confirm: action.confirm, focused: 0 } });
      case "DismissProblem":
        return ask(here, { _tag: "DismissSettingsProblem" });
      case "Scan":
        return wifi(here, scan(korri));
      case "Join":
        return wifi(here, pickNetwork(korri, action.networkId));
      case "JoinHidden":
        return wifi(here, joinHidden());
      case "Look":
        return stay(here);
    }
  }
  var saving = (korri, settingId) => korri.settingsStatus._tag === "Saving" && korri.settingsStatus.settingId === settingId;
  function stepChoice(state, rowId, direction, korri, plan) {
    const row = plan.rows.find((candidate) => candidate.id === rowId);
    const interaction = row?.item.interaction;
    if (row !== undefined && interaction?.kind === "number") {
      if (saving(korri, row.id))
        return stay(state);
      const next2 = zoneNumberStep(interaction, direction);
      return next2 === undefined ? stay(state) : ask(state, { _tag: "ChangeSetting", settingId: row.id, value: String(next2) });
    }
    if (row === undefined || interaction?.kind !== "choice" || interaction.choices.length === 0)
      return stay(state);
    if (isPick(row.item)) {
      const count2 = interaction.choices.length;
      const from = state.pick?.rowId === rowId ? state.pick.at : 0;
      return stay({ ...state, pick: { rowId, at: (from + direction + count2) % count2 } });
    }
    if (row.press._tag === "Look")
      return stay(state);
    if (saving(korri, row.id))
      return stay(state);
    const count = interaction.choices.length;
    const at = choiceIndex(row.item);
    const next = at < 0 ? direction > 0 ? 0 : count - 1 : (at + direction + count) % count;
    const choice = interaction.choices[next];
    if (choice === undefined || next === at)
      return stay(state);
    return ask(state, { _tag: "ChangeSetting", settingId: row.id, value: choice.value });
  }
  function key(state, keyboard, pressed) {
    const { draft, caret } = keyboard;
    const limit = entryHeading(keyboard.entry).maxLength;
    switch (pressed.kind) {
      case "char":
      case "space": {
        if (limit !== undefined && draft.length >= limit)
          return stay(state);
        const char = pressed.kind === "space" ? " " : pressed.char;
        return stay({
          ...state,
          keyboard: { ...keyboard, draft: draft.slice(0, caret) + char + draft.slice(caret), caret: caret + 1 }
        });
      }
      case "delete":
        return caret === 0 ? stay(state) : stay({ ...state, keyboard: { ...keyboard, draft: draft.slice(0, caret - 1) + draft.slice(caret), caret: caret - 1 } });
      case "layer":
        return keyboard.layer === pressed.layer ? stay(state) : stay({ ...state, keyboard: { ...keyboard, layer: pressed.layer } });
      case "done":
        return commit(state, keyboard);
    }
  }
  function commit(state, keyboard) {
    const entry = keyboard.entry;
    if (entry._tag !== "Setting")
      return wifi(state, submitWifi(entry, keyboard.draft));
    if (entry.secret && keyboard.draft === "")
      return stay({ ...state, keyboard: undefined });
    if (entry.numeric !== undefined) {
      if (keyboard.draft.trim() === "") {
        return entry.numeric.clearable ? ask({ ...state, keyboard: undefined }, { _tag: "ChangeSetting", settingId: entry.settingId, value: "" }) : stay({ ...state, keyboard: undefined });
      }
      if (!zoneNumberFits(keyboard.draft, entry.numeric))
        return stay(state);
    }
    const request = { _tag: "ChangeSetting", settingId: entry.settingId, value: keyboard.draft };
    return ask({ ...state, keyboard: undefined }, request);
  }

  // ../../../surfaces/zone/src/state/overlay.ts
  var ZONE_OVERLAY_START = {
    focusedId: undefined,
    asking: undefined,
    answer: 0,
    moving: {},
    leaving: false
  };
  function overlayControls(overlay) {
    return [...overlay.controls, ...overlay.groups.flatMap((group) => group.controls)];
  }
  var findControl = (overlay, id) => id === undefined ? undefined : overlayControls(overlay).find((control) => control.id === id);
  function placesOf(step5) {
    const text = String(step5);
    const dot = text.indexOf(".");
    return dot === -1 ? 0 : text.length - dot - 1;
  }
  function rangeStep(value, min, max, step5, direction) {
    const next = Number((value + direction * step5).toFixed(placesOf(step5)));
    return Math.min(max, Math.max(min, next));
  }
  function stepValue(model, control, direction) {
    const interaction = control.interaction;
    switch (interaction.kind) {
      case "command":
        return;
      case "toggle":
        return { kind: "toggle", value: !interaction.value };
      case "choice": {
        const count = interaction.options.length;
        if (count < 2)
          return;
        const at = interaction.options.findIndex((option) => option.value === interaction.value);
        const next = interaction.options[((at === -1 ? 0 : at + direction) % count + count) % count];
        return next === undefined ? undefined : { kind: "choice", value: next.value };
      }
      case "range": {
        const from = model.moving[control.id] ?? interaction.value;
        const next = rangeStep(from, interaction.min, interaction.max, interaction.step, direction);
        return next === from ? undefined : { kind: "range", value: next };
      }
    }
  }
  function invoke(model, control, value) {
    const request = value === undefined ? { _tag: "InvokeGameplayControl", controlId: control.id } : { _tag: "InvokeGameplayControl", controlId: control.id, value };
    const moving = value?.kind === "range" ? { ...model.moving, [control.id]: value.value } : model.moving;
    return ask({ ...model, moving, leaving: model.leaving || control.dismissOnSuccess }, request);
  }
  function change(model, control, value) {
    if (control.destructive) {
      return stay({ ...model, asking: value === undefined ? { controlId: control.id } : { controlId: control.id, value }, answer: 0 });
    }
    return invoke(model, control, value);
  }
  function updateOverlay(model, message, overlay) {
    const result = step5(model, message, overlay);
    return { model: settle4(result.model, overlay), requests: result.requests };
  }
  function step5(model, message, overlay) {
    switch (message._tag) {
      case "KorriPublished":
        return stay({ ...model, leaving: false });
      case "PressedBack":
        return model.asking !== undefined ? stay({ ...model, asking: undefined, answer: 0 }) : ask(model, { _tag: "DismissGameplayOverlay" });
      case "FocusedControl": {
        if (model.asking !== undefined)
          return stay(model);
        const left = model.focusedId !== undefined && model.focusedId !== message.controlId && model.moving[model.focusedId] !== undefined;
        const moving = left ? omit(model.moving, model.focusedId) : model.moving;
        return stay({ ...model, focusedId: message.controlId, moving });
      }
      case "PressedControl":
        return press2(model, message.controlId, overlay);
      case "SteppedControl": {
        const control = findControl(overlay, message.controlId);
        if (control === undefined || !control.enabled || model.leaving || model.asking !== undefined)
          return stay(model);
        if (control.interaction.kind === "command")
          return stay(model);
        const value = stepValue(model, control, message.direction);
        return value === undefined ? stay(model) : change({ ...model, focusedId: control.id }, control, value);
      }
      case "FocusedAnswer":
        return model.asking === undefined ? stay(model) : stay({ ...model, answer: message.index });
      case "ChoseAnswer": {
        const question = model.asking;
        if (question === undefined)
          return stay(model);
        const closed = { ...model, asking: undefined, answer: 0 };
        const control = findControl(overlay, question.controlId);
        if (!message.yes || control === undefined || !control.enabled)
          return stay(closed);
        return invoke(closed, control, question.value);
      }
    }
  }
  function press2(model, controlId, overlay) {
    const control = findControl(overlay, controlId);
    if (model.asking !== undefined || control === undefined)
      return stay(model);
    if (!control.enabled || model.leaving)
      return stay({ ...model, focusedId: control.id });
    const focused = { ...model, focusedId: control.id };
    switch (control.interaction.kind) {
      case "command":
        return change(focused, control, undefined);
      case "toggle":
      case "choice":
        return change(focused, control, stepValue(focused, control, 1));
      case "range":
        return stay(focused);
    }
  }
  function omit(record, key2) {
    const { [key2]: _gone, ...rest } = record;
    return rest;
  }
  function settle4(model, overlay) {
    const controls = overlayControls(overlay);
    const focusedId = controls.some((control) => control.id === model.focusedId) ? model.focusedId : controls[0]?.id;
    const askedControl = findControl(overlay, model.asking?.controlId);
    const asking = askedControl?.enabled === true ? model.asking : undefined;
    let moving = model.moving;
    for (const id of Object.keys(model.moving)) {
      const control = findControl(overlay, id);
      const caughtUp = control?.interaction.kind === "range" && control.interaction.value === model.moving[id];
      if (control === undefined || !control.enabled || control.interaction.kind !== "range" || caughtUp) {
        moving = omit(moving, id);
      }
    }
    const same = focusedId === model.focusedId && asking === model.asking && moving === model.moving;
    return same ? model : { ...model, focusedId, asking, answer: asking === undefined ? 0 : model.answer, moving };
  }

  // ../../../surfaces/zone/src/plan/launch-settings.ts
  var launchOpen = (korri) => korri.launchSettings !== undefined && korri.launchSettings._tag !== "Closed";
  var pluginSection = (title) => title.includes(" · ");
  var ZONE_LAUNCH_SCOPE_ROW = "zone:launch-scope";
  var launchScopeStep = (count, scope, direction) => count === 0 ? 0 : (scope + direction + count) % count;
  var sameScope = (a, b) => a.kind === b.kind && a.label === b.label;
  function launchScopes(launch) {
    const scopes = [];
    for (const place of launch.places) {
      if (!scopes.some((scope) => sameScope(scope, place.scope)))
        scopes.push({ kind: place.scope.kind, label: place.scope.label });
    }
    return scopes;
  }
  function launchPlan(launch, status, scope = 0) {
    const close = launch.actions.find((action) => action.id.endsWith(":cancel"));
    const rows = launch._tag === "Ready";
    const scopes = launchScopes(launch);
    const shown = Math.max(0, Math.min(scope, scopes.length - 1));
    const chosen = scopes[shown];
    const places = chosen === undefined ? [] : launch.places.filter((place) => sameScope(place.scope, chosen));
    const runner = {
      title: "Runner",
      items: [{
        id: ZONE_LAUNCH_SCOPE_ROW,
        label: "Runner scope",
        value: chosen?.label ?? "",
        description: "Which runners the values below apply to.",
        interaction: {
          kind: "choice",
          choices: scopes.map((candidate, index) => ({ value: String(index), label: candidate.label })),
          selected: String(shown)
        }
      }]
    };
    return {
      slices: {
        settings: rows ? [
          ...scopes.length > 1 ? [runner] : [],
          ...places.map((place) => ({ title: place.layer.title, items: place.groups.flatMap((group) => group.items) }))
        ] : [],
        settingsStatus: status,
        actions: []
      },
      rows,
      scopes,
      scope: shown,
      actions: launch.actions,
      ...close === undefined ? {} : { closeActionId: close.id },
      captions: new Map(places.flatMap((place) => place.groups.flatMap((group, index) => {
        const first = group.items[0];
        const previous = place.groups[index - 1];
        const captioned = pluginSection(group.title) || previous !== undefined && pluginSection(previous.title);
        return first !== undefined && captioned ? [[first.id, group.title]] : [];
      })))
    };
  }
  function launchSlices(korri, scope = 0) {
    const launch = korri.launchSettings;
    return launch === undefined || launch._tag === "Closed" ? undefined : launchPlan(launch, korri.settingsStatus, scope).slices;
  }

  // ../../../surfaces/zone/src/plan/options-run-with.ts
  var zoneRouteRowId = (runnerId) => `route:${runnerId}`;
  var zoneSavedRowId = (label) => `saved:${label}`;
  function runWithPlan(choice) {
    const routes = choice.routes.map((route) => ({
      id: zoneRouteRowId(route.runnerId),
      route,
      saved: choice.saved.filter((saved) => saved.runnerId === route.runnerId).map((saved) => saved.label)
    }));
    const orphans = choice.saved.filter((saved) => !choice.routes.some((route) => route.runnerId === saved.runnerId));
    const all = [...choice.routes.flatMap((route) => route.actions), ...choice.actions];
    const ids = [
      ...orphans.map((orphan) => zoneSavedRowId(orphan.label)),
      ...routes.flatMap((entry) => [entry.id, ...entry.route.actions.map((action) => action.id)]),
      ...choice.actions.map((action) => action.id)
    ];
    return { routes, orphans, ids, actions: new Map(all.map((action) => [action.id, action])) };
  }

  // ../../../surfaces/zone/src/state/run-with.ts
  var RUN_WITH_START = { focused: undefined };
  function update4(state, message, korri) {
    const choice = korri.runnerChoice;
    if (choice === undefined || choice._tag === "Closed") {
      return state.focused === undefined ? stay(state) : stay(RUN_WITH_START);
    }
    const plan = runWithPlan(choice);
    switch (message._tag) {
      case "KorriPublished":
        return state.focused === undefined || plan.ids.includes(state.focused) ? stay(state) : stay(RUN_WITH_START);
      case "PressedBack":
        return stay(state);
      case "FocusedRow":
        return plan.ids.includes(message.rowId) ? stay({ focused: message.rowId }) : stay(state);
      case "PressedAction": {
        const action = plan.actions.get(message.actionId);
        return action === undefined || !action.enabled ? stay(state) : ask(state, { _tag: "RunAction", actionId: action.id });
      }
    }
  }

  // ../../../surfaces/zone/src/state/status.ts
  var ZONE_STATUS_START = { shown: "none", focus: 0 };
  function shownFor(status) {
    switch (status._tag) {
      case "Browsing":
        return "none";
      case "Busy":
        return "busy";
      case "Running":
        return "running";
      case "Problem":
        return "problem";
      case "Confirm":
      case "ChooseSave":
        return "confirm";
      case "Paused":
        return "paused";
      case "Ended":
        return "ended";
    }
  }
  function problemButtons(status) {
    return status._tag === "Problem" && status.canRetry ? ["dismiss", "retry"] : ["dismiss"];
  }
  function statusButtonCount(status) {
    switch (status._tag) {
      case "Busy":
        return status.actions?.length ?? 0;
      case "Problem":
        return problemButtons(status).length;
      case "Confirm":
        return status.actions.length;
      case "ChooseSave":
        return status.sides.length + status.actions.length;
      case "Paused":
        return status.actions.length;
      case "Ended":
        return 1;
      case "Browsing":
      case "Running":
        return 0;
    }
  }
  function updateStatus(model, message, status) {
    const result = step6(model, message, status);
    return { model: settle5(result.model, status), requests: result.requests };
  }
  function step6(model, message, status) {
    switch (message._tag) {
      case "KorriPublished":
        return stay(model);
      case "PressedBack":
        return status._tag === "Problem" || status._tag === "Ended" ? ask(model, { _tag: "DismissProblem" }) : stay(model);
      case "FocusedStatusButton":
        return stay({ ...model, focus: Math.max(0, message.index) });
      case "ChoseBusyAction":
        return status._tag === "Busy" ? runOffered(model, status.actions, message.actionId) : stay(model);
      case "ChoseProblemButton":
        if (status._tag !== "Problem")
          return stay(model);
        if (message.button === "retry")
          return status.canRetry ? ask(model, { _tag: "Retry" }) : stay(model);
        return ask(model, { _tag: "DismissProblem" });
      case "ChoseConfirmAction":
        return status._tag === "Confirm" ? runOffered(model, status.actions, message.actionId) : stay(model);
      case "ChosePausedAction":
        return status._tag === "Paused" ? runOffered(model, status.actions, message.actionId) : stay(model);
      case "ChoseEndedDismiss":
        return status._tag === "Ended" ? ask(model, { _tag: "DismissProblem" }) : stay(model);
    }
  }
  function runOffered(model, actions, actionId) {
    const action = actions?.find((candidate) => candidate.id === actionId);
    return action?.enabled === true ? ask(model, { _tag: "RunAction", actionId: action.id }) : stay(model);
  }
  function opening(status) {
    if (status._tag !== "Confirm" && status._tag !== "Paused")
      return 0;
    return Math.max(0, status.actions.findIndex((action) => action.enabled));
  }
  function settle5(model, status) {
    const shown = shownFor(status);
    const count = statusButtonCount(status);
    const focus = shown !== model.shown ? opening(status) : Math.min(model.focus, Math.max(0, count - 1));
    return shown === model.shown && focus === model.focus ? model : { shown, focus };
  }

  // ../../../surfaces/zone/src/plan/store.ts
  var ZONE_STORE_SEARCH_ROW = "search";
  var zoneStoreProviderRow = (providerId) => `provider:${providerId}`;
  var zoneStoreEntryRow = (entryId) => `entry:${entryId}`;
  var ZONE_STORE_QUERY_MAX = 40;
  function zoneStoreSearch(store, provider) {
    const search = store.search;
    return search._tag !== "Idle" && search.providerId === provider ? search : { _tag: "Idle" };
  }
  function zoneStoreGetActions(get) {
    switch (get._tag) {
      case "Offer":
      case "Confirm":
      case "Working":
      case "Problem":
        return get.actions;
      case "Unavailable":
      case "InLibrary":
        return [];
    }
  }
  function zoneStoreActions(store) {
    return [
      ...store.providers._tag === "Problem" ? store.providers.actions : [],
      ...store.search._tag === "Problem" ? store.search.actions : [],
      ...store.details._tag === "Problem" ? store.details.actions : [],
      ...store.details._tag === "Ready" ? zoneStoreGetActions(store.details.get) : []
    ];
  }
  var zoneStoreActionLive = (store, actionId) => zoneStoreActions(store).some((action) => action.id === actionId && action.enabled);

  // ../../../surfaces/zone/src/state/store.ts
  var ZONE_STORE_START = {
    open: false,
    page: "providers",
    provider: undefined,
    keyboard: undefined,
    focused: undefined,
    dialog: 0
  };
  function updateStore(model, message, korri) {
    const store = korri.store;
    if (store === undefined)
      return stay(model === ZONE_STORE_START ? model : ZONE_STORE_START);
    switch (message._tag) {
      case "OpenedStore":
        return ask({ ...ZONE_STORE_START, open: true }, { _tag: "OpenStore" });
      case "KorriPublished":
        return stay(model);
      case "PressedBack":
        return stay(back3(model, store));
      case "FocusedRow":
        return model.focused === message.rowId ? stay(model) : stay({ ...model, focused: message.rowId });
      case "FocusedDialogButton":
        return stay({ ...model, dialog: Math.max(0, message.index) });
      case "FocusedKey":
        return model.keyboard === undefined ? stay(model) : stay({ ...model, keyboard: { ...model.keyboard, key: message.id } });
      case "PressedKey": {
        const keyboard = model.keyboard;
        if (keyboard === undefined || model.provider === undefined)
          return stay(model);
        if (message.key.kind === "done") {
          return ask({ ...model, keyboard: undefined, focused: ZONE_STORE_SEARCH_ROW }, { _tag: "SearchStore", providerId: model.provider, query: keyboard.draft.trim() });
        }
        const next = pressFindKey(keyboard, message.key);
        return stay({ ...model, keyboard: next.draft.length > ZONE_STORE_QUERY_MAX ? keyboard : next });
      }
      case "PressedAction":
        return zoneStoreActionLive(store, message.actionId) ? ask({ ...model, dialog: 0 }, { _tag: "RunAction", actionId: message.actionId }) : stay(model);
      case "PressedRow":
        return pressRow(model, message.rowId, store);
    }
  }
  function pressRow(model, rowId, store) {
    const [kind, ...rest] = rowId.split(":");
    const id = rest.join(":");
    switch (kind) {
      case "provider": {
        const listed = store.providers._tag === "Ready" && store.providers.providers.some((item) => item.id === id);
        if (!listed)
          return stay(model);
        const asked = zoneStoreSearch(store, id)._tag !== "Idle";
        return stay({
          ...model,
          page: "results",
          provider: id,
          keyboard: asked ? undefined : ZONE_FIND_START,
          focused: asked ? ZONE_STORE_SEARCH_ROW : undefined
        });
      }
      case ZONE_STORE_SEARCH_ROW: {
        const search = zoneStoreSearch(store, model.provider);
        const draft = search._tag === "Idle" ? "" : search.query;
        return stay({ ...model, keyboard: { ...ZONE_FIND_START, draft, caret: draft.length } });
      }
      case "entry": {
        const search = zoneStoreSearch(store, model.provider);
        if (search._tag !== "Ready" || !search.entries.some((entry) => entry.id === id))
          return stay(model);
        return ask({ ...model, page: "entry", focused: undefined, dialog: 0 }, { _tag: "OpenStoreEntry", entryId: id });
      }
      case "action":
        return zoneStoreActionLive(store, id) ? ask({ ...model, dialog: 0 }, { _tag: "RunAction", actionId: id }) : stay(model);
      case "play": {
        const details = store.details;
        if (details._tag !== "Ready" || details.get._tag !== "InLibrary" || details.get.gameId !== id)
          return stay(model);
        return ask({ ...ZONE_STORE_START }, { _tag: "LaunchGame", gameId: id });
      }
      default:
        return stay(model);
    }
  }
  function back3(model, store) {
    if (!model.open)
      return model;
    if (model.keyboard !== undefined)
      return { ...model, keyboard: undefined, focused: ZONE_STORE_SEARCH_ROW };
    switch (model.page) {
      case "entry":
        return {
          ...model,
          page: "results",
          focused: store.details._tag === "Closed" ? undefined : zoneStoreEntryRow(store.details.entry.id),
          dialog: 0
        };
      case "results":
        return {
          ...model,
          page: "providers",
          focused: model.provider === undefined ? undefined : zoneStoreProviderRow(model.provider)
        };
      case "providers":
        return ZONE_STORE_START;
    }
  }

  // ../../../surfaces/zone/src/state/surface.ts
  var withLook = (state, korri) => ({ ...korri, look: state.look });
  var runnerOpen = (korri) => korri.runnerChoice !== undefined && korri.runnerChoice._tag !== "Closed";
  function zoneShown(state, korri) {
    if (korri.presentation.kind === "gameplay-overlay")
      return "paused";
    if (korri.onboarding !== undefined)
      return state.options.keyboard === undefined ? "onboarding" : "join";
    if (runnerOpen(korri))
      return "runwith";
    if (launchOpen(korri))
      return "launch";
    if (state.store.open && korri.store !== undefined)
      return "store";
    return state.nav.tab;
  }
  var covered = (korri) => korri.status._tag === "Busy" || korri.status._tag === "Problem" || korri.status._tag === "Confirm";
  function zoneSurfaceStart(korri, look) {
    const start = {
      look,
      nav: ZONE_START,
      options: optionsStart(korri),
      runWith: RUN_WITH_START,
      identity: IDENTITY_START,
      status: ZONE_STATUS_START,
      overlay: ZONE_OVERLAY_START,
      onboarding: ZONE_ONBOARDING_START,
      store: ZONE_STORE_START,
      launch: OPTIONS_START,
      launchScope: 0,
      launchAction: undefined
    };
    return update5(start, { _tag: "KorriPublished" }, korri).model;
  }
  var idle = (step7, before) => step7.model === before && step7.requests.length === 0;

  class Steps {
    state;
    requests = [];
    constructor(state) {
      this.state = state;
    }
    put(key2, step7) {
      if (step7.model !== this.state[key2])
        this.state = { ...this.state, [key2]: step7.model };
      this.requests.push(...step7.requests);
      return step7;
    }
    done() {
      return { model: this.state, requests: this.requests };
    }
  }
  function update5(state, message, korri) {
    const steps = new Steps(state);
    route(steps, message, korri);
    return steps.done();
  }
  function route(steps, message, korri) {
    const state = steps.state;
    switch (message._tag) {
      case "KorriPublished":
        return published2(steps, korri);
      case "PressedBack":
        return back4(steps, korri);
      case "PressedMenu":
        return menu(steps, korri);
      case "PressedOptions":
        if (zoneShown(state, korri) === "games" && !covered(korri))
          steps.put("nav", update2(state.nav, message, korri));
        return;
      case "PressedSystem":
        return system(steps, korri);
      case "Games":
        return games(steps, message.message, korri);
      case "Options":
        lookRow2(steps, message.message);
        steps.put("options", update3(state.options, message.message, withLook(steps.state, korri)));
        if (message.message._tag === "FocusedRow")
          steps.put("nav", update2(steps.state.nav, { _tag: "FocusedPage" }, korri));
        return;
      case "RunWith":
        steps.put("runWith", update4(state.runWith, message.message, korri));
        return;
      case "SaveData":
        steps.put("identity", update(state.identity, message.message, korri.identityManagement));
        if (message.message._tag === "FocusedSaveCard" || message.message._tag === "FocusedSecret") {
          steps.put("nav", update2(steps.state.nav, { _tag: "FocusedPage" }, korri));
        }
        return;
      case "Status":
        steps.put("status", updateStatus(state.status, message.message, korri.status));
        return;
      case "Paused":
        if (korri.presentation.kind === "gameplay-overlay") {
          steps.put("overlay", updateOverlay(state.overlay, message.message, korri.presentation));
        }
        return;
      case "Launch": {
        const inner = message.message;
        if ((inner._tag === "SteppedRow" || inner._tag === "PressedRow") && inner.rowId === ZONE_LAUNCH_SCOPE_ROW) {
          const launch = korri.launchSettings;
          if (launch === undefined || launch._tag === "Closed" || optionsLayer(state.launch) !== "none")
            return;
          const count = launchPlan(launch, korri.settingsStatus, state.launchScope).scopes.length;
          const next = launchScopeStep(count, state.launchScope, inner._tag === "SteppedRow" ? inner.direction : 1);
          steps.put("launchScope", { model: next, requests: [] });
          steps.put("launch", { model: { ...state.launch, focused: ZONE_LAUNCH_SCOPE_ROW, at: 0, pick: undefined }, requests: [] });
          return;
        }
        const slices = launchSlices(korri, state.launchScope);
        if (slices !== undefined)
          steps.put("launch", update3(state.launch, message.message, slices));
        if (message.message._tag === "FocusedRow")
          steps.put("launchAction", { model: undefined, requests: [] });
        return;
      }
      case "FocusedLaunchAction":
        steps.put("launchAction", { model: launchOpen(korri) ? message.actionId : undefined, requests: [] });
        return;
      case "LaunchAction": {
        const launch = korri.launchSettings;
        const published2 = launch !== undefined && launch._tag !== "Closed" && launch.actions.some((action) => action.id === message.actionId && action.enabled);
        if (published2)
          steps.put("launch", ask(state.launch, { _tag: "RunAction", actionId: message.actionId }));
        return;
      }
      case "Onboarding":
        return onboarding(steps, message.message, korri);
      case "Store":
        return store(steps, message.message, korri);
    }
  }
  function lookRow2(steps, message) {
    if (optionsLayer(steps.state.options) !== "none")
      return;
    if (message._tag === "PressedRow" && message.rowId === ZONE_SYSTEM_LOOK) {
      steps.put("look", updateLook(steps.state.look, { _tag: "SteppedLook", direction: 1 }));
    } else if (message._tag === "SteppedRow" && message.rowId === ZONE_SYSTEM_LOOK) {
      steps.put("look", updateLook(steps.state.look, { _tag: "SteppedLook", direction: message.direction }));
    }
  }
  function published2(steps, korri) {
    const message = { _tag: "KorriPublished" };
    const state = steps.state;
    steps.put("nav", update2(state.nav, message, korri));
    steps.put("options", update3(state.options, message, withLook(steps.state, korri)));
    steps.put("runWith", update4(state.runWith, message, korri));
    steps.put("identity", update(state.identity, message, korri.identityManagement));
    steps.put("status", updateStatus(state.status, message, korri.status));
    steps.put("overlay", korri.presentation.kind === "gameplay-overlay" ? updateOverlay(state.overlay, message, korri.presentation) : { model: ZONE_OVERLAY_START, requests: [] });
    steps.put("onboarding", korri.onboarding !== undefined ? updateOnboarding(state.onboarding, message, korri) : { model: ZONE_ONBOARDING_START, requests: [] });
    steps.put("store", updateStore(state.store, message, korri));
    const slices = launchSlices(korri, state.launchScope);
    steps.put("launch", slices === undefined ? { model: OPTIONS_START, requests: [] } : update3(state.launch, message, slices));
    if (slices === undefined)
      steps.put("launchScope", { model: 0, requests: [] });
  }
  function back4(steps, korri) {
    const state = steps.state;
    const message = { _tag: "PressedBack" };
    if (korri.status._tag === "Problem") {
      steps.put("status", updateStatus(state.status, message, korri.status));
      return;
    }
    if (korri.status._tag === "Busy" || korri.status._tag === "Confirm")
      return;
    switch (zoneShown(state, korri)) {
      case "paused":
        if (korri.presentation.kind === "gameplay-overlay") {
          steps.put("overlay", updateOverlay(state.overlay, message, korri.presentation));
        }
        return;
      case "join":
        steps.put("options", update3(state.options, message, withLook(steps.state, korri)));
        return;
      case "onboarding":
        steps.put("onboarding", updateOnboarding(state.onboarding, message, korri));
        return;
      case "runwith":
        steps.put("runWith", update4(state.runWith, message, korri));
        return;
      case "launch": {
        const launch = korri.launchSettings;
        const slices = launchSlices(korri, state.launchScope);
        if (launch === undefined || launch._tag === "Closed" || slices === undefined)
          return;
        if (optionsLayer(state.launch) !== "none") {
          steps.put("launch", update3(state.launch, message, slices));
          return;
        }
        const close = launchPlan(launch, korri.settingsStatus, state.launchScope).closeActionId;
        if (close !== undefined)
          steps.put("launch", ask(state.launch, { _tag: "RunAction", actionId: close }));
        return;
      }
      case "store":
        steps.put("store", updateStore(state.store, message, korri));
        return;
      case "games":
        steps.put("nav", update2(state.nav, message, korri));
        return;
      case "options":
        if (!idle(steps.put("options", update3(state.options, message, withLook(steps.state, korri))), state.options))
          return;
        steps.put("nav", update2(state.nav, message, korri));
        return;
      case "savedata":
        if (!idle(steps.put("identity", update(state.identity, message, korri.identityManagement)), state.identity))
          return;
        steps.put("nav", update2(state.nav, message, korri));
        return;
    }
  }
  function identityHolds(identity, korri) {
    const status = korri.identityManagement?.status._tag;
    return status === "Problem" || status === "Switched" || identity.flow._tag !== "idle" && identity.flow._tag !== "showSecret";
  }
  function menu(steps, korri) {
    const state = steps.state;
    if (covered(korri))
      return;
    const message = { _tag: "PressedMenu" };
    switch (zoneShown(state, korri)) {
      case "games":
        steps.put("nav", update2(state.nav, message, korri));
        return;
      case "options":
        switch (optionsLayer(state.options)) {
          case "keyboard":
            steps.put("options", update3(state.options, message, withLook(steps.state, korri)));
            return;
          case "dialog":
            return;
          case "none":
            steps.put("nav", update2(state.nav, message, korri));
            return;
        }
        return;
      case "savedata":
        if (!identityHolds(state.identity, korri))
          steps.put("nav", update2(state.nav, message, korri));
        return;
      case "join":
        steps.put("options", update3(state.options, message, withLook(steps.state, korri)));
        return;
      case "launch": {
        const slices = launchSlices(korri, state.launchScope);
        if (slices !== undefined && optionsLayer(state.launch) === "keyboard") {
          steps.put("launch", update3(state.launch, message, slices));
        }
        return;
      }
      case "paused":
      case "onboarding":
      case "runwith":
      case "store":
        return;
    }
  }
  function system(steps, korri) {
    const state = steps.state;
    if (covered(korri))
      return;
    switch (zoneShown(state, korri)) {
      case "games":
      case "options":
      case "savedata":
        break;
      case "paused":
      case "join":
      case "onboarding":
      case "runwith":
      case "launch":
      case "store":
        return;
    }
    if (state.nav.tab === "savedata" && identityHolds(state.identity, korri))
      return;
    const nav = steps.put("nav", update2(state.nav, { _tag: "PressedSystem" }, korri)).model;
    if (nav.optionsGroup !== "system")
      return;
    steps.put("options", { model: focusSystemGroup(steps.state.options), requests: [] });
    steps.put("nav", update2(nav, { _tag: "LandedOptions" }, korri));
  }
  function games(steps, message, korri) {
    const state = steps.state;
    if (message._tag === "IdleElapsed" && zoneShown(state, korri) !== "games")
      return;
    const nav = steps.put("nav", update2(state.nav, message, korri)).model;
    if (message._tag === "ChoseGetGames" && nav.zone === "get" && zoneShown(steps.state, korri) === "games" && !covered(korri)) {
      store(steps, { _tag: "OpenedStore" }, korri);
    }
  }
  function store(steps, message, korri) {
    const step7 = steps.put("store", updateStore(steps.state.store, message, korri));
    for (const request of step7.requests) {
      if (request._tag === "LaunchGame") {
        steps.put("nav", update2(steps.state.nav, { _tag: "ShowedGame", gameId: request.gameId }, korri));
      }
    }
  }
  function onboarding(steps, message, korri) {
    if (korri.onboarding === undefined)
      return;
    const next = steps.put("onboarding", updateOnboarding(steps.state.onboarding, message, korri)).model;
    if (message._tag === "OnboardingJoin" && next.join === message.networkId) {
      const rowId = zoneNetworkRowId(message.networkId);
      steps.put("options", update3(steps.state.options, { _tag: "PressedRow", rowId }, withLook(steps.state, korri)));
    }
  }

  // ../../../surfaces/zone/src/fixtures/fixture-host.ts
  var HERE = { id: "here", label: "This device" };
  var ZAO = { id: "zao", label: "zao" };
  var fixtureGames = [
    { id: "celeste", title: "Celeste Classic", subtitle: "PICO-8", resumable: true },
    { id: "hollow-knight", title: "Hollow Knight", subtitle: "Switch" },
    { id: "tetris", title: "Tetris", subtitle: "GB", launchLocations: [ZAO] },
    { id: "spelunky", title: "Spelunky", subtitle: "PC", launchLocations: [HERE, ZAO] },
    { id: "lantern-keep", title: "Lantern Keep", subtitle: "PC" },
    { id: "tide-pool", title: "Tide Pool", subtitle: "Mega Drive", launchLocations: [ZAO] },
    { id: "comet-courier", title: "Comet Courier", subtitle: "PC" },
    { id: "bramble-run", title: "Bramble Run", subtitle: "GBA" },
    { id: "petal-quest", title: "Petal Quest", subtitle: "PICO-8" },
    { id: "oracle-of-seasons", title: "The Legend of Zelda: Oracle of Seasons", subtitle: "GBC" },
    { id: "wario-land-4", title: "Wario Land 4", subtitle: "GBA", launchLocations: [HERE, ZAO] },
    { id: "super-metroid", title: "Super Metroid", subtitle: "SNES" }
  ];
  var fixtureModel = {
    presentation: { kind: "catalog" },
    catalog: { _tag: "Ready", games: fixtureGames },
    status: { _tag: "Browsing" },
    clockLabel: "21:47",
    actions: [],
    settings: [],
    settingsStatus: { _tag: "Idle" },
    buildLabel: "korri 0.4.1 · 9a1fb22",
    network: {
      connection: { _tag: "Wifi", ssid: "hummingbird-lake", strengthPercent: 72 },
      wifi: { _tag: "Available", scanning: false, networks: [], attempt: { _tag: "Idle" } }
    }
  };

  // ../../../surfaces/zone/src/fixtures/fixture-store.ts
  var fixtureStoreProviders = [{ id: "@korri:pico8", title: "PICO-8 BBS" }];
  var fixtureStoreEntries = [
    { id: "celeste_classic_2-5", title: "Celeste Classic 2", system: "PICO-8" },
    { id: "lighthouse-1", title: "The Legend of the Supercalifragilisticexpialidocious Lighthouse Keeper", system: "PICO-8" },
    { id: "wip-3", title: "untitled work in progress" }
  ];
  var fixtureStoreManyEntries = [
    ...fixtureStoreEntries,
    ...["Poom", "Dank Tomb", "Picoracer-2048", "Porklike", "Just One Boss", "Mot's Grand Prix", "Lazy Devs Shmup"].map((title, index) => ({ id: `cart-${index}`, title, system: "PICO-8" }))
  ];
  var action = (id, label, enabled = true) => ({ id, label, enabled });
  var fixtureStoreGets = {
    offer: { _tag: "Offer", actions: [action("store:1:get", "Get")] },
    cancelled: { _tag: "Offer", note: "Download cancelled. Nothing was kept.", actions: [action("store:2:get", "Get")] },
    unavailable: {
      _tag: "Unavailable",
      reason: "The provider doesn't say which system this game is for, so it can't be downloaded here."
    },
    confirm: {
      _tag: "Confirm",
      kicker: "Download to this device",
      detail: "Celeste Classic 2, 15.9 KB",
      actions: [action("store:3:download", "Download"), action("store:3:cancel", "Cancel")]
    },
    starting: {
      _tag: "Working",
      kicker: "Starting download…",
      detail: "Asking the provider for the file",
      actions: [action("store:4:cancel", "Cancel")]
    },
    downloading: {
      _tag: "Working",
      kicker: "Downloading Celeste Classic 2",
      detail: "8 KB of 15.9 KB · 8 KB/s · 0:01 left",
      progress: { done: 8000, total: 15872 },
      actions: [action("store:5:cancel", "Cancel")]
    },
    unsized: {
      _tag: "Working",
      kicker: "Downloading Celeste Classic 2",
      detail: "8 KB so far · 4 KB/s",
      progress: { done: 8000 },
      actions: [action("store:6:cancel", "Cancel")]
    },
    queued: {
      _tag: "Working",
      kicker: "Waiting to download",
      detail: "Waiting for the download before it",
      queued: true,
      actions: [action("store:9:cancel", "Cancel")]
    },
    placing: { _tag: "Working", kicker: "Adding to your library…", actions: [] },
    inLibrary: { _tag: "InLibrary", detail: "In your library", gameId: "celeste" },
    failed: {
      _tag: "Problem",
      kicker: "Couldn't start the download",
      reason: "The provider did not answer. Try again.",
      actions: [action("store:7:retry", "Retry")]
    },
    stopped: {
      _tag: "Problem",
      kicker: "Download stopped",
      reason: "Korri restarted before the download finished. Nothing was kept, and your library is unchanged.",
      actions: [action("store:8:retry", "Retry")]
    }
  };
  var fixtureStoreDetails = (get, entry = fixtureStoreEntries[0]) => ({
    _tag: "Ready",
    entry,
    description: "Madeline climbs again, with a grappling hook this time.",
    get
  });
  var fixtureStore = (overrides = {}) => ({
    providers: { _tag: "Ready", providers: fixtureStoreProviders },
    search: { _tag: "Ready", providerId: "@korri:pico8", query: "cel", entries: fixtureStoreEntries },
    details: fixtureStoreDetails(fixtureStoreGets.offer),
    ...overrides
  });
  var fixtureStoreFresh = fixtureStore({ search: { _tag: "Idle" }, details: { _tag: "Closed" } });

  // ../../../surfaces/zone/src/fixtures/sample-art.ts
  var ZONE_ART_SUMMIT = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAIAAAACACAMAAAD04JH5AAAAwFBMVEX19f//+uv/5ni+38VGUG6FP39ePYM0PGBZOn9VOXxTOHtRN3lPNnhONndLNXVKNHRJNHNIM3NHM3JGM3FEMm9DMW9BMW5AMG0/MGwoMFCTID4+L2s9L2o8Lmo6Lmk5LWg4LWc3LGY2LGU1K2UzK2QzKmMyKmIwKWEvKWEuKGAsJ14rJ10pJlwoJlsmJVolJVklJFkkJFgjJFgiI1chI1YgIlUfIlUeIlQeIVQdIVMcIVMbIFIbIFEaIFEYH1AWHk6wIIyMAAADqklEQVR42uWa13LbMBBFLzQ7WafZ6QmcHttxSZzei/P/fxVSECVSooDFLiRMJmdGhkAR9x6z6UX4Uxnol7oyAhcBd2FGF4GLiji3JYHYscHvSrjZiF+Vwc/K4EcS92OT4Htl8K0y+LpZXGoHfNHgyi3C58rgU2Xw0YqzLbcLGMGHyuB9ZfCuMni7Hvd2C+CNHPdmA8Cy2Dm3bxY4N9AKNFgizvHawExgf9+QUUbA4IBXBvoCDaqMYgLaDJwZmAsYMkoInLmxz2QJZzg10AqcGrEKWFYHgRMDjcCJlfoCxwYagWNTQANe2gCMCfUFjvJwwylajizgMBM3mE0FDi0YV8NsgIO1uIMkM4EDA3hhAB2WjPjHTtRvMcBzPT0BQ8gzNeijT3mqZiCgT8nZ2a3t1xvgiRIsE9nXxXLiH5cRiEnhsQ6sogx6pGNEoN3s8oOK9UMXhYcqRgV0SQ80jLRT84qscGuj7isY66fWQJOlFpg29gVonYCLZt3LpyukYX87V4Q5jQDRUIC6DblR7h7uZjOvmxv0NuSn3cmGaMmgvyE7Dab+YEAWA9zOhFYElrZk5lkFaLk/W+BWHpQmL3CrAm5M4GYWJCGa4JbmuJGFSCArEXsBtyeBZOxlgJydicobYHeK25UgFtiVg4x9iTZggOtyMgTkodhIf4ZBfYFrUigPaSyuCqFchLlbE2CjAJHNgHmNAa7IUAj0lzf9PB6MjfX3DZjXGiQEfEmBUQNcjuG9n46ko4uZ9fNYBRL9UwMikwFzxGALAswxA+ysxwd2SE+TwgNWSpDs994kwGoB74sYcMIAgn6LAXPCAJJ+vQGbBZoQgwBz0gDpC1B/CJjTBrg0hl8R8OX6eVCFeP9CwBfrHxog2j/7dYTOgEUGiP7/AwFfrF8sMP+BiEKAWWYAwQHQHAJmoQEi/SsCvlj/wgCR/uVrQG7AagGfEPDFBHhUwK8wrBcaMMsNUgIrFOsfE/C+hAFzjgFy+5MGzFkGEPZPGsLfyYSofbV08/CumzN3Y5aAFwv4UgLcF0jUh1tyMgn3ZTuG2m5O1I7dlm67wKCwAKsFUicgPA7CiQjzxYFfnAqeCoRTILwOIboBhAKcL8AQ3YALgfBkHp6C9l047AsB0QmYC/jKApKnz+A27D0PFiNxX0B8EsoJsFpA+AQOB7/7fg5juPm6MSSGUXgbtgK+soC3oPkGKirgzf1WAW/t//cFvLHfLuBt/QUEuLIAVxbg/12AKwvY+/kvaIQ5eYFu/bAAAAAASUVORK5CYII=";
  var ZONE_ART_KNIGHT = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAFUAAACACAMAAABN2NX0AAAAwFBMVEX/+tLr8P//ylR4blA8WpaqMkaWKDwtOV0kNWIjNGAiM14hMVogL1cgLlYfLVQeLFIeK1AdKk8dKU0cKUweKFocKEwcKEscKEocJ0obJ0kbJkgbJkcaJUYaJUUaJEUaJEQZJEQZI0MZI0IcIjwZIkEYIkAYIT8YIT4XID0XIDwXHzwXHzsWHzsWHjoWHjkWHTkVHTgVHTcVHDYVHDUUGzUUGzQUGjMUGjITGjITGTEWGCgSGC8SFy0RFisQFSkUEB4UVkMFAAADnUlEQVR42u3UiXLTMBAG4E2tatJyg7lvxN0GaHqkIU78/m+FVoclWZIj21ELTP/BOlbarykEA+vIBtPrQAWuSd30OUhQN7HmzTWp6/jZWiX9ILfqnq0Tmtfb1apy9ribTORTVRMeHOWZnu26rnlqFVGrnamVVMtSPtgp10zV5WzquuarFauMWm1VqwR1xcNWeosbNhExXapzZc8JqrDyqSulul1xVf50D13BUkVs5SqsLpdKWG5Vl93qZLJc6m5UzWzqV6AqBi51+K5Z4M2yvLwsSxzlzFdMza26pSoGFjp82yykulhgD5Zk90Kp7bqlKgYudPi2WbBm05y6dy6cNvWIQzHCuQ7fmlWzdhOoY0k94lCMYJ3rhXxr4IyjnM0KR/1g2LmjCgbOmjC9kCrOOMrZrHDUj1B5J2NnQtUMnPpx/wfoilgzOctH109PGf+Dwbs4wNyPq+qKWDuqrs/njN/haCOEVf57zdvdepTnbsVTT/xgz8mJUbFiRnnuVjBcbYSwipqeXTVUD6i//MguPZelXMXrMlxtBPjpx38TYrDuvs3lTd3F1UaAH37CKtZdVd5s2pgRYBYI//7MZu6DsWf56FF2GSCozkarx6Gw43GBo1DY0bjA9xyBbzkCX3Mkk/olR+BzjmRSP+VIJvVjjmRSP+RIJvV9jsC7HIG3OQJvcgRe5wi8Sgmxk3Af+omJMrzsjAXtY6x9VxukmfsmKS68iIYEUefzxlrheSQkhjosCTdDfzSBhWehuP/gnSohgX54GkgQrTFB1gfgiR8SUuvaYls3PAH6oYksPG6HhNS6dtn2pZYBj1ohg1TiIvDQDQmqdd1i/WuOAttQpe7tCRGnsOqw8MAOGaESy0lVt/4NxNXgq5QE1PBFS71vQqLqfsJH5WmkBJV4KumhEpLyYTs+qmHhXhOSwnaiRFOQgBLqvLMo2cqmqdR6udIU9a4K6VSpgsWq66rEUlTaznWpd2TIjlQitASV0j7sv6feFumHdrLI/VPqLZGeKLJFpAG5wWpRFMNVSrvZQmZnqgbb7kg1wiaolCZ/2MJWD0WGqCjtqVgscsPVwlOLRJVetVooFXi0WrTUw52ph1tV+pepBzJ90agqNK0eDFXt70Ch0FzqVKUnGvu+SqxRp4PU9ptgOlalAdRXpz1RjzXoKJV6L8KAOu2t0ggaV2lanDd2UJ0OUO0WA8HvcGhqgt3/gUrpKPZK1ZG5UW/UGzWP+gdxKDWmkoUVTwAAAABJRU5ErkJggg==";
  var ZONE_ART_BLOCKS = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAGAAAACACAMAAADDApyIAAAAwFBMVEX/+ub86J760jzwyHjwx3jvx3fvxnfvxXbuxHbuxHXuw3XtwnTtwXPtwHPsv3LsvnHrvXHrvHDqu2/qum/puW7puG7puG3ot23otmzntWvntGvns2rNv5VQyHjppqjlrmfkrWbjqmThpmHgo1/eoF3cnFvbmVnal1fZlFbYklTXj1LWjFDUiE3ShUzRg0rQgEjPfkfOe0XNeUTMd0LLdUHKcj/JcT7JcD3Jbz3IbzxGlvDIbjzmPFC0WsgoHjweHjLOLbtKAAACLElEQVR42u2YC1PTQBSFD4oPsLWt8hBEERRUQAEFFNDl//8rXe+kzUmy3STc23bifjO7m9ttzzezk9sXHhqDZWPwyBg8NgZPjMFTY7BiDFaNwbMQ7h/xVa6CMfaCXhjn8BeZgXztx6Tu9WS/EvTDsCBf+zGp+33ZrwTPw/gAPztXruUqq7NnVGAvGITxATIDXDs3qQcDObJACIZh/MtkBrh2blIPhyIIhNQReIoCP+RxmcOCURgf42dPvs4LZH80QigIL6YByOzJ19mQx7Pd6ghzwUtjsGYM1o3BhjHYNAavjLEXbBmDbWPw2piC4E6DJGgo2CVUBJSYBHHBO0JFQIkdEOwRKgJKbCF4K5RWNcEvobR2VrBPqAgo8b8URNESlO7/asF7oomgdDQZlJgEccEHQkVAiYssCFJXEH3fv6/gt1BaGwoOCBUBJSbBAggOCZU+oMQGgtp90FZQ+4hmLPhIqAgosQOCT4SKgBIbCGozY8FnQkVAiUkQFxwRKgJKTIK44JhQEVBiEiyA4IRQEVDiPARtfxcsCXFB9K+CAA+EmQu+ECoCSkyCuOArodIHlDhFELv/b4TxOu6DuoLY0RQF4yNiwSmhIqDEDgjOCBUBJc5T0Jq2gh0h3gcsOCemCd4I8SOixA4IvhEqAkrsgOA7odIHlNhAkPVBsR9K34vaCrIjKh5V6SOTBReEioASkyAuuCRUBJTYQFCbGQuujMEPY/DTGFwbg1tj/gDqLZh1gRhpWAAAAABJRU5ErkJggg==";
  var ZONE_ART_CAVERN = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAIAAAAA8CAMAAACZ6EL3AAAAkFBMVEX/+sj/3Fr/1yj/vjzSlm6gWih4UChaOCRaHhQ8JBg7JBg7Ixg6Ixc5Ihc4Ihc4IRY3IRY2IBY1IBU0HxUzHxUzHhQyHhQxHRQwHRMvHBMuGxMtGxIsGhIrGhEqGREpGBEoGBAnFxAmFw8lFg8kFQ8jFQ4iFA4eFA4dEQwcEQwcEAsbEAsaDwsZDwoZDgoYDgqBxl/sAAACyUlEQVR42u2U63LTMBCF5QRsLgFap0lpm6RAW6C58f5vh3VbHa9kSaZh1B85M7GP12dXn62JReP0voQXUPxQwiPAxxIeAWYlfA5A1Ukfq2pmzvJ6Zu5qr6vWu3oK4FNacrQ+9s/2rv3pirt2mYjEZyVg414+ybtOVeV+9lo/Md7j1zoTmS++KEGR+1MAROaLCyUocu/2XEoOraoLs7APoO9LZJsPzQRfHuBSCYq+91+9HDsMoLfisnFbEJkv2rRCAG0rz/JuGMD1JYbjd6ANevdK5dO0bEtsIgThtiAyHwHm6jjtZL0PMH8BwNzMdmsxgCu7fpe6grrec3nWGb0V7txM5Xna6Br+bKeZb2bTWhxgoQGE6EKLAYDFCwAWZjatxQGWZn2ZWrJ6zNueVB5zVBfLvghghHJ7gjlxrRSkbJpr4B/2riee529K1cVXpf57ogzUY556EvnebFMPALj/Sj6A6UkCwGwLcKPU9Amsx3rM255UXuZY3Qe4qetmLEBjepL5LscBbj3V9e1o5fb4OfwO3BEleazHPLy1aJ5yd6EPUXGAFQxb+fXmrVOvzgFWw55yq1cJsIZha7/uAawHANbDnnJUFxtPdb0JCwCye5I5AwCcG3iaXt17A5AxPRte577Lsbq4V4JixL+xysznePFNCYoRTwCZ+RxfHuC7EhQjngAy8zneAOTJAZxO+B34kfIEkJnP8QjwkPIM4GFM75B/VQCPKc8AHsf0DvnyAE8jRABPp5P4qQRsEU8AmfkcXx7gV2GdAc4AZwD8FP8u4RHgWR0nkwl5V1fV7vDs5cnrRlanYmCmBthydS3bbbjqDqGOYGN4GkjslIAt5eUz7eDxdhP3lLsGvbuIzBR7JSiO8GqBPbzlPQDsAQA3gs35ZwCamZOf9CD7AAclCByg6wBh5qkjknE+NF97cVSCwBEAjhD+T178Kay/As27EeaLmSwAAAAASUVORK5CYII=";
  var ZONE_ART_TOWER = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEgAAACACAMAAACfiN+sAAAAwFBMVEX/+uv/9dL/+sj/zlNQUGQ8PFAePCh4MkZaMnhYMXZXMXVWMHRUL3NTL3FSLnBPLW5OLWxNLGxLK2pIKmdGKWVEKGNDKGNBJ2A/Jl8+JV09JVw7JFs6I1k3Ilc2IlU0IVQyIFIwH1AtHk4qHUsoHUAmHDk1F0AnG0glGkYkGzsiGUMhGUIjGjQhGTIgGEEeFz8dFj4bFjwZFTsgGDAeFy0dFisXFDgbFSgVEzcTEjQRETMQETEPEDENDy8MDy4LDi0gVLfVAAAC4UlEQVR42u2VaXuTQBSFT1jEXVNrXKutC/q41rXu/f//qjCEhOWcOwyZpx/UN0AY5szb+8AtwekAnM4DO85vg388oHP053bDOfA7EvgViXiinxRx2QABCTOLH5HA9xksagbXMFMzUmG+p2/Ct0AWXTrXsYuna9IiBIq+BrEYspkBOZsn+sjgVytGIv+SmSLsLvqgYXMDjx220KLjmuY4AL2TdtTzsPRkuOcY7yeyDW48/Xmw7IBiMB5rqsXv/BRF4Q9hkmeCCa+9OJE3hVc+igZfDBM9zITuYry0KbbYwfMSFcVUE15YFH2sKAI8pgnG1WKM/sMIKcgqCU8lBUOmEebRJgR6pCme6Amn0PAFCPYIk6qoImPINI4kVNRMkVXQQ0N05BWxitIav+hQga5nY9JxOXOYOypHkiTVsRlq0WNJW0+SNCY3lmm/KEnWJo/okYSKZDquCJaof4+06ECSdUyb5y/TOIAtGvSRFj2U5Pmmk9ouynOZNkT0f02LHkioqB8BPZ0h6jrvS6hIpuOJ7kmoSKZxV0JFMm2IcsYcUWBFdyRUJNPBIrUAq1W1UahopYCcCRXdllCRTGO/S29ERfsKyJl92kdyQbyKcEtBRTKtPXFEab1Xb/z6pd98t7sU7VHSst7dpzKV1XdZrsd8wZ4lSpulWb25yjI3DheVHVFZ72lmiJaUqpplK8rrrfpla8d8hRAtq3uStR+3teOlEt2U0Mcv07ghoSKZNkVplva6SIrgE7mbu+0is6LrknXnNB1UV1SLZNoWuc5pu6i0RdckedM5dQe5LnLvI5nGVUmWNZ1Trnf31GTaFo05B9EVCRXJNC5LqEimcUlCRTIdT3RRQn/XZNoQ0Yq06JmEimQazyVUJNN/s+iNhIpk2hDRPpojilZRoOithIpkOp7ok4SKZDqe6LOEimTaENE+0qIvElqRTMcTnUioSKZxwdEMxufN6s75icrHFu3Of9E/LToDc3jecgDiTKUAAAAASUVORK5CYII=";
  var ZONE_ART_SHORE = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAIAAAABgCAMAAADipIp7AAAAwFBMVEX////6+v7/8Jb53Mn128ry28vu2s3q2s7m2dDi2NH01pXz1JPy0pHw0I7vzoztzIne2NLZ19TU1tbP1dnJ1dvE1N2/09+60uC30uOy0eSu0Oapz+ilz+mhz+qczuyXze6RzPHryYboxYLnw3/lwXzkv3rivHjhu3bfuXPet3HdtW+My/KFyvWAyfd8yfl4yPooi8fwUDzIPDImhcIkf70je7ohdrYgcrIda60bZqgaY6YYXqIeWqAXWJ0VU5kUUJYlP7U8AAACnklEQVR42s2UiXLUMAyG/4S73Mt9LbRcBcqRcNMsvP9bEcVxHDm2Ey3jiG+mlm1p/H+zmSk+KoMPyugLvFcGJynQwev43p/jE7SezPGfC9ATTYuttJqHm8bcu6lxz1U3Ewt4l8ZE2WoetSe6d1PjnqtuJhawQMBVFwTwKcL2+N9cwLM09ISrYwE+FReYC9hTgD4An3KfSCbwNA094SoX4FMxgbmARQLmJ7bVfAIgpEm3pm8r0hHYpqGnaSVstQ/7U6a/3Zq+rUhHzAnQM2YlppVPmXu/Nk1JRASezNE0ZiWmlU+Ze7/2AmUZen5e4N8oOQGBx1kpfSYTeJSTcoo/gpXzJwZ4mJGggDeDB/kow/AhfYH72ShjsCl9gXvZiAqwKdzNRlSATekL3MlGVIBN6Qvczkcknw/pC9zKSDDfm9EX2OQkkO+P5BXYzOZvcDMvXv50ILcAUwi1cSM/fXy4uYZAElxSBheV0Re4oAzOK6MvcE4ZnFUGZ5TRF/ikjL7AZ2X0Bb4og6/K6At8UwbfldEX+KGMvsBPZfBLGZwSheHUsuIZ3XG4H5d1ztjtip2j2K19BjvTzcpnFL85a59R/OGsfUZxwFn7jANlcFkZXFFGKlBVVb9r/5MNt27v+ksFrkqoDN2+DR3u7X7cXwiuSegDun0bOtzb/bi/EOyRbxICAqy/VOC6gCGADm3ocG73k/5CcChgCKC9J1B5/aWIBA6H96sQo35mgTlEAkcy2uePEtFdX4RUoJeICojBczHJX1/8mr7ACzFJAfFreCknkS9/TF/g1R7UXVg9zd/jLbyWU9eUbVYL7duO/DEcy6nrdql4fndnVhnYJ78PGwkYqeN9BN5IaZOHde52AXgrpa7dyq7rYZUAeX4kKCI2x19SBLr8fkNuDwAAAABJRU5ErkJggg==";
  var ZONE_ART_COMET = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAIAAAABICAMAAAAJWw0gAAAAwFBMVEX5/f//+uv0+v/q9//w3Krg8//m5vDS7f+95P+M8P+f2P/dqX/IbjwoFFooEEUnEEUmD0QlD0IkD0EjD0AjDj8iDj4hDj0gDjwgDTsfDTsfDToeDTkeDTgdDTgdDDccDDccDDYbDDUaDDQaCzMZCzIYCzEXCzAWCy8WCi4VCi0UCiwUCisTCisTCSoSCSoSCSkRCSgRCScQCScQCCYPCCUOCCQNCCMNByIMByELByAKBx8KBx4JBh4JBh0IBh0IBhzQ/q3vAAADAklEQVR42s2TC1ubMBSGz1ktLW06N+281NvWbl7qXatuc3P//18th0CTQAIJpMX3ecIhyYHvNUX41zLwVh98CwD8bRn40zLwWgt8DQX8bhn4tQ7QvgUevSvxgJ/rA7WbdAaG/bUCL43Al6bAcxPwuTHw3DLw5AU+hQaeWgYWBXAxWKwPg8BiwPF/E9YUeDQwSHl0Bx9rAg/4YCBTeAiJMQnuLWQK93awZBnvHYE7C4MldyvFKIC6QU0FrFxIBG5NYHJVFG5XBtyUIQ36/b6xA28aQA/DdTkyn3O9AuCqCpnPuQoOXFajCPT7l4GBuQ7OTSgC/V6vNw8IXCjgRYF0SY0nLoIBbm2YEwinAOeu6Pndbvc8CODRKwW6giACZz7oAp1O56wxcOqHzO8QH4jTJtgE0EmApwPRROCHL6mAmg/+b1lS59HEQMt3MxhKFIHvlWBxKRMAScVLKJVJaCo2HASMkEGlAMp4VkQowEwHS6cqOQF748wYLxRmM5jWRxMwt+B0ao0XCk0EptUCPJ+VA9+aIA1sHVX5DQWWBnIFtfjKfAZfPYgMa0m+pb86ngucuBJxTrxwyWdwbIDxgdpKlHDsh5vAkQHGtGmUQRM8csUpn8GhAdrI7iOFQx/c8hkcGEh26CbSOPDBMZ/BRAfpku5FOSYGcGKhrsBkKcCrg0DTfAb7BmgjvVXy931wF9gzwJgyWQrslYH61Dmfwa4Bpk9TgV0P8gIbJoTAjhHMzXn+jgfD0mANcH2nv0Dhj2WmY4Evq2BYGqx/A9qDGCbfMVsIjAkch8MnfSkgiTmyxrG52vvHG3wMRR9jtqoKbOtQg6zigWK1929wgW0vgS2VON7aGnGoxjFVguooQaxStfXrgyLFvnherr8jgc8qtJlVfcSx3lHdz1gmII6e0uT6OxL4pELtWc0PuePWn0XKH0LU/EfYusCmDh1SdtWPlFZoZzSy99M/Yf6n0P8Nix9hUIHNGgIf89BhZVeq2VBX7P3ibph8aGKIw5c1/xEqr8I2BP4DFJ6ivLIcTxAAAAAASUVORK5CYII=";
  var ZONE_ART_BRAMBLE = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAIAAAABACAMAAADlCI9NAAAAjVBMVEX///D/8LT63L7/zXb/y3b+yXX+xnT+xHP+wnP+wHL+vXH+u3D+uXD9tm/9tG79sm39r2z9rWz9q2v9qGr9pmn8pGn8omj8n2f8nWb8m2b8mGX8lmT8lGP7kWL7j2L7jWH7imD7iF/7hl/7hF77gV36f1z6fVz6elv6eFo8jPCWRlqWMjxQKEYePDIoHigONYjdAAACXUlEQVR42u2TCZPTMAyFX1pgl3u5b1pYriZx///PI6nq2pYlJ1GnGGb4ZrSOY+m9lzSLX5XBz8rgR2XwvTL4JoEDc9a0P5/3BdloOLqVIIE5a9qfz/uCbDQcfZUAuk6ukfSe7x8pz8tWXySWBPD9I+V52WorMdpst2ToVx6E7ndd6Jfm05KsNhKjwGZDRn7lAeh+14V+aT4tyeqzBA3wlRfd9/10Jc2Hkqw+SdAAX3nRfd9PV9J8KMnqo4T0sckf44jvj+epj07CnGT1QWJZAN8fz1MfnYQ5yeq9DP1bjatW/jxdwzwVncR7Bt7JkOC4auXP0zXMU9FJvGfgbWXwpjJ4fQ4NcY4EXtlpAnYRvLTSpFhl8MJIwzHq4LmNJscmhGcmGgmTEm5MiAFMSnhqoZGxSOGJBSWARQqPDTQaBi08MqAGMGjhoQE1gEELDwyoAQxauG9ADWDQwnWRNr7O4QGul4MrheDCtqUIRR2RK9yTaBcQ+Z8GFwjhbkRrw/snN+fq4c6BthpYr9dtTdBWZkYA9+8E4J2rAXOAnjgnwOpAH5F19kqAeMg555unA/SZf5rgJHq6guYbB+hVXPxcx84ZASJNTNjPCXA0ZZ1uUQDX6h6FAMzSFID/JkKvnqAYwBU+AR6AeThTANY5bHa73WpK82IBHAXoLQFKz1UIl+0Sf00T8stLe5PDygH0t8M6CwGSTkw8SSlAepbvogC6JsTspefSzrLd4O/ODuCc4HLhAMlufoC0k/xPCQqd2F+Gwfz4ZwJc0H8/J8LF3gBL8scDZEmqBdj/D/C3B/gNRXbprfGE280AAAAASUVORK5CYII=";

  // ../../../surfaces/zone/src/fixtures/zone-games.ts
  var ART = {
    celeste: ZONE_ART_SUMMIT,
    "hollow-knight": ZONE_ART_KNIGHT,
    tetris: ZONE_ART_BLOCKS,
    spelunky: ZONE_ART_CAVERN,
    "lantern-keep": ZONE_ART_TOWER,
    "tide-pool": ZONE_ART_SHORE,
    "comet-courier": ZONE_ART_COMET,
    "bramble-run": ZONE_ART_BRAMBLE
  };
  var FIXTURE_LAST_PLAYED = 1792411200000;
  var FACTS = {
    celeste: { lastPlayedAt: FIXTURE_LAST_PLAYED, playCount: 23, totalPlaytimeSeconds: 41460 },
    "hollow-knight": { lastPlayedAt: FIXTURE_LAST_PLAYED - 86400000 * 9, playCount: 7, totalPlaytimeSeconds: 11520 },
    spelunky: { lastPlayedAt: FIXTURE_LAST_PLAYED - 86400000 * 2, playCount: 1, totalPlaytimeSeconds: 2460 }
  };
  var fixtureLibrary = fixtureGames.map((game) => ({
    ...game,
    ...ART[game.id] === undefined ? {} : { coverArtUrl: ART[game.id] },
    ...FACTS[game.id]
  }));
  var gamesModel = (catalog) => ({ ...fixtureModel, catalog });
  var fixtureReadyModel = gamesModel({ _tag: "Ready", games: fixtureLibrary });
  var fixtureLoadingModel = gamesModel({ _tag: "Loading" });
  var fixtureErrorModel = gamesModel({
    _tag: "Error",
    message: "The library could not be read. The disk did not answer."
  });
  var fixtureEmptyModel = gamesModel({ _tag: "Empty" });
  var byId = (id) => {
    const found = fixtureLibrary.find((game) => game.id === id);
    if (found === undefined)
      throw new Error(`no fixture game ${id}`);
    return found;
  };
  var fixtureSectionedGames = [
    { ...byId("celeste"), section: "Continue" },
    ...["hollow-knight", "spelunky", "lantern-keep", "comet-courier", "bramble-run", "wario-land-4"].map((id) => ({ ...byId(id), section: "This device" })),
    ...["tetris", "tide-pool", "oracle-of-seasons"].map((id) => ({ ...byId(id), section: "zao" })),
    ...["petal-quest", "super-metroid"].map((id) => ({ ...byId(id), section: "Supercalifragilisticexpialidocious-handheld" }))
  ];
  var fixtureSectionsModel = gamesModel({ _tag: "Ready", games: fixtureSectionedGames });
  var fixtureOneGameModel = gamesModel({ _tag: "Ready", games: [byId("hollow-knight")] });
  var fixtureWideArtGames = fixtureLibrary.map((game) => game.id === "wario-land-4" ? { ...game, coverArtUrl: ZONE_ART_BRAMBLE } : game);
  var fixtureWideArtModel = gamesModel({ _tag: "Ready", games: fixtureWideArtGames });
  var fixtureWideArtIndex = fixtureWideArtGames.findIndex((game) => game.id === "wario-land-4");
  var fixtureFortyGames = Array.from({ length: 40 }, (_, index) => {
    const base = fixtureLibrary[index % fixtureLibrary.length];
    return index < fixtureLibrary.length ? base : { ...base, id: `${base.id}-${index}`, title: `${base.title} ${index + 1}` };
  });
  var fixtureFortyModel = gamesModel({ _tag: "Ready", games: fixtureFortyGames });
  var fixtureDownloadGames = fixtureLibrary.map((game) => game.id === "hollow-knight" ? { ...game, download: { bytes: 1500000000 } } : game.id === "spelunky" ? { ...game, download: {} } : game);
  var fixtureDownloadModel = gamesModel({ _tag: "Ready", games: fixtureDownloadGames });
  var fixtureGetGamesModel = { ...fixtureReadyModel, store: fixtureStore() };
  var fixtureGetGamesEmptyModel = { ...fixtureEmptyModel, store: fixtureStore() };

  // harness.ts
  function summarise(name, samples) {
    const sorted = [...samples].sort((a, b) => a - b);
    const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];
    const mean = samples.reduce((a, b) => a + b, 0) / samples.length;
    return { name, n: samples.length, p50: at(0.5), p95: at(0.95), p99: at(0.99), max: sorted[sorted.length - 1], mean };
  }
  function library(count) {
    return Array.from({ length: count }, (_, index) => {
      const base = fixtureLibrary[index % fixtureLibrary.length];
      return { ...base, id: `${base.id}-${index}`, title: `${base.title} ${index}` };
    });
  }
  function navigationScript(cards) {
    const script = [];
    for (let index = 0;index < cards; index += 1)
      script.push({ _tag: "Games", message: { _tag: "FocusedCard", index } });
    script.push({ _tag: "Games", message: { _tag: "ChoseCard", index: Math.floor(cards / 2) } });
    script.push({ _tag: "Games", message: { _tag: "FocusedAction", index: 0 } });
    script.push({ _tag: "PressedBack" });
    script.push({ _tag: "PressedMenu" });
    script.push({ _tag: "PressedBack" });
    return script;
  }
  function timeInputs(name, korri, steps, marshal) {
    let state = zoneSurfaceStart(korri, "modern");
    const games2 = korri.catalog._tag === "Ready" ? korri.catalog.games.length : 1;
    const script = navigationScript(Math.min(games2, 40));
    const samples = [];
    let sink = 0;
    for (let step7 = 0;step7 < steps; step7 += 1) {
      const message = script[step7 % script.length];
      const start = __now_us();
      const next = update5(state, message, korri);
      state = next.model;
      const shown = zoneShown(state, korri);
      if (marshal)
        sink += JSON.stringify({ shown, nav: state.nav, requests: next.requests }).length;
      else
        sink += shown.length + next.requests.length;
      samples.push(__now_us() - start);
    }
    if (sink < 0)
      throw new Error("unreachable");
    return summarise(name, samples);
  }
  function timePublish(name, korri, steps) {
    let state = zoneSurfaceStart(korri, "modern");
    const samples = [];
    for (let step7 = 0;step7 < steps; step7 += 1) {
      const start = __now_us();
      state = update5(state, { _tag: "KorriPublished" }, korri).model;
      samples.push(__now_us() - start);
    }
    return summarise(name, samples);
  }
  function timeStart(name, korri, steps) {
    const samples = [];
    for (let step7 = 0;step7 < steps; step7 += 1) {
      const start = __now_us();
      zoneSurfaceStart(korri, "modern");
      samples.push(__now_us() - start);
    }
    return summarise(name, samples);
  }
  function timeModelIn(name, korri, steps) {
    const text = JSON.stringify(korri);
    const samples = [];
    let sink = 0;
    for (let step7 = 0;step7 < steps; step7 += 1) {
      const start = __now_us();
      const parsed = JSON.parse(text);
      sink += parsed.catalog._tag.length;
      samples.push(__now_us() - start);
    }
    if (sink < 0)
      throw new Error("unreachable");
    return summarise(name, samples);
  }
  globalThis.runBench = (steps) => {
    const big = gamesModel({ _tag: "Ready", games: library(500) });
    const results = [
      timeStart("start (fixture, 7 games)", fixtureReadyModel, 50),
      timeInputs("input (fixture, 7 games)", fixtureReadyModel, steps, false),
      timeInputs("input + JSON out (fixture)", fixtureReadyModel, steps, true),
      timeInputs("input (500 games)", big, steps, false),
      timePublish("KorriPublished (fixture)", fixtureReadyModel, Math.floor(steps / 4)),
      timePublish("KorriPublished (500 games)", big, Math.floor(steps / 4)),
      timeModelIn("model JSON.parse (fixture)", fixtureReadyModel, Math.floor(steps / 4)),
      timeModelIn("model JSON.parse (500 games)", big, Math.floor(steps / 4))
    ];
    const bigModelText = JSON.stringify(big);
    return JSON.stringify({ results, bigModelText, modelBytes: { fixture: JSON.stringify(fixtureReadyModel).length, big: bigModelText.length } });
  };
})();
