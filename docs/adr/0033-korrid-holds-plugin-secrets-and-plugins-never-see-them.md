---
status: proposed (ready for review)
---

# korrid holds plugin secrets, and plugins never see them

**Owner request, 2026-10-23, verbatim:** "We probably want to think through
this one deeper. We're basically saying, we're asking what external inputs may
a plugin need. And for this one it's an API key, but for another plugin it may
be something else. Then there's the question of how do we store it in a secure
way. Maybe that's already covered by the current design."

**Owner decisions, 2026-10-23:**

- Per-plugin state lives "In korrid's private folder, one subfolder per
  plugin, named by the plugin's id".
- A plugin secret is the person's. It stays on the one device where the
  person entered it and never moves. At an identity switch it follows the
  person's play-log choice (section 2, "Owner and devices").

**Coordinator decision, 2026-10-23:** the names keep their main and legacy
roots, with the SteamGridDB part removed (section 2, "Names").

The answer: the current design already covers four of the five kinds of input
that real plugins need. One kind has a real case and no home. That kind is a
secret that korrid uses for a plugin, such as the SteamGridDB API key
([ADR 0032](0032-steamgriddb-artwork-comes-from-a-plugin.md)). This ADR gives
that kind a home and adds nothing for the other four.

No open owner questions.

## What exists today

Read on main at `a2151a47e` unless a line says otherwise. Plugin repositories
were read at `korri-plugins` `origin/main` `adaccd5`,
`simonwjackson/korri-plugins` `51e8864` and `korri-plugins-private` `bb849fc`.

### Values and choices: launch settings

- A runner describes its own settings. `settings.describe` returns the
  runner's schema fragment, and `settings.validate` reports what the build
  cannot apply (`services/korrid/SCRIPTING.md` :237–243,
  `launcher/typed_settings.rs` :1–12, :67–80). korrid keeps no key table.
- The values live in `device.yaml` and `catalog/library.yaml`, in the one
  cascade of [ADR 0028](0028-one-launch-settings-cascade-for-every-kind.md)
  (:22–37). The plugin's JS receives them in `settings.validate` and
  `launch.prepare`. A person edits them in the portal's launch-settings
  editor (`settings-model.ts` :140, ADR 0028 :44). Hooks, env and args are
  file-only ([ADR 0029](0029-hooks-env-and-args-run-as-the-game-and-live-in-files.md) :16).
- Sync is by kind, shared or device-only
  ([ADR 0018](0018-launch-settings-sync-by-kind-not-by-layer.md) :15–18).
  Sync is not built.
- A plugin that is not a runner or modifier has no settings of its own.
  `PluginSetting` is `{ id, title, enabled }` (`lib.rs` :925–929,
  `contracts/generated/korrid.ts` :1130). It carries no value.

### Secrets

| Secret | Where | Owner and mode | Who reads it |
|---|---|---|---|
| SteamGridDB API key | `<private>/steamgriddb.credential` (`config/settings.rs` :29, :242–244) | User `korrid`. The write sets no mode (`settings.rs` :538–541). The file is 0600 only because the unit has `UMask=0077` (`nixos-module.nix` :852) and the folder is 0700 (:496, :821). | korrid only. It adds the key as a Bearer header (`enrichment/steamgriddb.rs` :612, :630). |
| Device key | `<private>/identity/device.key` | Folder 0700 and file 0600, set in code (`identity.rs` :1093, :1199, :1220). `.gitleaks.toml` :25–28 refuses the path in commits. | korrid only. |
| Person key | local signer's own state root | 0600, set in code (`local_signer.rs` :1423, :1428). | The local signer only, never korrid (CONTEXT.md, "Person key"). |
| Tailscale auth key (fixture only) | systemd credstore | systemd owns it. The plugin host admits exactly one name, `LoadCredential=authkey:tailscale-authkey` (`plugin-host/src/native_unit.rs` :420–431, `README.md` :230). | The native service, through systemd. "Korri reads no secret, creates no credential and supplies no fallback." |

Other facts:

