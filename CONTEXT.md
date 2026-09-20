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
