---
status: proposed (ready for review)
---

# The shell reads a stored image through the RPC, and shows it when it arrives

**Context, 2026-11-11 (issue #129, Velvet epic #425).** korrid stores cover
art (`services/korrid/src/game_assets.rs`), but nothing handed it to a
surface: `SurfaceGame.coverArtUrl` was never set, and the shell loaded only
`data:` URLs and package files. No surface on any Linux device showed game
art. The owner chose to build the art chain before Velvet ("art first").

**Owner, 2026-11-11.** A first version served the bytes on
`GET /game-assets/<asset id>`. The owner's answer: "I thought we'd used our
RPC, not traditional REST." This record is the corrected decision.

## Decision

- A surface host reads a stored image with a tagged request,
  `app.game-assets.object { assetId }`. It posts it to the surface host
  router's object route with its capability, and the answer is the image's
  bytes, not a JSON outcome.
- This is the pattern `main` already uses for bytes. `app.saves.object` and
  the release object are `RpcRequest` variants answered with bytes on the
  peer's object routes (`saves/peer/object_route.rs`: "this file defines no
  new envelope"). The local route is their twin.
- The request passes the same checks as every `/rpc` request:
  `SurfaceHostAccess::authorize` and the permission table. Every surface host
  permission may read an image, because every one may read the model that
  names it. A peer may not (`LocalSurfaceHostOnly`). `/rpc` never answers it.
- korrid answers only for an image an assignment record names, by its asset
  id alone (`<sha256>.<png|jpg|webp>`). The rule comes from
  `KorriGameAssetPathHandler` at `android-final`.
- `SurfaceGame.coverArtUrl` names the image as `/game-assets/<asset id>`, the
  form it had for the Android WebView. It is a reference, not a route, and
  carries no host, port or capability (native surfaces contract, section 2).
  A surface hands it back unchanged as `{ $image: url }`.
- The shell accepts exactly that form and asks its own korrid for that asset
  id. A model cannot make the shell ask for anything else.
- The shell reads and decodes a stored image on its own thread. The view
  shows an empty image at once, and the shell sets the image where the view
  names it when the pixels arrive. An image that cannot be loaded stays
  empty. It never fails a view, so it never unloads a surface (amendment A5).

## Considered options

- **A `GET` route by path.** ADR 0032, section 6, proposed it for the web
  portal, where an `<img>` needs a URL. Rejected: no browser runs on a Linux
  device since 2026-11-01. A path route is a second way into korrid beside
  the RPC, with its own authorization code outside the permission table, and
  it cannot cross the peer wire.
- **Base64 in a JSON outcome on `/rpc`**, as `system.plugins.archive.chunk`
  carries bytes. Rejected: `main` already answers objects with bytes, and a
  third more bytes buys nothing.
- **A `data:` URL in the model.** Rejected: korrid pushes the whole model on
  every change, so 200 covers would cross the socket each time.
- **Decode where the view is applied**, as for `data:` URLs. Rejected: a
  cover's decode is longer than a frame on the handhelds, and amendment A3
  allows no frame over 16.7 ms.

## Consequences

- ADR 0032, section 6, is replaced by this record for the native shell.
- A cover for a game on another device can use the same request across the
  peer wire later. Today only a local game's cover is published.
- A surface cannot tell an image that is still loading from one that failed.
  It knows only that Korri has a cover (`coverArtUrl` is present).
- The object route serves cover art only. Save screenshots (#126, the other
  half of #129) need their own rule for which objects a caller may read.
- The shell keeps at most 24 MiB of decoded pixels for a surface. A 512×512
  tile is 1 MiB decoded, so about 24 fit. A surface that shows more large
  images at once will load some of them again as it scrolls. Serving an image
  at the size a surface draws it is not decided here.
