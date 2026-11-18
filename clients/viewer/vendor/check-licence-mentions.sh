#!/usr/bin/env bash
# The vendored trees' licence mentions against the notices (#391, #521).
# Usage: check-licence-mentions.sh VENDOR NOTICES
#
# It reads every line with one of its words, in any case, of every text
# file under VENDOR/nightjar and VENDOR/rubylight-native-api but
# nightjar/LICENSE.txt and rubylight-native-api/LICENSE. Its words:
# - anywhere in the line: "licenc" and "licens" (licence, license,
#   sublicense, relicensed, unlicensed, licensor, licensee, LicenseRef-,
#   license_file), "copyright" (copyrighted, copyrights), "public domain"
#   (also with "-", "_" or nothing between), "©", and "(c)" before a year
#   ("(c) 2020", "(c)2020");
# - with no letter, digit or underscore just before: "GPL", "LGPL" and
#   "AGPL" (so "GPL v2", "GPL 2.0", "GNU GPL version 2", "GPL-3.0",
#   "LGPLv2.1");
# - as whole words, with no letter, digit or underscore just before and none
#   just after or after a version glued on ("Apache2", "EUPLv1"): MIT, BSD,
#   0BSD, Apache, ISC, Zlib, MPL, CC0, CC-BY, BSL, Boost, EUPL, WTFPL, OFL,
#   EPL, CDDL, Artistic, Unicode-DFS, "Unicode-" and a digit, SSLeay, and
#   "(c)".
#
# It fails when such a line is not recorded word for word, with its file, in
# VENDOR/licence-mentions.txt (a new line in a listed file too); when a
# recorded line is no longer in its file; and when a recorded part is not in
# NOTICES: a part title is a line of its own there, and `crate:NAME` a line
# "  NAME VERSION" of cargo-about's crate lists.
#
# Not caught:
# - a port or a licence on a line with none of the words, such as "Ported
#   from foo" alone, or a licence name that is not a word here ("X11",
#   "NCSA", "Python-2.0", "PostgreSQL") without "licence" or "copyright";
# - a whole word with a letter, digit or underscore just before it, or a
#   letter or underscore just after it ("BSDish", "zlib_rs"); "GPL", "LGPL"
#   or "AGPL" with a letter, digit or underscore just before it ("nonGPL");
#   "(c)" with a letter, digit or underscore just before it and no year
#   after it ("Foo(c) Bar", "repr(C)");
# - a phrase or a word split across two lines ("public" and "domain",
#   "licen-" and "sed"), and a word in another language or spelled with
#   other characters ("Lizenz", "ⓒ", full-width letters);
# - binary files, which hold a NUL byte (grep -I skips them), and the names
#   of files;
# - text added to nightjar/LICENSE.txt or rubylight-native-api/LICENSE:
#   the notices' builds check only that each is the GPL version 3;
# - files outside nightjar/ and rubylight-native-api/;
# - a wrong part or crate: the script checks that it is in the notices, and
#   a person decides that it covers the line.
set -euo pipefail
# Bytes, so that each run reads and sorts the same way.
export LC_ALL=C
vendor=$1
notices=$2
list=$vendor/licence-mentions.txt
anywhere='licen[cs]|copyright|public[-_ ]?domain|©|\(c\) *[0-9]'
prefixed='GPL|LGPL|AGPL'
words='MIT|BSD|0BSD|Apache|ISC|Zlib|MPL|CC0|CC-BY|BSL|Boost|EUPL|WTFPL|OFL|EPL|CDDL|Artistic|Unicode-DFS|Unicode-[0-9]|SSLeay|\(c\)'
pattern="$anywhere|(^|[^A-Za-z0-9_])($prefixed)|(^|[^A-Za-z0-9_])($words)(v?[0-9]+(\\.[0-9]+)*)?([^A-Za-z0-9_]|$)"
tab=$(printf '\t')
# FILE<TAB>LINE for each matching line.
found=$(cd "$vendor" && { grep -rniIE "$pattern" nightjar rubylight-native-api || true; } |
  grep -v -e '^nightjar/LICENSE\.txt:' -e '^rubylight-native-api/LICENSE:' |
  sed -E "s/^([^:]*):[0-9]+:/\\1$tab/" | sort || true)
# The recorded lines: FILE<TAB>PART<TAB>LINE, less the comments and blanks.
recorded=$(grep -v -e '^#' -e '^$' "$list" | cut -f1,3- | sort || true)
failed=0
while IFS= read -r line; do
  [ -n "$line" ] || continue
  echo "notices: not in licence-mentions.txt: ${line%%"$tab"*}: ${line#*"$tab"}" >&2
  failed=1
done < <(comm -23 <(printf '%s\n' "$found") <(printf '%s\n' "$recorded"))
while IFS= read -r line; do
  [ -n "$line" ] || continue
  echo "notices: licence-mentions.txt records a line that ${line%%"$tab"*} no longer has: ${line#*"$tab"}" >&2
  failed=1
done < <(comm -13 <(printf '%s\n' "$found") <(printf '%s\n' "$recorded"))
while IFS="$tab" read -r file part _; do
  case "$file" in '' | '#'*) continue ;; esac
  case "$part" in
    -) ;;
    crate:*)
      grep -qE "^  ${part#crate:} [0-9]" "$notices" ||
        { echo "notices: no crate ${part#crate:} for $file" >&2; failed=1; }
      ;;
    *)
      grep -qx -- "$part" "$notices" ||
        { echo "notices: no part $part for $file" >&2; failed=1; }
      ;;
  esac
done < "$list"
exit "$failed"
