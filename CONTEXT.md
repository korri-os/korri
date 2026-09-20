# Korri

Product terms shared across Korri devices. These definitions distinguish the product promise from work still being tested.

## Language

**Supported image**:
A Korri installation image verified to meet all agreed product requirements that apply to its device. It can have explicit limitations in hardware features that the product does not require.

**Development image**:
A Korri installation image that has not qualified as a supported image. It can boot and provide some features without meeting all applicable product requirements.

**Required plugin**:
A plugin needed for a Korri release to provide an agreed required behavior on a device. Inclusion in a default bundle alone does not make it required.

**Person key**:
The Nostr key that identifies one person to Korri. korrid never holds its private half.
_Avoid_: account, user key, login

**Device owner**:
The person key whose signed statement binds one device key. It is the person for every local call on that device.
_Avoid_: active user, current user

**Local signer**:
A separate service on a device that holds a person key and signs for korrid on request.
_Avoid_: keystore, wallet

**Automatic identity**:
The person key a local signer creates on first boot without sign-in. It is a real person key, not a temporary account.
_Avoid_: guest account, default user

**Identity switch**:
The explicit, consented replacement of a device owner by a different person key. It is the only way device authority moves.
_Avoid_: login, account transfer, sign in

**Live session**:
The one exact game launch a device holds as active. Every session control names it and is refused for any other launch.
_Avoid_: current game, running process

**Leave**:
Moving away from a live session without ending it. The session is frozen and Korri owns input until the user returns.
_Avoid_: pause, minimise, background

**Return**:
Coming back to a live session. Korri thaws it, raises its window, and gives it live input.
_Avoid_: resume, unpause

**End**:
Destroying a live session on purpose. Unsaved progress is lost.
_Avoid_: quit, kill, close

**Gameplay overlay**:
The screen Korri shows over a live session. It offers return, end, the full Korri screen, and the actions the active plugin supports.
_Avoid_: pause menu, system panel

**Product module**:
The one shared composition of every required Korri part. Every device installs it whole.
_Avoid_: base, shared config, host module

**Hardware fact**:
A value only a device can supply, such as its display, kernel, or input map. It changes how the product runs, never what the product contains.
_Avoid_: device override, device setting

**Recorded limit**:
An explicit statement that an optional hardware feature is absent or unverified on one device. It is the only allowed omission from the product.
_Avoid_: disabled feature, workaround

**Product check**:
The single evaluation-time check that every device configuration contains every required part. Passing it allows a build, not a support claim.
_Avoid_: module check, lint

**Delivery**:
A published installation image and its complete signed-cache closure from the same commit. A supported image needs both.
_Avoid_: release, artifact

**Sleep state**:
One kind of device sleep a device model can provide: light sleep, deep sleep, or hibernation. A device declares every state it has, and declares none when it has no sleep.
_Avoid_: sleep tier, suspend support, power mode

**Light sleep**:
The screen is off, the live session is frozen, and the device stays awake. A fixed delay then shuts the device down cleanly.
_Avoid_: standby, fake suspend, idle

**Deep sleep**:
Suspend to RAM. The frozen live session returns on wake, with no time limit.
_Avoid_: S3, mem, real suspend
