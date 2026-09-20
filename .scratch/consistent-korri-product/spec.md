# One Korri product across devices

Status: ready-for-agent
Parent: [One Korri product across devices](map.md)

Source: the 15 resolved decision tickets under `issues/`. Every decision below
is traced to the ticket that made it. This document adds no new decision. Where
a question is open, it says so and names the ticket that owns it.

## Problem Statement

A person who flashes Korri onto one handheld gets a different product from the
person who flashes it onto another. The difference is not the hardware.

- The RG353M runs a session with a streaming host, a plugin host and an audio
  path. The RG35XXSP boots to a root console and nothing else.
- Four devices create a `gameplay` account with uid 1001. One creates `korri`
  with uid 1000. One creates no product account at all.
- The Odin runs a different browser launcher from every other device, with a
  different way to pass its local korrid credentials.
- The RP Mini V2 switches off the shared streaming units by hand. Nothing in
  the product says it may.
- Two of six devices have no published image and no signed closure, so nobody
  can install them the normal way.

Each device was built on its own, so each one grew its own answer. When a
person moves from one Korri device to the next, the shared parts do not feel
shared. When a new device arrives, its owner rebuilds the whole product again
by hand, and quietly leaves parts out.

Some promised behavior is missing from every device. No device has ever slept
and woken. No device creates a person key at first boot. No device shows the
gameplay overlay over a real live session. No device frees storage when a
plugin is removed.

## Solution

One **product module** holds every required Korri part. Every device installs
it whole. A device supplies **hardware facts**, such as its kernel, its screen
and its input map, and nothing else. When a device cannot do an optional thing,
it states that as a **recorded limit** in one explicit sentence. A device may
not add, remove or swap a product part, and may not switch off a product unit
by hand.

One **product check** runs at evaluation time over every device configuration.
A device that does not install the product module fails to evaluate. The check
gates a build. It does not grant support.

A device becomes a **supported image** when three things are true: it has
**delivery**, which is a published installation image and its complete signed
closure from one commit; the product check passes; and a person completes the
physical acceptance list on the real hardware.

From the owner's side, every Korri device then behaves the same way. It creates
a real person key at first boot with no sign-in. It sets itself up through its
own screen, with no SSH and no configuration file. It browses the owner's
content, plays it locally or through a stream, and holds one **live session**
that the owner can **leave**, **return** to and **end**. It installs, updates,
removes and rolls back plugins, and frees the storage when a plugin goes. It
declares which **sleep states** it has, or records that it has none.

## User Stories

### First use and identity

1. As a person who has just flashed a card, I want the device to reach the Korri screen on its own, so that I do not need a serial cable or an SSH session to start.
2. As a new owner, I want Korri to create a real person key for me at first boot, so that I own my history from the first minute without signing in.
3. As a new owner, I want no sign-in screen at first boot, so that I can play immediately.
4. As a new owner, I want my person key to stay outside korrid in a local signer, so that a fault in the main service cannot leak it.
5. As a new owner, I want nothing published to a relay when my key is created, so that starting a device does not announce me to anybody.
6. As an owner, I want to find my identity in settings later, so that I can learn what Korri made for me.
7. As an owner, I want to export my person key as an encrypted backup from settings, so that I can keep it safe myself.
8. As an owner, I want the backup shown as text and as a QR code, so that I can store it in the way that suits me.
9. As an owner, I want Korri to state plainly that it cannot recover a lost key, so that I do not expect an account recovery that does not exist.
10. As an owner with a backup, I want to import it into a new device, so that the new device becomes mine instead of its automatic identity.
11. As an owner, I want to point a device at my remote signer instead, so that one key can serve several devices.
12. As an owner running an identity switch, I want to see a plain list of what I lose before I confirm, so that I do not lose pairings by surprise.
13. As an owner running an identity switch, I want the new key to sign the new owner statement before anything changes, so that a half-finished switch cannot lock me out.
14. As an owner, I want a failed switch to leave my old identity untouched, so that a power cut costs me nothing.
15. As an owner, I want to choose whether my local records move to the new key, so that I decide what the new owner inherits.
16. As an owner who declines the transfer, I want the old key's local records deleted, so that no orphan data stays on the device.
17. As an owner whose device lost power during a transfer, I want Korri to finish the transfer at the next start before it serves any of my data, so that I never read half-moved records.
18. As an owner, I want my old key kept in the signer as retired and exportable, so that I can still reach my old history until I delete it.
19. As an owner, I want deleting the old key to be a separate deliberate action, so that a switch cannot destroy it for me.

