# Define the intended Korri product

Parent: [One Korri product across devices](../map.md)
Label: wayfinder:grilling
Type: grilling
Status: resolved
Blocked by:

## Question

Which user-visible behaviors belong to the intended Korri product, which are optional plugin choices, and which are deliberately outside it?

Start with the map's agreed brief. Inventory actual current consumers, device configurations, retained legacy behavior, and prior product decisions before asking Simon to fill gaps. RG353M is a useful reference, not the definition of completeness. Existing code proves an implementation exists; it does not prove the behavior remains wanted.

Review the whole experience, including first use, content discovery and play, federation, plugin management, and the controls needed to operate and recover the device. These are investigation areas, not a preapproved feature list. Respect the existing rule that a device need not have a screen or any fixed role.

Start from [device compositions](../../../nix/devices/), [surface contracts](../../../contracts/surface/korri-surface.ts), the [plugin-model brief](../../../docs/briefs/2026-09-15-plugin-model-brief.md), and its precise legacy references. Check other product decisions when the observed behavior points to them. Treat stale READMEs and historical work items as historical evidence, not current requirements.

Resolve through a live exchange with Simon. Record the agreed behavior inventory, source pointers, and known gaps. Distinguish required behavior from the plugins proposed to provide it. Create further decision tickets for newly exposed uncertainties rather than selecting schemas or architecture to make the inventory concrete.

## Comments

Source inventory is available in [Product behavior inventory](../evidence/product-inventory.md). It separates current consumers, legacy implementations, placeholders, and recorded intent. No device behavior was verified. The inventory itself is evidence, not an approved product classification.

### Routine setup and management

Simon selected self-service: after flashing, users must complete routine setup and management through Korri without SSH or configuration-file edits. The accepted option includes network setup, choosing content locations, managing plugins, and normal updates. This adds working product controls where an operator currently fills gaps. Advanced configuration and emergency repair are separate from this routine-use requirement.

### Content acquisition

Simon initially confirmed both user-supplied content and finding/acquiring content through optional plugins. Acquisition retains intended behavior from legacy that has not yet been ported, rather than adding a new product direction.

Subsequent scope amendment: while choosing default plugins, Simon was not ready to port the legacy providers and explicitly deferred acquisition from this effort. Current installations focus on user-supplied content. Acquisition remains future product intent, but neither provider ports nor a working acquisition route block completion of this effort. This deferral does not remove plugin installation or updates from the current product.

### Cross-device saved progress

Simon wants normal game-save synchronization eventually, but not today. Retain it as future product intent, not a requirement for the current consistent-installation effort. Do not make this effort wait for save synchronization. No synchronization mechanism, conflict policy, or save identity was selected. Emulator savestates and live-session transfer were not decided by this answer.

### User identity

Simon approved the direction of automatically creating a real Nostr user identity without mandatory sign-in, with an explicit option to switch to another Nostr identity later. It is not a temporary Korri account that must later be converted. Creation alone does not authorize publishing a profile or joining a federation. A switch should offer a choice to transfer local library choices, saves, and preferences without reinstalling; the identities and their signed histories remain distinct.

This is product direction, not approval of a key-storage or ownership-transfer design. The accepted identity protocol keeps the person's private key outside Korri. Current general owner replacement is refused; the existing offline exception only retires a test owner. Resolve signer custody, backup/recovery, data transfer, and old-access retirement in [Decide automatic Nostr identity and later replacement](08-decide-user-identity-lifecycle.md).

### Live game controls

Simon accepted working core session controls plus supported plugin actions in the current scope. Every installation must let the user return to Korri, return to the running game, and end the session. Korri must expose additional working actions supplied by the active plugin, such as save/load state or fast-forward when supported. This does not require identical features from every emulator or runner. Automatic pausing when opening Korri was not decided by this answer. It does not change the deferral of cross-device save synchronization.

Simon subsequently deferred automatic restoration of the exact game state after shutdown. Normal game saving is sufficient for the current effort. Supported plugin save-state controls remain in scope; their availability is not a promise of automatic restoration after power-off.

## Answer

Simon explicitly approved the consolidated behavior scope and closing this ticket, then explicitly deferred acquisition while choosing default plugins. The scope below reflects that amendment. The detailed agreements above remain the source for setup, acquisition timing, identity, session controls, and deferred saved-progress features.

The approved scope also retains local and streamed play through available routes, consistent with the repository's federation direction. Browsing and choosing content must lead to real playable routes, not only a successful preparation call. Devices do not acquire fixed roles.

Plugin management includes independent installation, updates, removal, and rollback. Removal reclaims storage that nothing else needs, subject to shared dependencies and retained rollback versions. The plugin-lifecycle ticket must reconcile the existing current-plus-previous rollback agreement with removable image bundles.

| Classification | Approved scope |
|---|---|
| Required now | Self-service setup and routine management; real Nostr identity without mandatory sign-in and an explicit later switch; browsing user-supplied content; local and streamed play through available routes; working core session and supported plugin controls; independent plugin lifecycle. |
| Optional selection | Individual runners and other plugins. Additional session actions depend on the active plugin's support. This is not permission for each device to silently omit the product integrations. |
| Deferred | Content acquisition and legacy acquisition-provider ports; cross-device game-save synchronization; automatic restoration of the exact game state after shutdown. Normal saving and supported plugin save-state controls remain in scope. |

The accepted cost is implementing or porting missing behavior; sharing device configuration alone cannot meet the scope. Approval establishes intent, not current feature completeness or verified device support. Existing user data remains protected by repository policy.

Hardware limits and product acceptance, default plugins, image variants, bundled-plugin removal, identity security, and shared integration are tracked separately in the map. The now-specific question of live pause/input/return behavior is captured in [Decide what happens when leaving and returning to a game](09-decide-live-session-behavior.md). This resolution approves no new schema, key-custody exception, device write, or deployment.
