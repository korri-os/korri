"""The plain text of an HTML notice, for the release APK's notices
(notices.nix, #434 D12). Rust ships the licences of its standard library
only as COPYRIGHT-library.html; the notices are one plain text.

Text inside <pre> (the licence texts) is kept as it is. Elsewhere, runs of
white space become one space, block elements start new lines, and runs of
blank lines become one.
"""

import html.parser
import re
import sys

BLOCKS = {"p", "div", "h1", "h2", "h3", "h4", "li", "ul", "br", "details", "summary"}


class Notice(html.parser.HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        # (text, inside <pre>) in document order.
        self.parts: list[tuple[str, bool]] = []
        self.pre = 0
        # The <title> repeats the first heading.
        self.title = False

    def handle_starttag(self, tag, attrs):
        if tag == "title":
            self.title = True
        if tag == "pre":
            self.pre += 1
        if tag == "pre" or tag in BLOCKS:
            self.parts.append(("\n", False))

    def handle_endtag(self, tag):
        if tag == "title":
            self.title = False
        if tag == "pre":
            self.pre -= 1
        if tag == "pre" or tag in BLOCKS:
            self.parts.append(("\n", False))

    def handle_data(self, data):
        if self.title:
            return
        if self.pre:
            self.parts.append((data, True))
        else:
            self.parts.append((re.sub(r"\s+", " ", data), False))


def text(document: str) -> str:
    notice = Notice()
    notice.feed(document)
    notice.close()
    # Runs of text outside <pre>, joined first so that inline elements keep
    # their spaces, then without indentation of their own.
    runs: list[tuple[str, bool]] = []
    for part, pre in notice.parts:
        if runs and runs[-1][1] == pre:
            runs[-1] = (runs[-1][0] + part, pre)
        else:
            runs.append((part, pre))
    out = []
    for run, pre in runs:
        if pre:
            out.append(run)
        else:
            out.append("\n".join(line.strip() for line in run.split("\n")))
    lines = [line.rstrip() for line in "".join(out).split("\n")]
    kept: list[str] = []
    for line in lines:
        if line or (kept and kept[-1]):
            kept.append(line)
    return "\n".join(kept).strip() + "\n"


if __name__ == "__main__":
    with open(sys.argv[1], encoding="utf-8") as source:
        sys.stdout.write(text(source.read()))