### Setup and routine management

20. As an owner, I want to join my Wi-Fi network from the Korri screen, so that I do not edit a file to get online.
21. As an owner, I want to choose where my content lives from the Korri screen, so that I can use the card or disk I prefer.
22. As an owner, I want to install, update, remove and roll back plugins from the Korri screen, so that I control what runs without a shell.
23. As an owner, I want normal system updates from the Korri screen, so that routine maintenance needs no operator.
24. As an owner, I want advanced repair kept separate from routine setup, so that ordinary tasks stay simple.

### Content and play

25. As an owner, I want to browse the content I supplied myself, so that I can find what I want to play.
26. As an owner, I want every item I can choose to lead to a real playable route, so that a successful preparation call is not a dead end.
27. As an owner, I want to play locally when my device can run the content, so that I do not depend on another machine.
28. As an owner, I want to play through a stream from another device when that route exists, so that a weaker handheld can still run heavy content.
29. As an owner, I want the route chosen for me from what is available, so that I do not learn which device holds which capability.

### Leaving, returning and ending

30. As a player, I want one tap of Home to open the gameplay overlay, so that I can reach Korri without leaving my game.
31. As a player, I want my game frozen while the overlay is in front, so that it uses no CPU and cannot advance without me.
32. As a player, I want Korri to own every input while the overlay or the portal is in front, so that my button presses never reach the frozen game.
33. As a player, I want a second Home tap to return me to the game, so that leaving is cheap.
34. As a player, I want the game to see a neutral controller when I return, so that a held button does not fire the moment I come back.
35. As a player streaming from another device, I want Home to freeze the far game too, so that the promise is the same on both routes.
36. As a player, I want the overlay to show only the actions my runner really supports, so that I never press a control that does nothing.
37. As a player using a runner with no session controls, I want the core controls alone, so that the screen tells me the truth.
38. As a player whose action was refused, I want a short notice that names the reason, so that I know the game is untouched.
39. As a player, I want the portal's end control to ask once before it stops my game, so that I do not lose progress by a stray press.
40. As a player whose game has eaten my input, I want the kill chord to end it at once with no prompt, so that I always have a way out.
41. As a player ending a streamed game, I want the far game and the local viewer both stopped, so that nothing keeps running where I cannot see it.
42. As a player, I want Korri to say so when it cannot confirm the far game stopped, so that it never reports an end it did not achieve.
43. As a player whose game window failed to raise, I want the game left running and return still offered, so that a display fault does not cost me my session.
44. As a player whose game already ended, I want Korri to tell me and reload, so that no control acts on a different launch.

### Plugins

45. As an owner, I want the bundled plugins owned by my plugin selections, so that I can remove one that came with the image.
46. As an owner, I want removal to free the storage before Korri reports it finished, so that "removed" means the space is back.
47. As an owner, I want a cleanup failure reported, so that Korri never claims space it did not free.
48. As an owner, I want my saves, settings, credentials and identity kept when I remove a plugin, so that removal never destroys my data.
49. As an owner whose device lost power during a removal, I want Korri to continue the same removal at the next start, so that the plugin does not come back to life.
50. As an owner, I want each installed plugin to keep its current and previous version, so that I can roll back a bad update.
51. As an owner, I want Korri to refuse to remove a plugin that a required behavior depends on, and to name that behavior, so that I cannot break my own device by accident.
52. As an owner, I want a required plugin disclosed with its permissions before I approve an update, so that I know what the release adds.
53. As an owner who declines a required plugin, I want my current release kept, so that a refusal does not leave me half upgraded.
54. As an existing owner, I want a newly recommended optional plugin offered, not installed, so that an updated default list does not change my device behind my back.
55. As an owner, I want my earlier removal and disablement choices respected by updates, so that I do not undo the same choice twice.

### Streaming host

