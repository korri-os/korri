/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef KORRI_WEBOS_VIEWER_H
#define KORRI_WEBOS_VIEWER_H
#include <Limelight.h>
#include <signal.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Local library API, not a korrid treaty or IPC protocol. One connection at a
 * time (common-c and ss4s are process-global). No implicit network or state path.
 * Fallible operations return 0 on success; errors contain no remote response/secret.
 * The caller supplies the provisioned public server certificate, never a TOFU pin.
 */
typedef struct kv_identity kv_identity;
int kv_identity_open(const char *absolute_directory, kv_identity **out);
const char *kv_identity_certificate(const kv_identity *identity);
void kv_identity_free(kv_identity *identity);

typedef struct {
    char uuid[128], app_version[64], gfe_version[64], state[128];
    unsigned https_port;
    int paired, current_game, codec_support;
} kv_serverinfo;
int kv_serverinfo_parse(const char *xml, size_t length, kv_serverinfo *out);
int kv_session_parse(const char *xml, size_t length, int resume, char url[1024]);
/* Numeric IPv4/IPv6 only; no DNS, redirects, proxies, cookies or trust store. */
int kv_serverinfo_public(const char *host, unsigned http_port, kv_serverinfo *out);
int kv_serverinfo_authenticated(kv_identity *identity, const char *host, unsigned https_port,
                                const char *server_certificate, kv_serverinfo *out);
int kv_applist_parse(const char *xml, size_t length, int *app_id);
int kv_applist(kv_identity *identity, const char *host, unsigned https_port,
               const char *server_certificate, int *app_id);
int kv_launch(kv_identity *identity, const char *host, unsigned https_port,
              const char *server_certificate, int app_id, int resume, STREAM_CONFIGURATION *config,
              char rtsp_url[1024]);
/* No GameStream /cancel: stopping this transport does not authorize ending the
 * selected far Korri launch. The future adapter owns exact leave/end ordering.
 * stop must remain valid until return; set it from a signal handler on the
 * owning control thread, not from another thread.
 * mode dummy is for workstation tests only; TV uses ndl-webos4 + pulse.
 */
int kv_stream(const char *host, const kv_serverinfo *server, const char *rtsp_url,
              STREAM_CONFIGURATION *config, int dummy, volatile sig_atomic_t *stop,
              unsigned seconds);
/* Inherited private socket; -1 disables workstation CLI readiness. */
void kv_stream_channel(int descriptor);
/* Same serialized NativeInputEvent/control records as korrid's local source. */
typedef int (*kv_controller_send)(short, short, int, unsigned char, unsigned char, short, short,
                                  short, short);
void kv_input_sender(kv_controller_send send);
int kv_input_frame(const char *json, size_t length);
void kv_input_reset(void);
const char *kv_error(void);
#ifdef __cplusplus
}
#endif
#endif
