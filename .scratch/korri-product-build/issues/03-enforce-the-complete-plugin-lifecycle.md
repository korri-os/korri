# Enforce the complete plugin lifecycle

Status: resolved
Blocked by: None

## What to build

Make install, update, rollback, and removal tell the truth. Removal must release storage before it succeeds, recover after interruption, preserve owner data, and refuse to break required Korri behavior.

## Acceptance criteria

- [ ] An installed plugin retains its current and previous software selections for rollback.
- [ ] A permitted uninstall releases both selections and reclaims eligible unused software before it reports completion.
- [ ] Saves, settings, credentials, identity, retained system versions, shared files, and files still used by another plugin are not deleted.
- [ ] A cleanup failure is visible and leaves the uninstall incomplete. Korri does not claim that storage was freed.
- [ ] An uninstall interrupted by service restart or power loss resumes the original request at startup and does not re-enable the plugin.
- [ ] Removing a plugin required by an agreed device behavior is refused, and the response names that behavior. Default-bundle membership alone does not cause refusal.
- [ ] An update that adds a required plugin shows that plugin and its permissions before approval. Declining it or failing to install it leaves the current release selected.
- [ ] A newly recommended optional default is not installed on an existing device by a system update. Earlier removal and disablement choices remain in effect.
- [ ] Plugin-host crate tests cover retention, cleanup success and failure, interruption recovery, required-plugin refusal, and update refusal.
- [ ] The existing `korri-runtime-plugin-host` VM test covers observable storage reclamation, failed cleanup, restart recovery, and required-plugin refusal.
- [ ] The implementation uses existing plugin selections and receipts as its grounding. It adds no speculative dependency, requiredness, or migration schema.