56. As an owner, I want the streaming host to be a plugin I can remove, so that a device I never stream from does not run it.
57. As an owner who removed the streaming host, I want my device to boot to the portal, accept input and reach its korrid as before, so that removal costs me nothing else.
58. As an owner, I want removal to leave no unit, no socket, no service group and no device rule behind, so that the removal is complete.
59. As an administrator, I want the approval report to name every extra authority the streaming host asks for, so that I approve it with my eyes open.
60. As an administrator, I want the host to refuse by name anything the approval did not name, so that one plugin's authority never spreads to another.
61. As an owner, I want the plugin's declared ports opened while it is enabled and withdrawn when it is disabled or removed, so that the network surface follows the plugin.
62. As an owner of an R36T Max, I want the streaming host left out of my default selection with the missing encoder recorded as a limit, so that my device is honest instead of broken.

### Sleep and power

63. As an owner, I want one press of the power button to enter my device's preferred sleep state, so that the button does the obvious thing.
64. As an owner, I want a second press to bring me back, so that waking is as simple as sleeping.
65. As an owner, I want closing the lid to do what pressing the button does, so that both controls agree.
66. As an owner of a device with no sleep state, I want the button and the lid to shut the device down cleanly, so that I never lose data to a hard power cut.
67. As a player, I want my live session frozen before the device suspends, so that my game is where I left it on wake.
68. As a player who slept from the overlay, I want to wake to the overlay, so that Korri does not put me somewhere I did not leave.
69. As an owner, I want a device with no sleep state to still be a supported image with the absence recorded, so that a missing optional feature does not hold back a finished product.

### Device support and delivery

70. As an owner, I want the same required Korri behavior on every device I own, so that moving between them needs no relearning.
71. As an owner, I want a device's missing driver called unfinished work, not a hardware limit, so that nobody explains away an incomplete port.
72. As an owner, I want an image with a missing required behavior called a development image, so that "supported" means something.
73. As an owner, I want each device's optional gaps stated explicitly in its own module, so that I can read what my device does not do.
74. As an owner, I want a supported image to have a published image and a complete signed closure from one commit, so that I can install it the normal way.
75. As an owner of an RG35XXSP, I want a working screen, buttons, Wi-Fi and audio, so that the device is a real handheld and not a silent one.
76. As a maintainer, I want one product check over every device configuration, so that a new device cannot quietly omit a required part.
77. As a maintainer, I want the first image for a device model accepted by hand on real hardware with no manual fixes, so that support rests on evidence.
78. As a maintainer, I want later releases to need a boot-and-play check plus tests of what changed, so that repeat testing stays proportionate.
79. As a maintainer, I want the device check for each board to shrink to hardware facts, so that product rules live in one place.

## Implementation Decisions

### The product module

One shared Nix module composes every required part: base policy, the Linux
host, the portal browser path, the plugin host, device cache policy and the
runtime account. A device module imports it and supplies hardware facts and
recorded limits only. Source: ticket 07.

Today no module assembles the product. `nix/base` is a policy module that sets
SSH off, autologin, substituters and `stateVersion`, and composes no product
service. Six device modules assemble the product by hand, and each one differs.

A device may not add, remove or swap a product service. It may not force a
product unit off. The RP Mini V2 pattern of overriding `systemd.services.sunshine`
and `systemd.sockets.korri-certificate-control` ends.

### The hardware-fact option list

Tickets 07, 12 and 14 assigned this extraction to `/to-spec`. The list below is
extracted from the options the six device modules already set. It names no new
option except where a decision created one, and each of those is marked.

Hardware facts a device supplies:

| Option in use today | What it carries |
|---|---|
| `services.korriLinuxHost.label` | The device name used by the session |
| `services.korriLinuxHost.compositor.backend` | `drm` on every device today |
| `services.korriLinuxHost.compositor.drmDevice` | The KMS card, named by hardware path where card order is unstable |
| `services.korriLinuxHost.compositor.renderDevice` | The render node |
| `services.korriLinuxHost.compositor.outputName` | The connector, `DSI-1` on every device today |
| `services.korriLinuxHost.compositor.mode` | Resolution and refresh rate |
| `services.korriLinuxHost.compositor.renderer` | `gles2` on every device today |
| `services.korriLinuxHost.compositor.localInput.enable` | Whether the board has its own controls |
| `services.korriLinuxHost.compositor.remoteInput.enable` | Whether remote input is wired |
| `services.korriLinuxHost.compositor.extraConfig` | Native compositor configuration for rotation and touch mapping |
| `services.korriLinuxHost.deviceConfig` | The korrid device configuration file |
| Kernel, device tree and bootloader selections | Per-device packages |
| Input provider data | The board's controller profile |

Four values that devices set today are **not** hardware facts, and the product
module takes them over:

- `runtimeUser`, `runtimeUid`, `runtimeGroup`, `runtimeGid`. Ticket 11 fixes
  these at `korri`, uid 1000, group `korri`, gid 1000, home `/home/korri`, on
  every device. Four devices declaring `gameplay:games` 1001 converge.
- `compositor.neverFocusAppIds`. Ticket 12 gives the product the browser window
  identity and the game-return exclusion together.
- The portal's Wayland connection. Ticket 12 requires the portal to get it from
  the compositor. Its present read of the streaming host's service environment
  is an integration dependency to remove, because ticket 13 makes that host
  removable.
- `validation.enable` and `audio.enable`. Devices use these today to switch off
  unverified behavior. Under ticket 02 an off switch with no stated reason is
  unfinished integration. Each device either enables the behavior or records a
  limit.

Three values are unclassified, and the implementation must classify each one
with the user rather than guess:

- `relays`. Five devices set a different value, and two set an inert loopback
  endpoint. This is neither a hardware fact nor clearly a product constant.
- `services.korri.webSurfaceHost.surfaceId`. Four devices select `pico`. No
  ticket decided whether the surface choice belongs to the product, the device,
  or the owner.
- `services.korridLinuxDevice.address`. One device forces a non-default port.

### Sleep declarations

A device declares each sleep state it has. It does not hold one rank. The
states are **light sleep**, **deep sleep** and **hibernation**. A device with no
sleep declares none, and that is a recorded limit, not a blocker. Source:
ticket 15.

All six devices declare no sleep state on day one. Every one records the limit.

The preference order while hibernation stays deferred is deep sleep, then light
sleep, then a clean shutdown. The power button enters the preferred state; a
second press returns. The lid follows the button. On a device with no state,
both shut the device down cleanly with no prompt. This replaces the ignored
power key the Odin declares today.

Light sleep freezes the live session, turns the screen off, keeps the device
awake, and shuts down cleanly after a fixed product-wide delay. **The number is
not chosen.** Ticket 15 records that ROCKNIX uses 900 s and leaves the value to
the implementation.

Ticket 15 states that the option name and its place in Nix are not chosen. The
implementation names it beside the other hardware facts and confirms the name
with the user. The three state names come from the glossary and are fixed.

No test gates a declaration. Correcting a wrong declaration is ordinary
maintenance. Korri does not use the word `standby`, because the kernel already
uses it for a different thing.

### The runtime account

User `korri`, uid 1000, group `korri`, gid 1000, home `/home/korri`, declared
by the product module. These values come from the Odin's existing declaration,
not from a new schema. Source: ticket 11.

No migration. Simon removed it from scope: "there will be 0 backwards compat.
we are in alpha stage right now. dont waste cycles on this". Do not build an
upgrade path, an alias, a fallback read or a dual write. Do not recreate this
work as a ticket. Installed alpha devices stay untouched; this spec authorizes
no wipe and no deployment.

The account change does not touch the person key, the device owner, or the
separate service identities. korrid's private state stays unreachable from the
runtime account.

### The browser path

The portal module with the private `window.KorriRpc` binding is the product
browser path. The Odin converges to it. Retire the old kiosk launcher crate and
its NixOS module, and remove its package, module and check registrations.
Source: tickets 07 and 12.

Keep no internal adapter, no `/runtime.json` fallback and no second credential
delivery path. Keep the independent Chromium transparency patch and its native
pixel tests; retiring the launcher neither deletes nor certifies that work.

The product owns browser startup, security, the browser window identity and the
game-return exclusion. The device describes its screen, GPU and controls. The
new shell's real Wayland app id must be determined on the device. The old
bootstrap URL's app id cannot be carried forward, because it came from a URL
the new shell does not use.

The portal keeps its own `korri-portal` service identity, which stays separate
from the `korri` runtime account.

### Identity and the local signer

A new **local signer** service holds the person key under its own uid and its
own private directory. It answers korrid through the existing `PersonSigner`
contract. korrid never holds a person private key. Source: ticket 08.

At first boot the signer creates the **automatic identity**, and korrid binds it
as device owner at once through the existing owner-binding path. No screen, no
sign-in, no relay publication, no federation.

Backup is a NIP-49 encrypted export of the person key, on demand from settings,
shown as text and as a QR code, with a plain statement that Korri cannot recover
a lost key.