- `<private>` is `KORRID_PRIVATE_STATE_ROOT`, `/var/lib/korrid` on devices
  (`nixos-module.nix` :216–219, :758). Game units cannot see it: it is in
  their `InaccessiblePaths` (`host/systemd_unit.rs` :1116–1118). Plugin
  native services run with `DynamicUser` and `ProtectSystem=strict`
  (`plugin-host/src/unit.rs` :583). A plugin approved for `User=root`, such
  as `@korri:ssh`, can read everything (`plugin-host/README.md` :226).
- No secret is encrypted at rest. main has no TPM, keyring or
  `systemd-creds` code (`grep` over `services/korrid/src`). NIP-44 encrypts
  only data in transit, such as peer RPC (`peer_rpc.rs` :237).
- The SteamGridDB key is write-only. The snapshot carries only
  `SecretSettingStatus` (`settings.rs` :33–36, :148–167). Tests check that no
  RPC returns the key or writes it to readable config
  (`settings_secrets_tests.rs` :53, :103). The portal row is `sensitiveText`
  (`contracts/surface/korri-surface.ts` :163–168, `settings-model.ts`
  :141–160).
- The set and clear RPCs are `OwnerDeviceOnly` (`authorization.rs`
  :246–247). That admits the local portal, the local control socket and the
  person's other devices (:269–281). In host mode both RPCs answer
  `OperationUnsupported` (`lib.rs` :3459–3464, :3490–3495). Every device runs
  host mode (ADR 0017), so on a device nobody can set the key today.
- An identity switch moves or deletes only the play log
  (`identity_switch/coordinator.rs` :350–360). The SteamGridDB key stays on
  the device for the next owner.
- A plugin's JS never receives a secret. ADR 0016 turns refuse the
  `authorization`, `proxy-authorization`, `cookie`, `set-cookie` and `host`
  headers (`acquisition/turns.rs` :154–160).
- The approval report has `hosts` and `warning` (`plugin-host/src/package.rs`
  :31–54). The portal's `PluginInspection` carries `warning` but not `hosts`
  (`plugin_admin.rs` :127–139, `plugin-host/src/admin.rs` :263). The
  portal's approval text shows the warning, ports, background units and
  dependencies, and no hosts (`plugin-repositories.ts` :287–310). The native
  credential request already adds a sentence to `warning`
  (`package.rs` :1003–1008).
- Ordinary plugin removal keeps "state directories, saves, settings,
  credentials, or identity data" (`plugin-host/README.md` :181).

## Inputs that current and planned plugins need

"Uses it" names the code that needs the value itself.

| Plugin | Input | Kind | Uses it | Where it lives today |
|---|---|---|---|---|
| SteamGridDB (in korrid, ADR 0032 moves it) | API key | secret | korrid, as `Authorization: Bearer` | `<private>/steamgriddb.credential` |
| legacy itch.io (out of scope: ADR 0016, "skip itchio") | API key | secret | the plugin's JS (legacy `itchio/src/definition.ts` :569–571, Bearer at :607) and the native `butler` daemon (`Profile.LoginWithAPIKey`, :2085–2086) | `process.env.ITCHIO_API_KEY`. Legacy `credentials.get` read any variable (`acquisition/plugin-runtime.ts` :73–81). |
| `@korri:tailscale` | tailnet sign-in | third-party account login | native `tailscaled` | An operator runs `tailscale up` (`tailscale/plugin.ts` :2). Session in the plugin's `StateDirectory`. |
| core Tailscale fixture (`plugin-host/vm-test.nix` :169) | auth key | secret | native service | systemd credstore, through the one admitted `LoadCredential` name. The official plugin requests none. |
| `@korri:steam` | Steam sign-in | third-party account login | the Steam client | Its own sign-in window (`steam/README.md` :27–28, :41–43). Session in `${accountRoot}/steam` (:5). |
| `@korri:retroarch`, `@korri:libretro` | RetroAchievements user name | non-secret value | the plugin's JS writes it into RetroArch config | launch settings (`retroarch/render-settings.ts` :435, legacy `retroarch/src/policy.ts` :417) |
| `@korri:retroarch`, `@korri:libretro` | RetroAchievements password or token, netplay passwords | secret | native RetroArch reads it from its config | Nowhere. Both plugins refuse these keys (`libretro/retroarch.ts` :76–85, legacy `setting-policy.ts` :3–9). |
| runners and modifiers (`libretro`, `retroarch`, `gamescope`, …) | runner settings, modifier on or off | non-secret value, choice | the plugin's JS | launch settings (ADR 0028) |
| owned-game plugins (`fallout1-ce`, `fallout2-ce`, `2ship`, `opengoal`, `melee-pc`, `fable-ii-recomp`, `the-simpsons-game`, …) | the person's own ROM, disc image or game data | file | korrid checks the declared hash, the runner gets `contentPath` | A source folder, or the declared account-relative `file` (`fallout2-ce/README.md` :53–56). A mismatch goes to the attention list. |
| `actraiser` (private) | the person's own ROM, at build time | file | the Nix build machine (`actraiser/README.md` :31–35) | Outside Korri. No runtime input. |
| `@korri:ssh` | the person's SSH public key | non-secret value | native `sshd` | Device-owned `authorized_keys` files. Enrollment does not exist (`ssh/README.md` :8–9, :26–29). |
| `@korri:sunshine` | Moonlight client trust | none from the person | korrid provisions peer certificates (`host/moonlight_certificate.rs` :24–30) | `/home/korri/.config/sunshine` (`sunshine/README.md` :11) |
| `@korri:pico8`, `homebrewhub`, `retrobrews` | none | | | Public hosts only. |

