#!/usr/bin/env bash
# The vendored trees' licence mentions against the notices (#391, #521).
# Usage: check-licence-mentions.sh VENDOR NOTICES
#
# Every file of VENDOR/nightjar and VENDOR/rubylight-native-api that names a
# licence, a copyright or a notice must be in VENDOR/licence-mentions.txt;
# every line there must still name such a file; and each part that a line
# names must be a title line of NOTICES. The words are whole words, in any
# file but the trees' own LICENSE files and Cargo.lock.
set -euo pipefail
vendor=$1
notices=$2
words='MIT|BSD|BSD-[0-9]-Clause|Apache|Apache-2\.0|ISC|Zlib|zlib|MPL|MPL-2\.0|Unlicense|CC0|CC0-1\.0|BSL|BSL-1\.0|Boost|LGPL|LGPL-[0-9.]+|WTFPL|0BSD|OFL|EPL|CDDL|Artistic|Unicode-[0-9.]+|Unicode-DFS-[0-9]+|SSLeay|public domain|Public Domain|copyright|Copyright|COPYRIGHT|\(c\)|\(C\)|©|licen[cs]ed under|SPDX-License-Identifier'
found=$(cd "$vendor" && grep -rIlE "(^|[^A-Za-z0-9_])($words)([^A-Za-z0-9_]|$)" nightjar rubylight-native-api \
  --exclude=Cargo.lock --exclude='LICENSE*' | grep -v '^nightjar/protocol/vendor/rusty_enet/' | sort || true)
listed=$(grep -v '^#' "$vendor/licence-mentions.txt" | awk 'NF { print $1 }' | sort)
failed=0
for file in $(comm -23 <(echo "$found") <(echo "$listed")); do
  echo "notices: $file names a licence or a copyright; read it and list it in licence-mentions.txt" >&2
  failed=1
done
for file in $(comm -13 <(echo "$found") <(echo "$listed")); do
  echo "notices: licence-mentions.txt lists $file, which names no licence or copyright any more" >&2
  failed=1
done
while read -r file part; do
  case "$file" in '' | '#'*) continue ;; esac
  if [ "$part" != - ] && ! grep -qx "$part" "$notices"; then
    echo "notices: no part $part for $file" >&2
    failed=1
  fi
done < "$vendor/licence-mentions.txt"
exit "$failed"