An **identity switch** is an owner replacement and the only way device authority
moves. The replacement key comes from a NIP-49 backup imported into the local
signer, or from a NIP-46 remote signer. The new key must sign the new owner
binding, and the owner must confirm a plain list of what is lost: every peer
pairing and every stream client trust.

The order is prepare, then commit: stage a new device key, get the new owner
binding signed, get the old owner's revocation signed, then swap the identity
directory once. Any failure before the swap leaves the old identity untouched.
The switch publishes the old owner's revocation only if the old owner statement
was published before.

On transfer, unsigned per-person records are re-keyed from the old key to the
new key, and the old key's local records cease to exist on that device. Signed
events stay with the key that signed them; nothing is re-signed. On refusal,
the old key's local records are deleted. There is no orphan state and no
fallback read. If power fails after the commit and before the transfer
finishes, korrid finishes it from its journal at the next start, before it
serves any per-person data.

The only person-keyed record today is the play log. Save files and save states
are deferred, so their location and ownership do not change here. The fixed
`users/default` account root that plugins use today stays as it is.

`korrid identity reset` stays a device-identity reset. It removes the device key
and the owner event only. Wiping the signer is a separate explicit operation.

### The live session

korrid holds one live session per device, named by its launch id. Every control
names that id and is refused for any other launch, answering `StaleIdentity`.
Source: ticket 09.

Home opens the gameplay overlay and korrid freezes the exact launch. The game
uses no CPU, draws nothing and hears no input. Korri owns all input while the
overlay or the portal is in front. A second Home tap returns. The full portal is
reached from the overlay, not from Home. For a streamed session korrid forwards
the freeze to the far device.

Return thaws the exact launch and raises its window. Input that arrived while
Korri was in front is discarded: the game sees a neutral controller state, then
live input. This needs a seat reset that the present START/STOP seat protocol
does not have. On `FocusFailed` the overlay shows the failure, the game keeps
running and return stays available. Korri does not refreeze and does not end a
session for a display fault.

The portal's end control asks once, then stops the exact launch. The kill chord
acts at once with no confirmation. For a streamed session both stop the exact
far launch and the local viewer. A near device that cannot confirm the far stop
says so and does not report the session ended. There is no third verb: closing
the viewer while the far game runs is not a product behavior.

The overlay shows the core controls plus only the actions korrid lists for the
exact session. Today that is RetroArch and Moonlight; every other runner shows
core controls only. Nothing is greyed out and nothing is invented. A refused
action shows a short notice naming the reason, and the session is untouched.

Unbuilt today and part of this work: the sleep hook, the overlay controller
against real korrid state, the system panel wiring, the seat reset, and the
Linux stream viewer.

### Plugin lifecycle

Bundled plugins are owned by plugin selections, not by the system image. The
image delivers the initial plugins and their approval records; after that the
plugin host owns them like any installed plugin. Source: ticket 04.

The image-delivery producer does not exist and must be built. The plugin host
already has the runtime half: a `korri-plugin seed` entry point computes the
approval digest through the same load path the device re-derives, and its tests
assert that agreement. The missing half is the Nix producer that writes seeded
receipts and plugin packages into an image. Extract the receipt shape from that
existing entry point and its tests. Do not design a new receipt format.

Uninstall releases the plugin's current and previous software selections, and
reclaims eligible storage **before** it reports completion. Releasing references
alone is not completion. Cleanup failures are reported, not hidden. Only unused
software is eligible: user data, files other plugins need, and retained system
versions all stay.

An interrupted uninstall resumes at the next startup against the original
request. Interruption must never silently re-enable the plugin.

A **required plugin** supports an agreed required behavior on that device.
Default-bundle membership alone does not make a plugin required. Required
plugins are disclosed with their permissions before update approval and install
as part of the update. If the owner declines or installation fails, the new
release does not activate and the current release stays selected. Removal is
refused while a plugin is required, with the dependent behavior named.

New optional defaults reach existing devices through plugin management only.
System updates do not add them. Fresh installations still get the full default
selection.

### The streaming host as a plugin

Hosting a stream is a bundled plugin, not a product part. Source: tickets 07
and 13.

The plugin host widens what it admits in a native unit until the streaming host
can ship with its real systemd unit. korrid does not take the privileged parts
and hand them back. This is a change to the plugin host's security boundary, not
a packaging change, and it needs its own review and tests.

