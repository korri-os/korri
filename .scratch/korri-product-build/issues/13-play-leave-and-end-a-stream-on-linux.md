# Play, leave, and end a stream on Linux

Status: resolved
Blocked by: 04

## What to build

Give Linux devices a real streamed-play route with the same live-session promises as local play. The player can leave, return, and end the exact far game without learning which device runs it.

## Acceptance criteria

- [ ] A Linux stream viewer launches from a real playable route and is tracked as the near side of one exact live session.
- [ ] Home on the near device freezes the exact far launch and opens the local gameplay overlay.
- [ ] Return thaws the same far launch and returns to the local viewer. A stale launch is refused rather than retargeted.
- [ ] Portal end confirms once and the kill chord acts immediately. Both stop the exact far launch and the local viewer.
- [ ] If the near device cannot confirm the far stop, Korri says so and does not report the live session ended.
- [ ] Closing only the viewer while the far game continues is not exposed as a product action.
- [ ] The overlay shows only the core controls and actions that korrid lists for that streamed session.
- [ ] Restart recovery preserves or truthfully reports the exact near/far session state.
- [ ] Automated tests cover near/far freeze, thaw, stop, stale identity, lost confirmation, and restart recovery.
- [ ] A recorded Linux near/far acceptance run demonstrates play, leave, return, and end. It makes no encoder-performance or supported-device claim.
