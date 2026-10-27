#!/usr/bin/env bash
# Pure, secret-free predicates shared by the installed-device gate and fixtures.

korri_positive_overlay_predicate() {
  local lifecycle_records="$1"
  local guide_up=''
  local window_create=''

  guide_up="$(grep -E \
    'event=physical-guide-key key=BUTTON_MODE keyCode=110 action=1 deviceId=[0-9]+ consumed=true sessionAccepted=true showing=true' \
    <<<"$lifecycle_records" | tail -1 || true)"
  window_create="$(grep -E \
    'event=overlay-window-create result=success' \
    <<<"$lifecycle_records" | tail -1 || true)"

  [[ -n "$guide_up" && -n "$window_create" ]] || return 1
  printf '%s\n%s\n' "$guide_up" "$window_create"
}