The approval binds the reach. Each plugin's approval names the exact units,
directives, devices and capabilities that plugin requested, and the host refuses
anything the approval did not name, by name. One plugin may ship more than one
unit when its approval names them; the streaming host needs three today. This
follows the path the host already uses for a native `User=root` request: the
request asks, the report names the authority, the administrator approves the
exact build, and the policy version enters the effective unit and the approval
digest.

The streaming host is in the default selection of every device except the
R36T Max, which records its missing H.264 encoder as a limit and carries no
disabled unit. The RP Mini V2 stops force-disabling the units.

The plugin declares its TCP and UDP ports, and the host opens exactly those on
every interface through its own chain. Disable, removal, failed enable and
rollback withdraw the rules. The administrative port stays undeclared and
closed. The RG353M loses its present narrowing to two named interfaces.

korrid performs no encoder match and reads no encoder fact. One streaming plugin
release for each Nix system carries every encoder the architecture supports, and
Sunshine selects at start through its existing automatic selection. The
evaluation-time assertions that bind one exact approved build disappear with the
product-module composition; install-time approval of the exact build replaces
them.

**Open decision, owned by this work.** Ticket 13 did not settle how korrid stays
agnostic to the streaming host. The certificate control socket is named for
Sunshine, korrid speaks the protocol that one Sunshine patch implements, and the
input-seat receiver and its group are named for Sunshine as well. Whether the
plugin ships the seat under its own approval, or korrid offers a generic seat, is
undecided. Settle both with the user before the cut is implemented. Do not
invent a generic capability name here.

### RG35XXSP adoption

The RG35XXSP is a product device. Its console-only composition ends, and its
automatic root login on the serial and virtual consoles is bring-up state, not a
product part. Source: ticket 14.

It supplies a ROCKNIX-derived Linux 7.2 kernel. Mainline describes no display
engine, no TCON and no panel for H616 or H700 in 6.12 or 7.2, so the pinned
6.12.63 kernel produces no picture. Panfrost, mainline U-Boot v2025.10 with
`anbernic_rg35xx_h700_defconfig`, the AXP717 and the RTL8821CS all stay as
hardware facts.

The device is finished when the screen, the buttons, Wi-Fi and audio work.
Bluetooth, battery percentage, the real-time clock and USB host mode may each be
a recorded limit. Neither the 1 GB of RAM nor the 640x480 panel is a limit or a
blocker; both are ordinary hardware facts, and the memory cost is measured at
physical acceptance.

Every compositor and input value for this board is unknown until it boots. The
unlanded display worktree is the starting point for the port, not an approved
change, and nothing in it has been verified on hardware. The device fails the
shared base check today because it enables graphics with no seat; the
implementation corrects that, and does not widen the check's exclusion list.

### Delivery

A supported image needs a published installation image and its complete closure
in the signed cache, both from one commit. Source: ticket 07.

Today the image and cache workflows name four devices: RG353M, Odin 2 Portal,
RG DS and R36T Max, and the image workflow holds R36T Max back until its loader
notices ship. The RP Mini V2 and the RG35XXSP have no delivery. Their workflow
entries land in the same change that makes each device import the product
module.

Developer deployment tooling is outside the product. A supported image accepts
signed outputs only. The present deploy script copies with `--no-check-sigs`,
which the device cache policy forbids on devices; it stays a development-only
path.

### Default plugin selection

Each device model has its own default emulator list, because the selection
depends on the device. Defaults are curated and dependable: one preferred runner
for each included system, with alternatives left to the owner. Source: ticket 03,
with the six lists selected in ticket 10 from pinned ROCKNIX platform tables.

Moonlight ships preinstalled as a removable plugin for now, with a later move to
opt-in installation. No date was chosen. SSH and Tailscale are optional
additions, not preinstalled. Content acquisition is deferred, so no legacy
provider is ported.

The RG DS list uses the RK3566 table as a stated analogy. The R36T Max uses the
documented RK3326 family. Standalone PPSSPP is selected rather than the generated
libretro plugin with the same identity; that identity conflict must be resolved
if PSP stays in a selection. Missing packages and unfinished integration are
implementation gaps, not silent substitutions.

### Support policy

An image with a missing applicable required behavior is a **development image**.
Documenting the gap does not make it supported. A missing driver, unfinished
integration and unverified operation are not hardware limits.

Hardware features the product needs must work. Other hardware features may stay
unsupported when the limitation is explicit. Hardware variation may change which
content routes exist; it may not remove shared required behavior.

