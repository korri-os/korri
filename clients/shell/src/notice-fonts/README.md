# The notice's font

`Inter-VariableFont.ttf` is byte-for-byte the file Slint 1.18.1 embeds as its
fallback font (`i-slint-common-1.18.1/sharedfontique/Inter-VariableFont.ttf`,
SHA-256 `c333e1de6d6241530a473f480cd1f59a2578cd509825828b89a751c09fb9b61c`),
the same file as `clients/slint-spike/ui/fonts/`. It is not cut down: the
licence's Reserved Font Name forbids a modified copy named Inter.

The shell embeds it and registers it once per thread, the first time it shows
its notice (`src/notice.rs`), so the notice draws on a device with no system
fonts.

Licence: SIL Open Font License 1.1 with Reserved Font Name. `OFL.txt` is the
full text; `Inter-VariableFont.ttf.license` is Slint's SPDX record, unchanged.
