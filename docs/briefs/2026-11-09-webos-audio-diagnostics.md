# webOS audio: bounded diagnostics and real Pulse verification

**The TV's silent-audio cause is not established yet.** The next build measures
where audio stops. No TV or real host was contacted for this work.

The owner reports that IPK `07bde03e…`, source `651261748`, now auto-pairs,
resolves the authenticated transport app, reaches READY, shows full-screen video
and forwards the pad on the TV. The TV's GameStream Pulse sink input is unmuted
at 100% on running `pcm_output`, but its three-second monitor capture is entirely
zero. aka's game-to-Sunshine PipeWire links are active. Active links alone do not
prove that captured host samples are nonzero; the new viewer counters distinguish
that from a decode/feed failure.

## Source checks

| Question | Verified source and result |
| --- | --- |
| Is DIRECT_SUBMIT required for audio callbacks? | No. The exact shipped `clients/android/app/src/main/jni/moonlight-core/moonlight-common-c/src/AudioStream.c` queues audio when the flag is zero (lines 325/353), creates `AudioDec` (455), and calls `decodeInputData` from that thread. Both queued and direct paths invoke `decodeAndPlaySample`. `Limelight.h` describes the flags. The viewer retains zero to keep blocking Pulse writes off AudioRecv; changing this scheduling is not required to obtain callbacks. |
| What duration and mapping does this stream use? | The same fork's `RtspConnection.c:739–748` sets stereo to 48 kHz, two channels, one stream, one coupled stream and mapping 0,1. `SdpGenerator.c:509–517` chooses 5 ms with the existing zero capabilities, and `AudioStream.c:438` sets samplesPerFrame to 48 × packet duration, normally 240. The adapter uses that actual field, not a hardcoded size. |
| What does Moonlight Embedded do? | Its pinned [Pulse renderer][embedded] copies samplesPerFrame, creates the Opus multistream decoder from the supplied counts/mapping, and writes decoded_samples × channels × sizeof(short). It rearranges channels only for surround, not stereo. It advertises DIRECT_SUBMIT and arbitrary duration; those change scheduling/negotiation, not whether callbacks exist. |
| What does Moonlight TV do? | At the inspected [v1.6.36 source][tv], the PCM path uses opusConfig->samplesPerFrame as decode capacity and the supplied mapping, then sends channel_count × sizeof(int16_t) × decoded_samples to ss4s. It advertises DIRECT_SUBMIT. Korri keeps its separate decoder thread because this sink is blocking. |
| Does ss4s actually write Pulse PCM? | Its pinned [Pulse Feed][pulse] calls `pa_simple_write`; `SS4S_PlayerAudioFeed` in ss4s `src/audio.c` forwards the same pointer and byte count. Pulse `pulse/simple.h` documents read/write as blocking. Korri's existing patch propagates a negative write result as SS4S_AUDIO_FEED_ERROR. The new real null-sink test verifies nonzero PCM reaches a Pulse monitor through that exact module. |

The existing frame size and stereo mapping were correct by source inspection and
native encoded-packet tests. No speculative capability, packet-duration, mapping,
volume, sink-selection or buffer-policy change is made.

## New diagnostics

`clients/webos-viewer/audio.c` contains the same production callback adapter used
by `stream.c` and native tests. Its audio_init line reports configuration, rate,
channels, stream/coupled counts, mapping, samplesPerFrame and capabilities.

The control thread emits **at most one audio line per second, 30 reports per
viewer connection**. It still reports when no decoder callback ran. Counts are
cumulative; peak is the maximum absolute interleaved PCM sample since the previous
report. Samples are per channel; successful feed bytes are samples × 2 × 2.

- calls, real packets, PLC calls, decoded samples, decode errors and last decode
  result;
- interval PCM peak;
- feed OK/not-ready/overflow/error counts, last result, successful bytes,
  in-progress write flag and maximum completed feed duration;
- sanitized common-c counters for first received audio packet, decrypt failures,
  receive failures and packet-queue overflow.

Only exact known common-c format strings increment counters. Their arguments,
packet contents and arbitrary upstream diagnostics remain suppressed. The child
log prefix remains unchanged, so korrid's existing bounded stderr reader carries
these records. No keys, pins, URLs, session material, PCM contents, new wire
messages or persisted data are introduced.

A failed decode or any non-OK ss4s feed now signals the existing local transport
failure path. NOT_READY and OVERFLOW can no longer silently discard audio. This
still cannot authorize host End or GameStream /cancel.

## Workstation evidence

The test drives real Opus multistream-encoded, nonzero stereo packets through the
**production callbacks**, not a replacement decoder. It checks 120, 240, 480 and
960 samples/frame, PLC, invalid packet failure, exact feed byte counts, positive
PCM peaks, NOT_READY handling and the diagnostic rate/total/line-length bounds.

A second test starts a private Unix-only Pulse daemon with a null sink and records
that sink's monitor. It loads the real patched ss4s-pulse module and sends the
same encoded tone through the production decoder and `pa_simple_write`. One run
recorded **261,738 samples, peak 19,926, RMS 8,947.1**. This is generated workstation
audio, not a TV or aka observation. No existing sound server or hardware sink is
used. The test's temporary daemon, socket, monitor file and configuration are
removed afterward.

`CARGO_BUILD_JOBS=2 nix run .#webos-viewer-check` runs the existing 14 tests, the C
protocol/media tests, and both new audio tests. The ARM/IPK symbol checks remain
required before a TV build is handed over.

## Owner's next TV run

Start a stream and retain the first 30 seconds of `stage=audio_init` and
`stage=audio` lines. Repeat the existing `pcm_output.monitor` RMS measurement.

| Observed counters | Next boundary to inspect |
| --- | --- |
| received=0, calls=0 | No first audio RTP packet reached common-c; check negotiated audio/UDP route. |
| received>0, decrypt_err rising, calls=0 | Audio decryption before the renderer. |
| calls rising but packets=0, PLC rising | Concealment rather than usable Opus packets. |
| decode_err rising / negative decode_last | Opus packet, mapping or frame-capacity mismatch. |
| packets/samples/feed_ok/bytes rising, peak=0 | Decoder receives valid silence; compare the host's actual captured PCM, not only active links. |
| peak>0, feeding=1 and feed_ok/bytes stop growing | Sink write is blocked; correlate Pulse buffering/server state. |
| peak>0 and feed_ok/bytes grow, but TV monitor stays zero | Nonzero PCM passed to Pulse; inspect TV routing/mixing/output after the write. |

Nonzero metrics do not prove audible TV output. This diagnostic build does not
claim to fix the TV sound defect before the owner repeats that check.

[embedded]: https://github.com/moonlight-stream/moonlight-embedded/blob/775444287305849ebdf4736c75298ad0713e2d5d/src/audio/pulse.c
[tv]: https://github.com/mariotaku/moonlight-tv/blob/89221f66b79e06178a6465503cc5a306d2794786/src/app/stream/audio/session_audio.c
[pulse]: https://github.com/mariotaku/ss4s/blob/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2/modules/pulse/pulse.c
