# PICO-8 BBS

`@korri:pico8` searches the Lexaloffle BBS for PICO-8 cartridges and resolves
a post to its cart file. It is a port of legacy
`product/plugins/pico8/src/plugin.ts` (legacy `0e4cec9d`). `plugin.ts` performs
no effects: each handler returns the GET requests it needs, and korrid calls it
again with the responses (`services/korrid/SCRIPTING.md`, "Acquisition
providers"). korrid downloads the cart.

The plugin declares no system. `artifact.system` is `pico8`, which the
publisher's `@korri:fake08` plugin declares (`plugins/libretro/systems.nix`,
extensions `p8` and `p8.png`) and runs.

## Source

| Operation | Request | Read from the page |
|---|---|---|
| `claims.search` | `https://www.lexaloffle.com/bbs/?cat=7&sub=2&mode=carts&orderby=ts&search=<query>&max=50` | The `pdat=[...]` script array: post id, thread title (field 2; the cart label in field 22, often a file slug such as `violet`, is only a fallback), thumbnail, author, dates. The BBS's `[no posts found]` line is an empty result. A page with neither fails. |
| `claims.details` | `https://www.lexaloffle.com/bbs/?pid=<id>` | The first `href` to a `.p8.png`; the thread title (`og:title`), else the cart label beside "Cart File"; the author. |
| `artifact.resolve-download` | the claim's post page, unless the URL is already a cart file | `FinalDownload` of `https://www.lexaloffle.com/bbs/cposts/<folder>/<cart>.p8.png`, `image/png`. |

Changes from legacy, all forced by the current BBS:

- Cart files moved from numeric folders (`/bbs/cposts/1/15133.p8.png`) to the
  first two characters of the cart id (`/bbs/cposts/ce/celeste_classic_2-5.p8.png`).
  Legacy accepted only digits. Both layouts are accepted.
- The BBS answers an unknown post id with its front page and status 200.
  Legacy then invented a "PICO-8 Cart <id>" result. A post page must now carry
  a cart link or the post container `<div id=p<id>`.
- Legacy preferred the cart label as the title; it is often a file slug
  (`violet`, `farlandv2`, `ccbtt`), so the thread title wins now.
- Legacy read any page without the cart list as no results. That hides a
  markup change, so only `[no posts found]` is empty now.

## Licence and terms

Each BBS cart carries its author's licence on its post page. The acceptance
specimen is **Celeste Classic 2** by noel (post `86783`, cart
`celeste_classic_2-5.p8.png`, 40,610 bytes, SHA-256
`15b1f2639c8dd237ddb1176b1d9f311bb960082b780668e14c0f1d0fd8f8d96d`, measured
2026-10-20). Its post page states `License: CC4-BY-NC-SA`
(https://creativecommons.org/licenses/by-nc-sa/4.0/). Lexaloffle's terms of use
(https://www.lexaloffle.com/info.php?page=tos, read 2026-10-20) say that BBS
cartridges remain their authors' property, that uploading grants Lexaloffle
the right to distribute them, and ask people not to redistribute or remix a
cart without permission. Korri therefore fetches each cart from Lexaloffle onto
the person's own device and never redistributes it. No cart bytes are in this
repository.

## Checks

```sh
bun test plugins/pico8/plugin.test.ts
plugins/pico8/live-probe.ts            # live BBS, small GETs, prints timestamps
plugins/pico8/live-probe.ts --record plugins/pico8/fixtures
```

`fixtures/` holds verbatim line excerpts of live pages, written by the probe's
`--record` mode: the search script arrays, the BBS's empty answer, a current
and an old post page, and the front page served for an unknown post. The unit
tests read only these files.
