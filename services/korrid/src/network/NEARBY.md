# Nearby link

The owner approved the nearby link contract on 2026-10-23. ADR 0026 defines
its purpose. The `spike/reach-nearby` probe at `90ab09ea8` supplies the native
NetworkManager settings and the addresses, `10.77.77.1/24` and
`10.77.77.2/24`. This code adds no persisted Korri schema.

## Key agreement

`DeviceIdentity::nearby_key_material` uses
`nostr::nips::nip44::v2::ConversationKey::derive`. This is the key agreement
used by identity's existing NIP-44 encryption. The conversation key stays in
identity. HMAC-SHA256 under that key, with `korri/nearby/key-material/v1\0`
and the sorted, fixed-length public keys, makes nearby-only material.

Separate HMAC-SHA256 labels make the SSID and passphrase:

- `korri/nearby/ssid/v1`: the first 16 bytes, encoded as 32 hexadecimal characters.
- `korri/nearby/passphrase/v1`: the first 30 bytes, encoded as 60 hexadecimal
  characters. This is a WPA personal passphrase, not a raw PSK.

Both devices compute these values. No signal exchanges them. korrid does not
save or log them. NetworkManager holds the volatile connection in memory.
The hidden SSID is not a privacy boundary: directed Wi-Fi probes reveal it.
They do not reveal the separately derived passphrase.

## Lifecycle

The network-owned task reads the existing federation snapshot. It considers
only authorized members in `Failed`. If several are eligible, it tries the
lowest public key first. Backoff lets it consider the others. It starts an
attempt only when
NetworkManager and the existing Linux route reader both report no IPv4 or
IPv6 default route. A failed route read blocks the attempt. A directed scan
supplies the hidden SSID. The task waits for `LastScan` to advance before it
reads the access points. An existing beacon takes precedence over the starter
rule. Otherwise only the device with the lower public key starts an AP.
The other device keeps scanning. Peer power is unknown, so power does not
select the starter.

The AP uses the probe's 5 GHz channel 36. Both ends use `wpa-psk`, static
addresses, no gateway, no DNS and no IPv6. `AddAndActivateConnection2` uses
`persist=volatile`, and `connection.autoconnect` is false. The normal local
network controls share the radio operation lock with this task. One device
holds at most one nearby link at a time. A healthy link lasts until stop or
server shutdown; this chunk does not bind its lifetime to a live session.

Before activation, the task asks NetworkManager for a checkpoint of the Wi-Fi
device with `DELETE_NEW_CONNECTIONS` and a 120 s rollback timeout. It retains
the checkpoint for the entire link. Every 30 s it checks active state and
refreshes the timeout. It never destroys a checkpoint to confirm activation.
Stop, activation failure and shutdown roll back the checkpoint, then destroy
it. Rollback restores the exact previous home activation and removes the new
connection. A killed korrid cannot cancel NetworkManager's deadline.

At startup, a leftover `korri-nearby` volatile connection causes rollback of
its device's checkpoint. The task also removes any leftover nearby profile.
It does not delete the home profile. An expired checkpoint is already restored
by NetworkManager; stopping it again succeeds.

## Bounds chosen for this implementation

| Operation | Bound |
| --- | --- |
| One attempt, including scan and activation | 75 s |
| One directed scan | 25 s |
| One control call or restoration | 15 s |
| No successful signed peer contact after activation, or lost contact | 75 s |
| Trigger poll | 5 s |
| Retry delay | 30, 60, 120, 240, then 300 s |
| Independent crash rollback | 120 s since the last refresh |

The existing LAN calls, through federation's `Ready` state, prove peer
contact. An AP with no reachable peer does not keep the device away from its
home network indefinitely. A successful peer contact clears this deadline.
A later loss of contact starts it again. No peer state is written here.

`nearby_peer_ip(device_public_key)` exposes `.2` to the starter and `.1` to
the other device while the Wi-Fi connection is active. It returns `None`
before activation, during restoration, for another peer and after an expired
local lease. The connection worker adds this IP with the peer's existing
korrid port to the LAN candidate race. No new peer RPC is needed. There is
no Bluetooth path, so the stream stays on Wi-Fi.

## Remaining hardware checks

The tests exercise real zbus serialization and a controlled NetworkManager
D-Bus service. They do not qualify the hardware. Check checkpoint rollback,
permissions, fresh hidden discovery, actual signed peer contact, home
restoration after a killed korrid and stream play on the Odin and Mini.
The R36T Max's AP support is still unverified. Channel 36 and the fixed private
subnet can fail under other regulatory or route conditions. The Hardware test
stage must check these limits. No device was touched during this implementation.
