# Decide the Odin runtime account cut-over

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: resolved
Blocked by: 07

## Question

The product owns the runtime account with one name and one uid on every device ([Decide the shared product and hardware boundary](07-decide-product-device-boundary.md#ownership)). Odin defines `users.users.korri` in [runtime-user.nix](../../../nix/devices/odin2portal/runtime-user.nix); three other devices define `gameplay` uid 1001. What is the product account, and what happens to data under Odin's existing account at cut-over?

Inspect the real Odin install for what lives under the current home: plugin storage, saves, korrid private state, owner binding. Recheck [korri-linux-host](../../../services/inputd/nix/korri-linux-host.nix) `runtimeUser`, `runtimeUid`, `storageRoot`, and `privateStateRoot` for what depends on the uid. Repository rules apply: one clean cut, no dual reads or fallback paths, an explicit one-off operational migration for real user data.

Resolve with Simon the account name and uid, the migration step and its evidence, and whether a device that never had a different account needs anything. Record decisions, not a migration script.

## Comments

Investigation at `bf4feea8`: four device configurations already declare `gameplay` uid 1001, primary group `games` gid 1001, and home `/home/gameplay`: `nix/devices/rg353m/sunshine-host.nix:22-29`, `nix/devices/r36tmax/portal.nix:13-20`, `nix/devices/rgds/portal.nix:44-51`, and `nix/devices/rpminiv2/portal.nix:39-46`. The question's reference to three devices omitted RG353M. Odin declares `korri` uid/gid 1000 and `/home/korri` in `nix/devices/odin2portal/runtime-user.nix:8-17`; `web-session.nix:25-28` selects that identity for the host.

The host derives its runtime directory from the uid and its home from the account declaration. It requires runtime and service identities to remain distinct (`services/inputd/nix/korri-linux-host.nix:42-43,667-709`). Its state defaults are `/var/lib/korri` and `/var/lib/korrid`, not the runtime home (`:477-484`). The korrid module owns its private identity directory as `korrid:korrid` (`services/korrid/nixos-module.nix:439-440`). An account change does not authorize changing device identity or granting the runtime account access to private state.

Live inventory is unavailable so far. Tailscale reports `odin2portal` at `100.68.151.111` offline, last seen five days ago. Read-only SSH attempts with strict host-key checking failed: `192.168.1.103` returned `No route to host`; `100.68.151.111` timed out. No device state was read or changed. Installed files, ownership, and uid collisions remain unverified.

## Answer

Resolved 2026-09-20. Simon selected the account through `ask_user`, then removed migration work from scope.

### Product account

Every device uses user `korri`, uid 1000, primary group `korri`, gid 1000, and home `/home/korri`. These values come from `nix/devices/odin2portal/runtime-user.nix`, not a new account schema. The product module owns the declaration, as ticket 07 decided.

Odin's source declaration already matches. Four other device configurations declare `gameplay:games` uid/gid 1001 and must converge instead. RG35XXSP has no product runtime account declaration. These are source facts, not an inventory of installed systems.

Fresh installations create the selected account through the product module. A matching account needs no account-conversion logic. The account choice does not change the person key, device owner, or separate service identities. In particular, korrid's private state remains inaccessible to the runtime account.

### Scope disposition

When asked to choose a migration policy for existing `gameplay` installations, Simon answered:

> there will be 0 backwards compat. we are in alpha stage right now. dont waste cycles on this

No migration design or implementation belongs to this effort. Do not build an upgrade path for existing alpha accounts, compatibility aliases, fallback reads, or dual writes. This supersedes the migration requirement in the original question and ticket 07's assumption that Odin needed an account migration. Do not recreate that work as a child ticket or a prerequisite for `/to-spec`.

Existing installations remain untouched by this planning decision. It authorizes no wipe, ownership rewrite, identity reset, or device deployment.

### Evidence and limits

Source inspection establishes the current declarations and the selected target. Odin was unreachable, so this decision makes no claim about its installed data or service state. A live inventory is no longer a prerequisite for closing this planning ticket because migration is out of scope.

Cost: four device configurations must change, and existing alpha installations have no supported account-upgrade path from this decision. Product checks and physical acceptance still apply to new images under tickets 07 and 02. No runtime tests, builds, or deployments were performed here.
