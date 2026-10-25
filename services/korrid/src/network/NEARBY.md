# Nearby link

The owner approved the nearby link contract on 2026-10-23. ADR 0026 defines
its purpose. The `spike/reach-nearby` probe at `90ab09ea8` supplies the native
NetworkManager settings. It also supplies the addresses, `10.77.77.1/24` and
`10.77.77.2/24`. This code adds no persisted Korri schema.

## Key agreement

`DeviceIdentity::nearby_key_material` uses
`nostr::nips::nip44::v2::ConversationKey::derive`. Identity's existing NIP-44
encryption uses this key agreement. Identity keeps the conversation key
private. HMAC-SHA256 under that key makes nearby-only material. Its inputs
include `korri/nearby/key-material/v1\0` and the sorted, fixed-length public
keys.

Separate HMAC-SHA256 labels make the SSID and passphrase:

- `korri/nearby/ssid/v1`: korrid encodes the first 16 bytes as 32 hexadecimal
  characters.
- `korri/nearby/passphrase/v1`: korrid encodes the first 30 bytes as 60
  hexadecimal characters. korrid uses this value as a WPA personal passphrase.
  It does not use a raw PSK.

Both devices compute these values. The devices do not exchange them through
a signal. korrid does not save or log them. NetworkManager holds the volatile
connection in memory. The hidden SSID does not provide a privacy boundary.
Directed Wi-Fi probes reveal the hidden SSID. They do not reveal the
passphrase.

## Lifecycle

The network-owned task reads the existing federation snapshot. It considers
only authorized members in `Failed`. If several are eligible, it tries the
lowest public key first. Backoff lets it consider the others.

The task scans for a member's hidden network even when a default route
exists. Each attempt requests one directed scan for the derived SSID. The
task waits for `LastScan` to advance before it reads the access points. If
the task finds that network, korrid connects to it regardless of default
routes. The derived SSID identifies the nearby peer's network.

If the scan finds no network, only the device with the lower public key can
start an AP. Starting also requires both NetworkManager and the existing
Linux route reader to report no IPv4 or IPv6 default route. NetworkManager
must also have finished its startup connection attempts and must not be
connecting: at boot, a missing route only means home Wi-Fi is not up yet.
A failed route
read blocks AP creation. It does not block scanning or connecting. Devices
cannot read each other's power state. Power does not select the starter.

The task makes at most one scan per attempt. Each scan has a 25 s limit.
The task waits 30 to 300 s after an attempt before it retries that peer.
Backoff applies even when the scan finds no network. This limit prevents
constant scans at home. The device with the higher key also uses this
backoff while it waits for the other device's network.

The AP uses the probe's 5 GHz channel 36. Both ends use `wpa-psk`, static
addresses, no gateway, no DNS and no IPv6. `AddAndActivateConnection2` uses
`persist=volatile`, and `connection.autoconnect` is false. The normal local
network controls share the radio operation lock with this task. One device
holds at most one nearby link at a time. A healthy link lasts until stop or
server shutdown. This chunk does not bind its lifetime to a live session.

Before activation, korrid asks NetworkManager for a checkpoint of the Wi-Fi
device. NetworkManager sets `DELETE_NEW_CONNECTIONS` and a 120 s rollback
timeout. korrid retains the checkpoint for the entire link. Every 30 s,
korrid checks active state and refreshes the timeout. korrid never destroys
a checkpoint to confirm activation.

On stop, activation failure or shutdown, korrid asks NetworkManager to roll
back the checkpoint. NetworkManager restores the exact previous home
activation. It also removes the new connection. korrid then destroys the
checkpoint if NetworkManager still holds it. A korrid crash cannot cancel
NetworkManager's deadline.

At startup, korrid checks for a leftover `korri-nearby` volatile connection.
If korrid finds one, it asks NetworkManager to roll back that device's
checkpoint. korrid also removes any leftover nearby profile. It does not
delete the home profile. If NetworkManager already removed an expired
checkpoint, korrid accepts another stop call without repeating restoration.

## Implementation bounds

| Operation | Bound |
| --- | --- |
| One attempt, including scan and activation | 75 s |
| One directed scan per attempt | 25 s |
| One control call or restoration | 15 s |
| No successful signed peer contact after activation, or lost contact | 75 s |
| Trigger poll | 5 s |
| Retry delay | 30, 60, 120, 240, then 300 s |
| Independent crash rollback | 120 s since the last refresh |

The existing LAN calls prove peer contact through federation's `Ready`
state. An AP with no reachable peer does not keep the device away from its
home network indefinitely. A successful peer contact clears this deadline.
A later loss of contact starts it again. The nearby task does not write
peer state.

`nearby_peer_ip(device_public_key)` exposes `.2` to the starter and `.1` to
the other device while the Wi-Fi connection is active. It returns `None`
before activation, during restoration, for another peer and after an expired
local lease. The connection worker adds this IP to the LAN candidate race.
It uses the peer's existing korrid port. This code adds no peer RPC.
This code adds no Bluetooth path. The stream uses Wi-Fi.

## Known limit

Two devices on the same captive or isolated network may both have a default
route. Neither device will start a nearby link in that case. Each device
still scans with backoff. It can connect if it finds the peer's nearby
network.

## Remaining hardware checks

The tests exercise real zbus serialization and a controlled NetworkManager
D-Bus service. They do not qualify the hardware. The Hardware test stage must
check checkpoint rollback and permissions. It must also check fresh hidden
discovery and actual signed peer contact. That stage must check home
restoration after a korrid crash and stream play on the Odin and Mini.
The probe did not verify the R36T Max's AP support. Channel 36 and the fixed
private subnet can fail under other regulatory or route conditions.
The Hardware test stage must check these limits. I did not access a device
during this implementation.