No plugin needs a URL or a path from the person. Paths come from korrid
(`contentPath`, `accountRoot`, artifact folders). The person picks source
folders for korrid's discovery, not for a plugin. URLs come from the plugin
or from a response.

## Decision

### 1. Five kinds of input, one of them new

| Kind | Real cases | Mechanism | New? |
|---|---|---|---|
| Value or choice the plugin's JS reads | runner settings, RetroAchievements user name | launch settings: `settings.describe` schema, values in `device.yaml` and `catalog/library.yaml`, portal editor, sync by kind | No |
| File the person owns | ROMs, disc images, game data | discovery sources and a declared game's `file` and hashes, `contentPath` | No |
| Third-party account login | Steam, Tailscale | the native program's own sign-in and state. Korri never sees the password. | No |
| Secret that a native program reads | Tailscale auth key (fixture), RetroAchievements token (refused today) | systemd `LoadCredential` and credstore. Korri supplies no value. | No |
| Secret that korrid uses for the plugin | SteamGridDB (and legacy itch.io) API key | plugin secret (section 2) | **Yes** |

No other kind gets a mechanism. A new kind needs its own decision and a real
case.

### 2. Plugin secret

**Who reads it.** korrid only. The plugin's JS never receives it, in input,
in a response or in a failure. korrid adds it to a request after the request
passes admission. Native programs do not receive it (section 3).

**Declaration.** The plugin's `plugin.nix` declares that it uses one secret
and names the one host that receives it. The host must also be in `hosts`.
The builder carries it into `manifest.json`, so the approval digest binds it,
as it does `hosts` (ADR 0016). The field is `credentialHost`, for example
`credentialHost = "www.steamgriddb.com"`. The legacy baseline is
`credentialRequired: boolean`
(`legacy:product/platform/protocol/acquisition/plugin.ts` :19). The delta is
the host, and the rename follows it: a field named "Required" that holds a
host reads like a yes or no. A boolean would send the key to every declared host, and SteamGridDB
must not send it to its image host (legacy `tools/lab/vite.config.mjs`
proxies the image host with no key). One secret per plugin: every real case
has one.

**How korrid applies it.** korrid adds `Authorization: Bearer <secret>` to a
turn request whose host is the declared host. On a redirect, it adds the
header again only if the next host is the same. The plugin still cannot set
`authorization`. Bearer is the only form, because both real cases use it
(SteamGridDB OpenAPI `securitySchemes`, legacy itch.io :607). A query
parameter or another header waits for a real case. No log, diagnostic or
failure text contains the secret.

**No secret, no call.** If the secret is not set, korrid does not call the
plugin's handler that needs it. korrid reports a diagnostic, as
`SteamGridDbCredentialMissing` does today.

**Storage.**

