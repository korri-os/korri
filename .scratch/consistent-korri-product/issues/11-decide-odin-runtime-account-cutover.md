# Decide the Odin runtime account cut-over

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: claimed
Blocked by: 07

## Question

The product owns the runtime account with one name and one uid on every device ([Decide the shared product and hardware boundary](07-decide-product-device-boundary.md#ownership)). Odin defines `users.users.korri` in [runtime-user.nix](../../../nix/devices/odin2portal/runtime-user.nix); three other devices define `gameplay` uid 1001. What is the product account, and what happens to data under Odin's existing account at cut-over?

Inspect the real Odin install for what lives under the current home: plugin storage, saves, korrid private state, owner binding. Recheck [korri-linux-host](../../../services/inputd/nix/korri-linux-host.nix) `runtimeUser`, `runtimeUid`, `storageRoot`, and `privateStateRoot` for what depends on the uid. Repository rules apply: one clean cut, no dual reads or fallback paths, an explicit one-off operational migration for real user data.

Resolve with Simon the account name and uid, the migration step and its evidence, and whether a device that never had a different account needs anything. Record decisions, not a migration script.
