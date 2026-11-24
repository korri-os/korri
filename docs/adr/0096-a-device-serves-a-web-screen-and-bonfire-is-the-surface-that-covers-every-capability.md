---
status: accepted
---

# A device serves a web screen, and Bonfire is the surface that covers every capability

**Owner decisions, 2026-11-23.** aka runs only the server part (ADR 0051), so
no surface can appear on it. Steam sign-in there needed a terminal script
(issue #603). The owner wants a person to open a URL on a device after an
install, as with Sunshine, and manage that device there. The owner also wants
no person ever stuck in a surface that lacks a capability.

This amends the 2026-11-01 rule that no browser, web server or second host
runs on a Linux device. A device still runs no browser. korrid may now serve a
**web screen** (`CONTEXT.md`) to a browser on another machine.

**Changed by ADR 0097 on the same day:** where this record says "Bonfire", read
"the management surface". The owner moved the name Bonfire to a look for a
different surface, and the management surface has no name yet.

## Decision

- A **web screen** is a surface host that runs in a browser. It carries the
  same treaty as the shell: one `SurfaceModel` in, `SurfaceRequest`s out. It
  loads any surface package. No package is a special case in it.
- **Bonfire** is a surface package under that treaty, like Pico, Zone and
  Velvet. One Slint codebase runs in the shell and in the web screen. Bonfire
  is the surface a web screen shows first.
- Bonfire is the surface that grows to cover every capability in the treaty,
  plainly. Until it does, it is the one surface that does not meet the
  completeness rule in `docs/design/surfaces/CAPABILITIES.md`.
- Each capability in Bonfire is a self-contained view that lays out at any
  container size. A later host can then show one view beside or above another
  surface. That later design is not decided.
- korrid serves the web screen itself, on a second listener with its own
  router. The loopback listener that the shell uses, and its loopback guard,
  do not change. No separate web service runs.
- The second listener is reachable from the local network and uses HTTPS with
  a certificate that the device makes itself. A person accepts a browser
  warning once for each device.
- On first visit a person enters a one-time code read from the device. The
  browser then holds its own session, which korrid can revoke. The device's
  RPC capability never reaches a browser.
- Actions in a web screen apply to the device that serves it. The treaty gains
  a list of the other devices; a surface does not act on a peer.
- The web screen is on where a device runs only the server part. Where a
  device also runs the client part it is off, with an option to turn it on.
- A physical keyboard types into a surface's text entry as it does in any
  program on a computer: cursor, selection, copy and paste. For text entry
  only, a package may use Slint's own text field and the host lets keys
  through to it. The package still draws the field to fit its own design, and
  its logic sees every change to the text and decides which field is open.
  This amends, for text entry only, the rules that a surface receives no keys
  and that a package's logic holds all focus. A device with only a controller
  keeps the package's on-screen keyboard.

## The first slice

A person at another machine opens aka's web screen in a desktop browser and in
a phone browser, and can do three things: join the device to an existing
federation (ADR 0070), make an account sign-in (ADR 0089), and see the devices
of the federation. They land in this order after the shared base: account
sign-in, the device overview, then join. A throwaway korrid proves the join;
aka keeps its identity. The shell does not ship Bonfire yet, but the package
check must load and draw it in the shell, so that it stays a real surface.
A new install path for a machine outside Korri's NixOS
configuration, creating a new federation, plugin management and removing a
device are not in it.

## Considered options

- **A standalone Slint program that calls korrid's RPC.** Fastest to a working
  screen, because the CLI's calls already cover management. Rejected: it is not
  a surface, the shell cannot load it, and it would be a second contract.
- **A separate web service next to korrid.** korrid would stay loopback-only
  and the web path could fail alone. Rejected: that service would hold the one
  capability that can do everything and decide for the browser.
- **The first visitor sets a password.** No terminal needed. Rejected: whoever
  on the network opens the page first owns the device, and a password is a
  second identity beside the federation's keys.
- **Plain HTTP on the local network.** No browser warning. Rejected: the
  session crosses the network in clear text, and a browser withholds the
  clipboard and Web Crypto from such a page.
- **One screen that acts on any peer.** The real goal. Deferred: peer settings
  and peer sign-in are not in the treaty or the peer RPC.

## Consequences

- Slint's documentation does not recommend its browser build for
  general-purpose web applications. It names "applications where the web is
  not the primary platform, but a consistent UI is still needed" as a suitable
  use, which is this case. In a browser Bonfire has no screen reader support
  and does not behave like HTML. The owner accepted this.
- The web screen is a new host. The shell's carrying code depends on QuickJS,
  blocking sockets and fontconfig, which do not build for a browser.
- korrid has one shared capability today and no session for a client. The
  browser session is new.
- korrid listens to the local network for web clients for the first time. A
  fault in that path is a fault in the korrid process.
- Reach beyond the local network, through the federation, is wanted later so
  that a person is less likely to be locked out. A browser cannot speak iroh
  (ADR 0095), so this needs its own decision.
