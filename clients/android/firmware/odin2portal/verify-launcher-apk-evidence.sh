#!/usr/bin/env bash
set -Eeuo pipefail

APK_EVIDENCE="${1:?usage: verify-launcher-apk-evidence.sh <apk-SHA256SUMS> <apk-contract>}"
APK_CONTRACT="${2:?usage: verify-launcher-apk-evidence.sh <apk-SHA256SUMS> <apk-contract>}"

for file in "$APK_EVIDENCE" "$APK_CONTRACT"; do
  [[ -f "$file" && ! -L "$file" ]] || {
    echo 'launcher APK evidence input is missing or symbolic' >&2
    exit 1
  }
done

expected_apk_sha256="$(tr -d '[:space:]' < "$APK_CONTRACT")"
embedded_apk_sha256="$(awk '$2 == "Korri.extracted.apk" {print $1}' "$APK_EVIDENCE")"
apk_evidence_records="$(awk 'NF {count++} END {print count + 0}' "$APK_EVIDENCE")"
if [[ ! "$expected_apk_sha256" =~ ^[0-9a-f]{64}$ ]] ||
   [[ "$apk_evidence_records" -ne 1 ]] ||
   [[ "$embedded_apk_sha256" != "$expected_apk_sha256" ]]; then
  echo 'launcher APK evidence does not match its contract' >&2
  exit 1
fi