- Folder: `<private>/<plugin id>/`, the owner's per-plugin folder. The plugin
  id is used as written, such as `@korri:steamgriddb`. `validate_id` allows
  only `@`, `:`, lowercase letters, digits, `-`, `_` and `.`, and a name
  cannot start with `.` (`plugin-host/src/declaration.rs` :101–117). So an id
  is always one safe path part. korrid checks the id again before it builds
  the path.
- File: `<private>/<plugin id>/credential`. The name is main's
  `steamgriddb.credential` without the SteamGridDB part.
- Owner `korrid`. korrid creates the folder with mode 0700 and the file with
  mode 0600, in code, as `identity.rs` does. Delta from today: the
  SteamGridDB write relies on the unit's `UMask`. A korrid that runs outside
  the NixOS unit, such as in development, gets the shell's umask, often 022,
  and the file becomes 0644.
- The write stays atomic: a new temporary file, `fsync`, rename, folder
  `fsync` (`settings.rs` :526–556).
- At rest the file is plain text, like `device.key`. An encryption key on the
  same disk protects nothing from a reader of `/var/lib/korrid`. TPM sealing
  (`systemd-creds`) is a separate decision. I did not check which devices
  have a TPM.
- The secret never goes into `device.yaml`, `catalog/library.yaml`, a
  record, a log or an RPC response.

**Entry.** A person enters the secret in the plugin's own row in the portal
settings (`pluginSettingGroups`, `plugin-repositories.ts` :506). The row uses
the existing `sensitiveText` interaction with a clear action. The surface
receives only `SecretSettingStatus` (`Configured`, `NotConfigured`), never the
value. The set and clear RPCs are today's pair with a plugin id added:
`system.settings.credential.set { pluginId, token }` and
`system.settings.credential.clear { pluginId }`. They stay `OwnerDeviceOnly`, so a
person's other device can also set a handheld's secret over the encrypted
peer wire. They answer in host mode too, wherever the plugin runs. This
removes today's host-mode refusal. `PluginSetting` gains
`credential: SecretSettingStatus` when the plugin declares a secret.
`SettingsSnapshot.steam_grid_db_credential` goes.

**Names.** Coordinator decision, 2026-10-23. Each name keeps its root, with
the SteamGridDB part removed:

| Name | Root |
|---|---|
| `plugin.nix` `credentialHost` (one host, a string) | legacy `credentialRequired: boolean`, renamed because the value is now the one receiving host, not a yes or no |
| file `<private>/<plugin id>/credential` | main `steamgriddb.credential` (`settings.rs` :29) |
| `system.settings.credential.set { pluginId, token }` | main `system.settings.steamgriddbCredential.set { token }` (`lib.rs` :1378) |
| `system.settings.credential.clear { pluginId }` | main `system.settings.steamgriddbCredential.clear {}` (`lib.rs` :1380) |
| `PluginSetting.credential: SecretSettingStatus` | main `SettingsSnapshot.steam_grid_db_credential` (`lib.rs` :943) |

**Approval.** The plugin host adds one sentence to the report's `warning`
when a plugin declares a secret. The sentence names the receiving host, as
the native credential sentence does today (`package.rs` :1003–1008). Both the
administrator CLI and the portal show `warning`, so the person sees which host
gets the key before approval. The wording is the implementer's.

**Removal.** Removing a plugin keeps its folder and its secret, as plugin-host
removal keeps credentials (`README.md` :181). Clear deletes the secret.

**Owner and devices.** Owner decision, 2026-10-23: a plugin secret is the
person's, kept on one device.

- It never moves to another device. It is not a record in the sense of
  CONTEXT.md and ADR 0027, so it never spreads and never goes to a relay. The
  person enters it once on each device.
- At an identity switch it follows the choice the person makes for the play
  log. The person picks `transfer` or `delete` in the switch request
  (`IdentityDataDisposition`, `lib.rs` :1079–1083). The journal keeps it as
  `DataDisposition` (`identity_switch/journal.rs` :15–18, :70). The
  coordinator applies it to the play log at `coordinator.rs` :350–360:
  `Transfer` calls `transfer_person_records` (`host/play_log.rs` :215) and
  `Delete` calls `delete_person_records` (:298). At the same point, korrid
  applies the same choice to every plugin secret on the device. `Transfer`
  keeps each `credential` file for the new owner. `Delete` removes each one.
  A delete of a file that is already gone succeeds, so a switch that resumes
  from its journal can run the step again, as `delete_person_records` can.

