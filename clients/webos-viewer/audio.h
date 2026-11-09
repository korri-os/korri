/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef KV_AUDIO_H
#define KV_AUDIO_H
#include <Limelight.h>
#include <ss4s.h>
#include <stdint.h>

typedef struct {
    uint64_t calls, packets, plc, decoded_samples, decode_errors;
    uint64_t feed_ok, feed_not_ready, feed_overflow, feed_errors, feed_bytes, feed_max_us;
    uint64_t first_packet, decrypt_errors, receive_errors, queue_overflow;
    unsigned peak;
    int in_feed, last_decode, last_feed;
} kv_audio_stats;

/* One common-c connection per process. Supplies the production callbacks used
 * by both the viewer and native tests; a blocking sink keeps the decoder thread. */
void kv_audio_callbacks(SS4S_Player *player, AUDIO_RENDERER_CALLBACKS *out, void (*failed)(void));
void kv_audio_snapshot(kv_audio_stats *out);
/* At most one line per elapsed second and 30 reports per connection, including
 * intervals with no decode callbacks. Uses counts/amplitudes only, never samples. */
void kv_audio_report(unsigned long long elapsed_seconds);
/* Allowlisted common-c audio events: never formats or prints upstream text. */
void kv_audio_common_log(const char *format);
#endif