The first supported image for a device model needs full physical acceptance
through normal Korri setup with no manual fixes. Later supported releases need a
boot-and-play check plus physical tests of affected behavior on the affected
models. Earlier evidence carries forward for unchanged behavior, and identifying
each change's impact is part of the release work.

## Testing Decisions

A good test here checks what an owner or an administrator can observe. It does
not assert the shape of the code that produces it. Two rules follow from the
decisions themselves. A passing check grants a build, never support. A
configuration assertion is not physical acceptance.

Four seams carry this work. The user approved growing the existing plugin-host
virtual-machine test rather than building a second harness.

### 1. The product check, at evaluation time

New. One flake check reads every entry in `nixosConfigurations` and asserts that
each one installs the product module and every required part, that the runtime
account is `korri` uid 1000, that no product unit is force-disabled, that each
device declares its sleep states or none, and that a device claiming support has
both delivery entries. A device that does not import the product module fails to
evaluate.

Prior art: the `korri-base` check, which already evaluates the real base without
a device module and then compares every exported device against it, including a
named single exception. The `korri-portal-module` and `korri-device-cache` checks
follow the same shape.

Per-device checks shrink to hardware facts. Two devices have no device check at
all today, and the product check covers them from the first run.

What this seam cannot prove: that any of it works. The RP Mini V2 declares
`transform 90` while its own check asserts `270`, and the check passes.

### 2. The product virtual-machine test, at runtime

Grown from the existing `korri-runtime-plugin-host` check, which already boots a
real machine with real systemd, a real korrid and a real plugin host, and drives
a cold-host install, enable, update and remove.

Extend it to the product behavior that evaluation cannot see:

- First boot creates the automatic identity in the local signer and binds it as
  device owner, with nothing published.
- The portal starts, consumes both binding methods before it mounts, and
  completes an authenticated call to its own korrid. The host reports ready only
  after both calls.
- The portal refuses to start when a binding method is not consumed.
- Plugin removal frees storage before it reports completion, and reports a
  cleanup failure instead of claiming space.
- A removal interrupted by a restart resumes and does not re-enable the plugin.
- Removal of a required plugin is refused and names the dependent behavior.
- With the streaming plugin removed, the device still boots to the portal,
  accepts input and completes an authenticated local RPC, and no unit, socket,
  group or device rule is left behind.
- An identity switch commits atomically, or leaves the old identity untouched,
  and finishes an interrupted transfer from its journal before serving
  per-person data.

What this seam cannot prove: anything about a real panel, a real controller, a
real encoder or a real battery.

### 3. korrid crate tests

Ticket 09 chose these as its evidence, so the choice is fixed. The session state
machine already carries `Running`, `Frozen`, `Stopping`, `Completed`, `NoActive`
and `RecoveryBlocked`, and already answers `StaleIdentity` for any launch but the
exact one.

Cover freeze, thaw, stop, `StaleIdentity`, `FocusFailed`, and recovery after a
korrid restart. Add the seat reset that input discard on return needs, and the
sleep hook, at the same seam. Physical per-device passes, a near/far streaming
pass and a sleep/wake pass were offered to Simon and not selected; they reach the
product through the physical acceptance list instead.

Prior art: the existing host module tests beside `session_state`,
`compositor_focus`, `input_seat` and `identity`.

### 4. Plugin-host crate tests

The widened unit admission is a security change, so it is tested where the policy
lives. Cover: an approval that names extra units, directives, devices and
capabilities admits exactly those; anything unnamed is refused by name; the
approval report names the widened authority before approval; and the policy
version reaches the effective unit and the approval digest.

Prior art: the existing unit-policy, native-unit and host-boundary tests, and the
seed test that already asserts an image-time receipt matches what the device
re-derives.

### 5. Physical acceptance, by a person

No automated seam reaches this. For the first supported image of each device
model: fresh flash; normal setup with no manual fixes; browse and launch one
local game for each bundled runner; leave, return and end; one streamed session
if the device hosts or plays streams; sleep and wake if the device declares a
state; plugin install, remove and rollback; identity backup export. Each device
also records which encoder Sunshine selected at start, with CPU load and
temperature.

Later releases need boot-and-play plus the affected behavior. There are no
numeric performance targets. Two testers can disagree about "works", and nothing
measures speed.