### 3. What this ADR does not cover

- **A secret that a native program reads,** such as a RetroAchievements token
  for RetroArch, a Tailscale auth key or a `butler` login. systemd credentials
  stay the only path, with the one admitted name. No current plugin asks
  korrid to supply such a value. Inferred, not checked: systemd can load a
  credential from an absolute path, so a later case can point
  `LoadCredential` at the plugin's folder. That waits for a real case.
- **Tailscale sign-in from the portal.** Today it needs an operator shell.
- **SSH key enrollment.** The SSH plugin README already records it as missing.
- **Settings for a plugin that is not a runner or modifier.** No real case
  exists.

## Grounding and deltas

| Element | Comes from | Delta and why |
|---|---|---|
| Input kinds | the inputs table above | None invented. Each row has a plugin or legacy producer. |
| Secret declaration | legacy `credentialRequired: boolean` | Renamed to `credentialHost` because the value is now the one receiving host, not a yes or no. |
| Declaration place | ADR 0016 `hosts` in `plugin.nix` and `manifest.json` | None. Same path into the approval digest. |
| Secret value | main `steamgriddb.credential`, legacy `credentials.get` | The plugin's JS no longer sees it (legacy did). korrid adds it as a header. |
| Bearer form | SteamGridDB OpenAPI, legacy itch.io :607, legacy `vite.config.mjs` | None. |
| Folder | owner decision 2026-10-23 | None. |
| File name | main `steamgriddb.credential` | `credential`: the SteamGridDB part removed. |
| Modes 0700 and 0600 | `nixos-module.nix` :496, :852, `identity.rs` :1093, :1199 | Set in code, not by `UMask` alone. |
| Status | main `SecretSettingStatus`, `SettingsSnapshot.steam_grid_db_credential` | Moves onto `PluginSetting` as `credential`. |
| Set and clear | main `system.settings.steamgriddbCredential.set { token }` and `.clear {}` | `system.settings.credential.set { pluginId, token }` and `.clear { pluginId }`. Adds the plugin id. Served in host mode. |
| Identity switch | main play-log `Transfer` and `Delete` (`coordinator.rs` :350–360) | The same choice now also keeps or deletes plugin secrets. |
| Approval sentence | `package.rs` :1003–1008 native credential sentence | A second sentence for the plugin secret. |

## Considered options

- **Give the secret to the plugin's JS** (legacy `credentials.get`, legacy
  itch.io). Rejected: the JS can put it in a URL or a header to any declared
  host. The sandbox stays free of secrets only if korrid holds them.
- **Use systemd credentials for every secret.** Rejected for this kind: the
  plugin's JS runs inside korrid, not in a systemd unit, so korrid must read
  the credential back to add the header. It also needs a root
  write into credstore for every key a person enters.
- **Store the secret in `device.yaml` beside other settings.** Rejected:
  `device.yaml` is readable config, and the tests forbid the key there.
- **Encrypt the file at rest with a key on the same device.** Rejected: it
  protects nothing that the folder mode does not already protect.
- **A general schema of plugin inputs with types and labels.** Rejected: four
  of five kinds already have a home, and no plugin needs the rest.

## Consequences

- A plugin's JS stays free of secrets. A second API-key plugin needs no new
  korrid code if it uses Bearer to one host.
- The person sees the receiving host in the approval warning before approval.
- A person can set a plugin secret on a device for the first time.
- The portal approval screen still does not list `hosts`. That gap comes from
  ADR 0016 and is not part of this decision.
- A plugin secret is plain text on disk. Anyone with root or the `korrid` user
  on the device can read it, including a plugin approved for `User=root`.
- A secret no longer passes to the next owner without a choice. Today the
  SteamGridDB key does.
- The person enters each key once on each device. A later move to spreading
  secrets between devices adds behaviour and removes none.

## Owner questions

No open owner questions.
