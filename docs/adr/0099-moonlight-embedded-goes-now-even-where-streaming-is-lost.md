---
status: accepted
---

# Moonlight Embedded goes now, even where streaming is lost

**Owner decision, 2026-11-24.** “Remove everywhere now.” The owner cares about
Galaxy Tab S6 (`tabs6`) and OnePlus 6 (`op6`). Both use Moonlight Embedded's
V4L2 M2M decoder today. The Rust viewer has no such decoder. The owner accepts
that both lose streaming until one lands. This task does not add that decoder.

## Decision

- Remove Moonlight Embedded, `moonlight-embedded-korri` and `@korri:moonlight`
  from the two repositories. `moonlight-final` marks the last Korri commit
  carrying the integration. The removal-scope brief records both revisions.
- The only Linux viewer is the removable `@korri:viewer` plugin. korrid
  requires it enabled with a treaty byte-for-byte equal to its own. Missing,
  disabled, unreadable, ambiguous or mismatched packages fail clearly. No
  second client runs after a refusal.
- Remove the old client from product images and the published plugin catalog.
  Do not install an unsupported Rust picture path to hide the resulting gap.
- Preserve the Moonlight streaming protocol, Sunshine, pairing, the host UUID,
  port 47989 and `app.moonlight.certificate.*`. The Rust viewer and Android app
  use these. The certificate rules of #476, #502 and #510 do not change.
- ibuki keeps streaming through the Rust viewer. Publish a viewer built from
  the matching Korri commit, then pin the tested catalog and deploy it.
  Removing the fallback does not excuse deploying a mismatched viewer there.

## Superseded parts of earlier decisions

| ADR | What changes | What stays |
| --- | --- | --- |
| 0059 | No licence change | GPL-3 and corresponding source |
| 0060 | Moonlight unit/status observation is removed | Separate Linux process and typed lifecycle socket |
| 0061 | No second viewer plugin or Moonlight fallback | Removable plugin, code in Korri, exact treaty refusal |
| 0062 | Embedded C code no longer stays in the old client pending a Rust proof | Measured stages within the Rust viewer |
| 0063 | Postponed devices do not keep Moonlight Embedded | Autonomous proofs, isolated aka fixture and handheld safety rules |
| 0064 | No change | Android 12 minimum and platform controller support |

This is the same deliberate cut as the Artemis removal (ADR 0087), not a claim
that the missing decoder or picture-path acceptance already exists.

## Cost and consequences

- `tabs6`, `op6`, Retroid Pocket Mini V2, Odin 2 Portal, Thor, RG353M and H700
  devices lose their installed Moonlight streaming path. The shared ARM image
  defaults no longer include a viewer. Other devices with only Moonlight also
  cannot stream until a supported Rust plugin is installed.
- Old acceptance reports and timing baselines stay as history. They do not
  establish Rust viewer acceptance on those devices.
- The Rust viewer retains its carried C picture/input/audio code and pinned
  `moonlight-common-c` alternative. Removing the program is not removing its
  protocol, licences or provenance.
- Historical releases and real user settings are not deleted. Old Moonlight
  settings do not silently become Rust viewer settings. Any needed transfer
  is an explicit one-off operation, not a runtime compatibility path.
