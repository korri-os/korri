# Migrate Odin and retire the kiosk launcher

Status: ready-for-agent
Blocked by: 01

## What to build

Make Odin boot through the same private `window.KorriRpc` portal path as every other product device. Remove the old kiosk launcher instead of keeping an adapter or second credential path.

## Acceptance criteria

- [ ] Odin imports the product module and supplies only its screen, GPU, controls, kernel, device-tree, boot facts, and permitted recorded limits.
- [ ] The old kiosk launcher crate and NixOS module are removed with their package, module, check registrations, and obsolete references.
- [ ] No internal adapter, `/runtime.json` fallback, second credential-delivery path, URL credential, browser-storage value, log value, or static runtime asset remains.
- [ ] The independent Chromium transparency patch and native pixel tests remain unless separate evidence authorizes their removal.
- [ ] The portal obtains its Wayland connection from the compositor and remains usable when the streaming host is absent.
- [ ] The product owns browser startup, security, actual Wayland app identity, and game-return exclusion. The old bootstrap URL identity is not reused.
- [ ] Shared browser tests prove that both private binding methods are consumed before mount, that startup fails when either is not consumed, and that the portal completes an authenticated call to its local korrid.
- [ ] On Odin hardware, normal boot reaches a correctly rendered portal with touch and controller input; launch, leave, return, and end target the exact game; browser, compositor, and korrid restart recovery work; and the portal still works with the streaming host removed.
- [ ] The Odin configuration passes the product check with no device exception.
- [ ] The migration adds no account migration, alias, fallback read, dual write, or compatibility branch.
