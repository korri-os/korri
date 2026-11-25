# InputPlumber source reattach regression

Pinned source: ShadowBlip/InputPlumber v0.75.2. The source and Cargo hashes
remain in `../nix/inputplumber-package.nix`.

## Failure mechanism

Upstream `src/input/composite_device/mod.rs:372-384` handles both a manager
removal and a source-task completion by calling `on_source_device_removed`.
The completion contains only `DeviceInfo`. Its `eventN`/`hidrawN` ID identifies
a reusable slot, not the task that owned that slot.

`on_source_device_removed` (upstream lines 1588-1685) removes the source paths,
capabilities and occupied IDs, but neither stops the source client nor joins
its task. A still-running evdev driver can therefore retain its exclusive
`EVIOGRAB` after its slot is advertised as vacant. A delayed completion from
the old task can also clear a replacement's bookkeeping. Another add then
opens the same live evdev device and fails at `GamepadEventDevice::new`'s
`device.grab()` (`src/input/source/evdev/gamepad.rs:50`) with EBUSY.

A real kernel disconnect normally makes `fetch_events()` return ENODEV, so
simple unplug/replug tests alone do not prove this fd-lifetime bug. The VM
also removes and recreates a live USB device's devnode. This forces the same
ownership boundary without relying on scheduler timing. Without the teardown
patch, the new VM fails with `Failed to add source device: "Device or resource
busy (os error 16)"` and an empty persistent composite. With the patch, the
provider releases the old grab, reuses the composite, and captures input.

None of Korri's previous patches changes this source-task lifecycle. In
particular, ignore-before-reconnect changes admission ordering, and
lazy-attributes changes matching reads. Neither filters removal events.

## Proof

```sh
nix build .#checks.x86_64-linux.inputplumber-reattach-vm --no-link -L
nix build .#checks.x86_64-linux.inputplumber-korri-package --no-link -L
nix build .#checks.x86_64-linux.inputplumber-adoption-vm --no-link -L
```

The isolated guest binds a real configfs USB HID gadget to `dummy_hcd`. It
uses the Kishi's 1532:0712 identity, with an evdev gamepad and hidraw interface.
It does not claim to copy the complete Kishi report descriptor or its four
USB interfaces. Unlike the adoption matrix, this test needs no virtual-device
admission override.

The test covers live devnode replay plus ten physical USB detach/bounce/attach
cycles, including detach while a button is held. It resets the surviving
composite to the upstream default profile while empty and requires inputd to
apply the immutable Korri profile on reattach. It checks persistent composite
and target paths, and button press/release frames from inputd's real
unprivileged `CaptureRuntime`. These frames traverse USB HID, evdev, InputPlumber's
normalized target and inputd's validated capture. It checks service invocation
IDs, PIDs and restart counts. No daemon restart is part of recovery.

The example driver is not korrid or the shell. It runs the production profile
reconciler, capture provider, topology fast path and capture runtime, then
prints the native producer frames instead of sending them to korrid. The VM
does not claim a complete shell-navigation or physical-tablet acceptance test.

## Hardware evidence and tablet workaround

The saved tablet journal has two different failures. In the manual-host boot,
the original source returned ENODEV, then InputPlumber added the same source
path twice and the second add failed with EBUSY. The lifecycle bug above is
consistent with that ordering, but the info-level journal cannot establish the
exact callback order. In the automatic-host boot, source re-add succeeded and
continued reading, but shell navigation did not return. No EBUSY was logged
in that second failure. Do not describe it as a proven instance of the first.

The tablet's provider-restart workaround was removed on 2026-11-25 and
restored the same day: the separate automatic-host failure (source re-added
into the surviving composite, no inputd profile event, dead input, no EBUSY)
reproduced on battery after repeated USB detach/re-add. Evidence and the open
question are in `nix/devices/tabs6/usb-role/typec-kernel.md`. The role watcher
restarts the gadget and both providers on every kernel host-to-device change.

No test here proves that failure is fixed.
