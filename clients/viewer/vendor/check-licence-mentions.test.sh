#!/usr/bin/env bash
# Tests of check-licence-mentions.sh (#521) on a small copy of the trees'
# layout. Both notices' builds run them before the check itself
# (../notices.nix, ../../android-app/notices.nix).
# Usage: check-licence-mentions.test.sh
set -euo pipefail
export LC_ALL=C
here=$(cd "$(dirname "$0")" && pwd)
check=$here/check-licence-mentions.sh
src=nightjar/protocol/client/src
failures=0

# A tree with one recorded port under MIT, one recorded GPL-3.0 line, the two
# licence files that the check skips, a crate's licence, and a notices file
# with their part and crate.
setup() {
  root=$(mktemp -d)
  mkdir -p "$root/$src" "$root/nightjar/protocol/vendor/enet" "$root/rubylight-native-api"
  echo '//! Ported from nanors (MIT, Joseph Calderon).' > "$root/$src/fec.rs"
  echo '//! Ported from moonlight-common-c (GPL-3.0, Cameron Gutman).' > "$root/$src/audio.rs"
  echo 'fn main() {}' > "$root/$src/main.rs"
  echo 'Copyright (C) 2007 Free Software Foundation' > "$root/nightjar/LICENSE.txt"
  echo 'Copyright (C) 2007 Free Software Foundation' > "$root/rubylight-native-api/LICENSE"
  echo 'Copyright (c) 2002 Lee Salzman' > "$root/nightjar/protocol/vendor/enet/LICENSE"
  {
    echo '# FILE<TAB>PART<TAB>LINE'
    printf '%s\t%s\t%s\n' "$src/fec.rs" nanors '//! Ported from nanors (MIT, Joseph Calderon).'
    printf '%s\t%s\t%s\n' "$src/audio.rs" 'nightjar-core' '//! Ported from moonlight-common-c (GPL-3.0, Cameron Gutman).'
    printf '%s\t%s\t%s\n' nightjar/protocol/vendor/enet/LICENSE crate:enet 'Copyright (c) 2002 Lee Salzman'
  } > "$root/licence-mentions.txt"
  printf 'nanors\nnightjar-core\n\n  enet 1.0.0\n' > "$root/NOTICES.txt"
}

# expect pass NAME, or expect fail NAME REASON: the check's exit status, and
# for a failure the exact line it prints to say why.
expect() {
  local want=$1 name=$2 reason=${3:-} got=0
  bash "$check" "$root" "$root/NOTICES.txt" 2> "$root/errors" || got=$?
  if [ "$want" = pass ] && { [ "$got" -ne 0 ] || [ -s "$root/errors" ]; }; then
    echo "FAIL: $name: expected a pass, got exit $got" >&2
    sed 's/^/  /' "$root/errors" >&2
    failures=$((failures + 1))
  elif [ "$want" = fail ] && { [ "$got" -eq 0 ] || ! grep -qxF -- "$reason" "$root/errors"; }; then
    echo "FAIL: $name: expected exit 1 with: $reason; got exit $got with:" >&2
    sed 's/^/  /' "$root/errors" >&2
    failures=$((failures + 1))
  else
    echo "ok: $name"
  fi
  rm -rf "$root"
}

# A new file whose only line is LINE fails, and the check names the line.
new_line() {
  setup
  printf '%s\n' "$1" > "$root/$src/new.rs"
  expect fail "a new line: $1" "notices: not in licence-mentions.txt: $src/new.rs: $1"
}

setup
expect pass "the recorded tree, with the two licence files skipped"

# Licence names, with the version as the review's bypasses wrote it.
for line in '(GPL-2.0-only, X)' '(GPL v2, X)' '(GPL 2.0, X)' '(GNU GPL version 2 only)' 'GPLv2 code' \
  '(GPL-3.0, X)' '(LGPL v2.1, X)' '(AGPL-3.0, X)' '(AGPL v3, X)' 'AGPLv3 code' '(EUPL-1.2, X)' \
  '(BSD-3-Clause, X)' 'Apache2 code' '(Apache-2.0, X)' '(MPL-2.0, X)' 'under zlib terms'; do
  new_line "//! Ported from foo $line."
