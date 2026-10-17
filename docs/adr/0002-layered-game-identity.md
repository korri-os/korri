# Game identity is layered: provider reference, optional hash lists, then a link

Devices discover content on their own schedules, and the portal joins copies from several devices by identity. A file release is identified by its bytes, so identical files join on their own. A game is identified, in order, by a provider reference (a store or plugin id), by a hash list that maps several dumps to one game when some plugin contributes one, or by a link. Korri must work fully with no hash list at all: none is assumed to exist. A link is made by one person but holds for every person and device in the federation, because a device has many people and a federation has many devices. Content none of these covers stays a separate game until someone links it. Identifiers minted by one device never join copies across devices.

## Considered Options

- Provider references only: every ROM, indie game and homebrew would stay per device.
- Provider references and a hash database built into Korri: puts system-specific data in core, and core would depend on a database that may never exist.
- Links scoped to one person: two people sharing a device would see the same games split.
- Release hash as the game key: two devices holding different versions of one game would show two games.
