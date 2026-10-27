#!/usr/bin/env bash
# Pure parser for the secret-free Android local-launch lifecycle publication.
# shellcheck shell=bash

korri_parse_wario_retroarch_publication() {
  local publication_lines="${1-}"
  local publication_count
  local launch_id

  if grep -qE 'KORRI_CONTROL_TOKEN|control(Token|Port)|capability|authorization([: ]|$)' \
      <<<"$publication_lines"; then
    return 1
  fi
  publication_count="$(grep -c . <<<"$publication_lines" || true)"
  [[ "$publication_count" -eq 1 ]] || return 1
  launch_id="$(sed -nE \
    's/^launchId=([0-9a-f]{32}) event=published gameId=01K4J6K8Y00000000000000002 package=com\.korri\.retroarch launcher=retroarch$/\1/p' \
    <<<"$publication_lines")"
  [[ "$launch_id" =~ ^[0-9a-f]{32}$ ]] || return 1
  printf '%s' "$launch_id"
}