done
# "licenc" and "licens" anywhere.
for line in 'sublicense it' 'relicensed by X' 'unlicensed code' 'the licensor' 'the licensee' \
  'LicenseRef-Foo' 'let license_file = 1;' 'licensed under the apache license' 'under the Foo Public License' \
  'Licence: Foo'; do
  new_line "// $line"
done
# Copyright and the public domain.
for line in 'Copyright 2020 X' 'copyrighted by X' 'all copyrights reserved' 'COPYRIGHT HOLDERS' \
  'in the public domain' 'public-domain code' 'publicdomain' '© 2020 X' 'X©2020' '(c) 2020 X' \
  '(C)2020 X' 'Foo(c) 2020 X' 'Foo (c) X'; do
  new_line "// $line"
done

setup
mkdir -p "$root/rubylight-native-api/rust/core/foo"
printf '[package]\nname = "foo"\n' > "$root/rubylight-native-api/rust/core/foo/Cargo.toml"
echo 'license = "Foo-1.0"' >> "$root/rubylight-native-api/rust/core/foo/Cargo.toml"
expect fail "a new crate manifest's licence" \
  'notices: not in licence-mentions.txt: rubylight-native-api/rust/core/foo/Cargo.toml: license = "Foo-1.0"'

setup
printf 'Copyright (c) 2020 X\nRedistribution and use in source and binary forms, with or without\n' > "$root/$src/LICENSE-BSD"
expect fail "a LICENSE-BSD in a subfolder" "notices: not in licence-mentions.txt: $src/LICENSE-BSD: Copyright (c) 2020 X"

setup
mkdir -p "$root/rubylight-native-api/rust"
echo 'Copyright (C) 2007 Free Software Foundation' > "$root/rubylight-native-api/rust/LICENSE"
expect fail "a LICENSE file other than the two skipped ones" \
  'notices: not in licence-mentions.txt: rubylight-native-api/rust/LICENSE: Copyright (C) 2007 Free Software Foundation'

setup
echo '//! Also from bar (BSD-3-Clause, X).' >> "$root/$src/fec.rs"
expect fail "a new line in an already-listed file" "notices: not in licence-mentions.txt: $src/fec.rs: //! Also from bar (BSD-3-Clause, X)."

setup
echo '//! Ported from moonlight-common-c (GPL-3.0, Cameron Gutman).' >> "$root/$src/audio.rs"
expect fail "a second copy of a recorded line" \
  "notices: not in licence-mentions.txt: $src/audio.rs: //! Ported from moonlight-common-c (GPL-3.0, Cameron Gutman)."

setup
echo '//! Written for Nightjar.' > "$root/$src/fec.rs"
expect fail "a recorded line that is gone" \
  "notices: licence-mentions.txt records a line that $src/fec.rs no longer has: //! Ported from nanors (MIT, Joseph Calderon)."

setup
printf 'nightjar-core\n\n  enet 1.0.0\n' > "$root/NOTICES.txt"
expect fail "a part that the notices lack" "notices: no part nanors for $src/fec.rs"

setup
printf 'nanors\nnightjar-core\n' > "$root/NOTICES.txt"
expect fail "a crate that the notices lack" "notices: no crate enet for nightjar/protocol/vendor/enet/LICENSE"

setup
echo 'fn mitigate() {} // MITM, Zlibs, Boosted, apachectl, #[repr(C)], push(c)' > "$root/$src/main.rs"
expect pass "names inside longer words, and (c) in code"

if [ "$failures" -ne 0 ]; then
  echo "check-licence-mentions: $failures test(s) failed" >&2
  exit 1
fi
