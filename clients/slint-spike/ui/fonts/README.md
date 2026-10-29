# Embedded font (THROWAWAY SPIKE)

`Inter-VariableFont.ttf` is byte-for-byte the file Slint 1.18.1 itself embeds
as its fallback font on targets without system fonts
(`i-slint-common-1.18.1/sharedfontique/Inter-VariableFont.ttf`,
SHA-256 `c333e1de6d6241530a473f480cd1f59a2578cd509825828b89a751c09fb9b61c`).
Slint pinned its optical-size axis and dropped Private Use Area code points;
see the test `embedded_fallback_font_is_minimal` in that file.

The inspector imports it in `ui/inspector.slint` with Slint's documented
`import "….ttf";` mechanism, so text renders on the LG TV without fontconfig
or any TV font. The desktop build uses the same font, so both builds show the
same view.

Licence: SIL Open Font License 1.1 with Reserved Font Name.
`Inter-VariableFont.ttf.license` is Slint's SPDX record for the file, kept
unchanged. `OFL.txt` is the full licence text from
`https://raw.githubusercontent.com/rsms/inter/v4.1/LICENSE.txt`.
