# The kernel modules whose coldplug events a system's udev rules can use, read
# from the rule files at build time. Output: $out, one module name per line.
#
# The build fails when a rule matches drivers, bus or subsystem events, or
# matches module events without naming each module exactly: the narrowed
# coldplug in udev-coldplug-package.nix would silently starve such a rule.
#
# Limits: only explicit SUBSYSTEM== matches are seen. A rule with no SUBSYSTEM
# key, or a SUBSYSTEM!= rule, also sees module events and is not checked.
# Rules a plugin writes under /run/udev/rules.d at runtime are not checked.
{
  pkgs,
  ruleDirs,
}:

pkgs.runCommand "korri-udev-coldplug-modules"
  {
    nativeBuildInputs = [ pkgs.gawk ];
    inherit ruleDirs;
  }
  ''
    set -euo pipefail
    files=()
    for dir in $ruleDirs; do
      for f in "$dir"/*.rules; do
        [ -e "$f" ] && files+=("$f")
      done
    done

    # One logical rule per line: join backslash continuations, keep the file
    # name for messages.
    : > rules.tsv
    for f in "''${files[@]}"; do
      awk -v file="$(basename "$f")" '
        { line = line $0 }
        /\\$/ { sub(/\\$/, "", line); next }
        { print file "\t" line; line = "" }
      ' "$f" >> rules.tsv
    done

    : > modules.txt
    awk -F'\t' '
      function glob_re(p,   r) {
        r = p
        gsub(/[.^$+(){}]/, "\\\\&", r)
        gsub(/\*/, ".*", r)
        gsub(/\?/, ".", r)
        return "^(" r ")$"
      }
      function fail(msg) { print "korri-udev-coldplug: " $1 ": " msg ": " $2 > "/dev/stderr"; bad = 1 }
      $2 ~ /^[ \t]*#/ { next }
      {
        rule = $2
        if (!match(rule, /(^|[ ,])SUBSYSTEM=="[^"]*"/)) next
        value = substr(rule, RSTART, RLENGTH)
        sub(/^[ ,]?SUBSYSTEM=="/, "", value); sub(/"$/, "", value)
        n = split(value, alt, "|")
        for (i = 1; i <= n; i++) {
          a = alt[i]
          if (a ~ /[*?[]/) {
            re = glob_re(a)
            if ("module" ~ re || "drivers" ~ re || "bus" ~ re || "subsystem" ~ re)
              fail("SUBSYSTEM pattern matches subsystem events")
            continue
          }
          if (a == "drivers" || a == "bus" || a == "subsystem") { fail("rule needs " a " events"); continue }
          if (a != "module") continue
          if (!match(rule, /(^|[ ,])KERNEL=="[^"]*"/)) { fail("module rule names no KERNEL"); continue }
          k = substr(rule, RSTART, RLENGTH)
          sub(/^[ ,]?KERNEL=="/, "", k); sub(/"$/, "", k)
          if (k == "" || k ~ /[*?[]/) { fail("module rule matches KERNEL by pattern"); continue }
          m = split(k, names, "|")
          for (j = 1; j <= m; j++) print names[j] >> "modules.txt"
        }
      }
      END { exit bad }
    ' rules.tsv
    sort -u modules.txt > "$out"
  ''