An automated on-device smoke run is the named future gate: boot, portal
reachable, one launch, required for support, with physical tests only for changed
behavior. No infrastructure supports it today.

## Out of Scope

- **Content acquisition** and the legacy provider ports. Deferred by ticket 01.
  Plugin installation and updates stay in scope; acquiring game content does not.
- **Cross-device save synchronization.** Deferred by ticket 01. No mechanism,
  conflict policy or save identity was chosen.
- **Automatic restoration of a game after shutdown.** Deferred by ticket 01.
  Normal saving is enough. Sleep is not shutdown.
- **Save file and save state ownership during an identity switch.** Deferred on
  2026-09-20. Saves sit under a fixed `users/default` account root today, which
  is an accident, not a design. Do not move them here.
- **Hibernation.** Named in ticket 15 and deferred, together with which storage
  may hold the memory image. Three facts deferred it: an SD card writes at
  roughly 20 to 90 MB/s, the card is removable, and three device checks assert
  no swap device today.
- **Runtime-account migration and backward compatibility** for installed alpha
  devices. Excluded by ticket 11 in the user's own words. Do not recreate it.
- **Image size measurement** (ticket 05) and **image variants and first-use
  readiness** (ticket 06). Both skipped at the user's request. Skipping them
  chose no image count, no size threshold and no offline-readiness promise.
- **Implementing performance targets.** No numeric target exists for any device.
- **Android.** Removed on 2026-09-16.
- **Any bootloader or firmware partition write.** Repository policy forbids it.
  This spec authorizes no device write, no flashing and no deployment.

## Further Notes

### Open decisions to settle during implementation

Five questions are open. Each one must be settled with the user at the point the
work reaches it, not filled in by an agent.

| Open question | Owner |
|---|---|
| How korrid stays agnostic to the streaming host: the certificate socket and the input-seat group are both named for Sunshine | The streaming-host cut, before it is implemented (ticket 13) |
| The fixed delay that ends light sleep | The sleep implementation (ticket 15) |
| The Nix option name and place for sleep-state declarations | The product-module implementation (ticket 15) |
| Whether `relays`, `surfaceId` and the korrid bind address are hardware facts or product constants | The product-module implementation |
| The PPSSPP standalone/libretro identity conflict, if PSP stays in a default selection | The plugin-selection implementation (ticket 10) |

### Costs this spec accepts

- One product module means one product change touches all six devices at once,
  and the refactor of six hand-assembled modules is real work before any device
  benefits.
- Retiring the kiosk crate removes the Odin's only working browser path until
  the shared portal runs there.
- Moving the streaming host into a plugin is a port, not a toggle. Until it
  lands, no device has a supported streaming host. Every device needs physical
  retesting after the cut.
- Widening plugin-host admission grows approval reports. A long report is harder
  to read, and a report that is hard to read is easier to approve without
  reading.
- Every device carries encoder support it never runs. The storage cost is
  unknown, because image measurement was skipped.
- Uninstalls take longer and can stay incomplete when cleanup fails. There is no
  offline undo after an uninstall.
- Refusing a required plugin blocks an upgrade.
- No recovery without a prior key export. Every identity switch costs re-pairing
  on every peer and every stream client.
- Freezing a game cuts audio at once and times out online games while the owner
  is in a menu.
- Light sleep ends a live session the owner did not end. It is the one place
  where Korri destroys a session by a timer.
- Nothing catches a wrong sleep declaration. A device that declares deep sleep
  without having it promises a wake that fails in the owner's hands.
- Three of six devices cannot be supported until the workflows cover them.
- The RG35XXSP kernel is 29 patches and an 8128-line configuration, re-based by
  hand on every bump, tracking ROCKNIX rather than upstream. Requiring audio can
  stall its adoption, and its Wi-Fi part already logs SDIO timeouts on the
  RG353M.

### Grounding

Every decision traces to a resolved ticket in `issues/`. Source facts in this
document were read in the tree at `d318df04` and in the evidence files under
`evidence/`. No device was contacted, no build was run and no test was executed
while writing this spec. Where this document states current behavior, it states
what the source declares, not what an installed device does.

The glossary at `CONTEXT.md` holds the agreed meaning of every bold term here.
Use those words in the tickets and in the code. It already carries the state
model from ticket 15: a device declares every sleep state it has and does not
hold a rank. Resolved tickets keep their older wording, so read "sleep tier" in
ticket 07 as "declared sleep state".
