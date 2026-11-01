#!/usr/bin/env bash
# Proves each gate bites: plant one defect, run the gates, expect a named
# failure, remove the defect. Run from clients/shell with bun on PATH:
#
#   bash packages/zone/test/bite.sh
set -uo pipefail
cd "$(dirname "$0")/../../.."
zone=packages/zone
plant() { # name file expected-failure (append TEXT | replace OLD NEW)
  local name=$1 file=$2 expect=$3 how=$4 backup
  backup=$(mktemp)
  if [[ -e $file ]]; then cp "$file" "$backup"; else : >"$backup"; fi
  if [[ $how == replace ]]; then
    python3 -c 'import sys; p,o,n=sys.argv[1:]; s=open(p).read(); assert o in s, o; open(p,"w").write(s.replace(o,n,1))' "$file" "$5" "$6"
  else
    printf '%s\n' "$5" >>"$file"
  fi
  local out
  out=$(bun test "$zone/test/" 2>&1)
  if grep -q "(fail) .*$expect" <<<"$out"; then echo "bites: $name -> (fail) $expect"; else echo "DOES NOT BITE: $name"; fi
  if [[ -s $backup ]]; then cp "$backup" "$file"; else rm -f "$file"; fi
  rm -f "$backup"
}
Z=$zone
plant "component with no preview" "$Z/ui/atoms/ZonePlanted.slint" "every component has its preview file" append 'export component ZonePlanted { }'
plant "component with no React counterpart" "$Z/ui/atoms/ZonePlanted.slint" "no component without a React counterpart" append 'export component ZonePlanted { }'
plant "preview that does not render its component" "$Z/ui/atoms/ZoneBars.atom.preview.slint" "renders that component" replace '    ZoneBars { ' '    Rectangle { '
plant "two components in one file" "$Z/ui/atoms/ZoneBars.slint" "exactly one component per component file" append 'component Extra { }'
plant "an atom importing a molecule" "$Z/ui/atoms/ZoneBars.slint" "imports point down" append 'import { ZoneCard } from "../molecules/ZoneCard.slint";'
plant "a shape writing the Tokens global" "$Z/ui/atoms/ZoneBars.slint" "writes the Tokens global" replace '    in property <int> lit;' '    in property <int> lit;
    init => { Tokens.look = "retro"; }'
plant "a raw colour in a component" "$Z/ui/atoms/ZoneBars.slint" "no raw colour" replace 'Tokens.track;' '#3a313a;'
plant "a pixel literal in a component" "$Z/ui/atoms/ZoneBars.slint" "no pixel literal" replace 'width: 11 * a;' 'width: 22px;'
plant "a drop shadow" "$Z/ui/atoms/ZoneBars.slint" "no blur, drop shadow" replace 'width: 11 * a;' 'width: 11 * a;
    drop-shadow-blur: 2 * a;'
plant "a smooth gradient" "$Z/ui/atoms/ZoneBars.slint" "hard-stop stripe" replace 'width: 11 * a;' 'width: 11 * a;
    Rectangle { background: @linear-gradient(90deg, Tokens.white 0%, Tokens.black 100%); }'
plant "an opacity under 1" "$Z/ui/atoms/ZoneBars.slint" "no opacity under 1" replace 'width: 11 * a;' 'width: 11 * a;
    opacity: 0.5;'
plant "a second see-through colour" "$Z/ui/tokens.slint" "the one colour with alpha" replace '    out property <color> veil-see-through: #00000080;' '    out property <color> veil-see-through: #00000080;
    out property <color> mist: #ffffff80;'
plant "the see-through read outside ZoneVeil" "$Z/ui/atoms/ZoneBars.slint" "only ZoneVeil reads it" replace 'Tokens.track;' 'Tokens.veil-see-through;'
plant "a steel too dark to read on black" "$Z/ui/tokens.slint" "steel on black" replace 'steel: #84a2b5;' 'steel: #303a40;'
plant "a ground too dark for the build string" "$Z/ui/tokens.slint" "on ground-tally" replace 'ground-tally: #efc300;' 'ground-tally: #3a3000;'
plant "a Tokens threshold away from the plan's" "$Z/ui/tokens.slint" "held equal" replace 'wide-at: 333;' 'wide-at: 334;'
