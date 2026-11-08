/* SPDX-License-Identifier: GPL-3.0-or-later */
#define _POSIX_C_SOURCE 200809L
#include "internal.h"
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <mbedtls/net_sockets.h>
#include <mbedtls/platform_util.h>
#include <mbedtls/ssl.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

typedef struct {
    int fd;
    long long deadline;
} transport;
static long long millis(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now))
        return 0;
    return (long long)now.tv_sec * 1000 + now.tv_nsec / 1000000;
}
static int wait_fd(transport *t, short events) {
    for (;;) {
        long long left = t->deadline - millis();
        if (left <= 0)
            return -1;
        struct pollfd p = {.fd = t->fd, .events = events};
        int result = poll(&p, 1, (int)left);
        if (result < 0 && errno == EINTR)
            continue;
        return result > 0 && (p.revents & events) ? 0 : -1;
    }
}
static int send_bytes(void *context, const unsigned char *data, size_t length) {
    transport *t = context;
    for (;;) {
        if (wait_fd(t, POLLOUT))
            return MBEDTLS_ERR_NET_SEND_FAILED;
        ssize_t n = send(t->fd, data, length, MSG_NOSIGNAL);
        if (n < 0 && (errno == EINTR || errno == EAGAIN))
            continue;
        return n > 0 ? (int)n : MBEDTLS_ERR_NET_SEND_FAILED;
    }
}
static int recv_bytes(void *context, unsigned char *data, size_t length) {
    transport *t = context;
    for (;;) {
        if (wait_fd(t, POLLIN))
            return MBEDTLS_ERR_NET_RECV_FAILED;
        ssize_t n = recv(t->fd, data, length, 0);
        if (n < 0 && (errno == EINTR || errno == EAGAIN))
            continue;
        return n >= 0 ? (int)n : MBEDTLS_ERR_NET_RECV_FAILED;
    }
}
static int connect_numeric(transport *t, const char *host, unsigned port) {
    struct sockaddr_storage storage;
    memset(&storage, 0, sizeof(storage));
    struct sockaddr_in *v4 = (struct sockaddr_in *)&storage;
    struct sockaddr_in6 *v6 = (struct sockaddr_in6 *)&storage;
    socklen_t size;
    if (!port || port > 65535)
        return -1;
    if (inet_pton(AF_INET, host, &v4->sin_addr) == 1) {
        v4->sin_family = AF_INET;
        v4->sin_port = (uint16_t)htons((uint16_t)port);
        size = sizeof(*v4);
    } else if (inet_pton(AF_INET6, host, &v6->sin6_addr) == 1) {
        v6->sin6_family = AF_INET6;
        v6->sin6_port = (uint16_t)htons((uint16_t)port);
        size = sizeof(*v6);
    } else
        return -1;
    t->fd = socket(storage.ss_family, SOCK_STREAM, 0);
    if (t->fd < 0)
        return -1;
    if (fcntl(t->fd, F_SETFD, FD_CLOEXEC) || fcntl(t->fd, F_SETFL, O_NONBLOCK))
        return -1;
    if (connect(t->fd, (struct sockaddr *)&storage, size) == 0)
        return 0;
    if (errno != EINPROGRESS || wait_fd(t, POLLOUT))
        return -1;
    int error = 0;
    size = sizeof(error);
    return getsockopt(t->fd, SOL_SOCKET, SO_ERROR, &error, &size) || error ? -1 : 0;
}
static int verify_pin(void *context, mbedtls_x509_crt *certificate, int depth, uint32_t *flags) {
    const mbedtls_x509_crt *pin = context;
    if (depth == 0 && (certificate->raw.len != pin->raw.len ||
                       memcmp(certificate->raw.p, pin->raw.p, pin->raw.len)))
        *flags |= MBEDTLS_X509_BADCERT_NOT_TRUSTED;
    /* Never clear verification flags. The exact self-signed leaf is the trust
     * anchor, not a public CA. No hostname check: Sunshine's generated subject
     * is not its route address; authenticated provisioning supplies identity. */
    return 0;
}
static int body_bounds(char *buffer, size_t used, size_t *offset, size_t *body_length) {
    char *end = strstr(buffer, "\r\n\r\n");
    if (!end)
        return used > 8192 ? -1 : 1;
    *offset = (size_t)(end + 4 - buffer);
    if (*offset > 8192 ||
        (strncmp(buffer, "HTTP/1.1 200 ", 13) && strncmp(buffer, "HTTP/1.0 200 ", 13)))
        return -1;
    *body_length = SIZE_MAX;
    char *line = strstr(buffer, "\r\n") + 2;
    while (line < end) {
        char *next = strstr(line, "\r\n");
        if (!next || next > end || line[0] == ' ' || line[0] == '\t')
            return -1;
        if (!strncasecmp(line, "Content-Length:", 15)) {
            if (*body_length != SIZE_MAX)
                return -1;
            char *start = line + 15;
            while (*start == ' ')
                start++;
            if (*start < '0' || *start > '9')
                return -1;
            char *tail;
            errno = 0;
            unsigned long n = strtoul(start, &tail, 10);
            if (errno || tail != next || n > KV_LIMIT - *offset)
                return -1;
            *body_length = n;
        } else if (!strncasecmp(line, "Transfer-Encoding:", 18) ||
                   !strncasecmp(line, "Content-Encoding:", 17)) {
            /* Sunshine emits content-length. Refuse alternate framing rather
             * than parse ambiguous chunking/compression or allocate unboundedly. */
            return -1;
        }
        line = next + 2;
    }
    return 0;
}
int kv_http(kv_identity *identity, const char *host, unsigned port, const char *pin_pem,
            const char *path, char **response, size_t *length) {
    *response = NULL;
    *length = 0;
    transport t = {.fd = -1, .deadline = millis() + 10000};
    mbedtls_ssl_context ssl;
    mbedtls_ssl_config config;
    mbedtls_x509_crt pin;
    mbedtls_ssl_init(&ssl);
    mbedtls_ssl_config_init(&config);
    mbedtls_x509_crt_init(&pin);
    int result = -1;
    char request[4096];
    char *buffer = calloc(1, KV_LIMIT + 1);
    if (!buffer) {
        kv_fail("allocation failed");
        goto done;
    }
    int secure = pin_pem != NULL;
    if (secure) {
        size_t pin_length = strnlen(pin_pem, 8192);
        if (!identity || pin_length == 8192 ||
            mbedtls_x509_crt_parse(&pin, (const unsigned char *)pin_pem, pin_length + 1) ||
            pin.next) {
            kv_fail("one valid provisioned server certificate is required");
            goto done;
        }
        if (mbedtls_ssl_config_defaults(&config, MBEDTLS_SSL_IS_CLIENT,
                                        MBEDTLS_SSL_TRANSPORT_STREAM, MBEDTLS_SSL_PRESET_DEFAULT))
            goto tls_error;
        mbedtls_ssl_conf_authmode(&config, MBEDTLS_SSL_VERIFY_REQUIRED);
        mbedtls_ssl_conf_ca_chain(&config, &pin, NULL);
        mbedtls_ssl_conf_verify(&config, verify_pin, &pin);
        mbedtls_ssl_conf_rng(&config, kv_random, NULL);
        if (mbedtls_ssl_conf_own_cert(&config, &identity->certificate, &identity->key) ||
            mbedtls_ssl_setup(&ssl, &config) || mbedtls_ssl_set_hostname(&ssl, NULL))
            goto tls_error;
    }
    if (connect_numeric(&t, host, port)) {
        kv_fail("numeric address connection failed or timed out");
        goto done;
    }
    if (secure) {
        mbedtls_ssl_set_bio(&ssl, &t, send_bytes, recv_bytes, NULL);
        if (mbedtls_ssl_handshake(&ssl) || mbedtls_ssl_get_verify_result(&ssl))
            goto tls_error;
        const mbedtls_x509_crt *peer = mbedtls_ssl_get_peer_cert(&ssl);
        if (!peer || peer->raw.len != pin.raw.len || memcmp(peer->raw.p, pin.raw.p, pin.raw.len))
            goto tls_error;
    }
    int n = snprintf(request, sizeof(request),
                     "GET %s HTTP/1.1\r\nHost: %s%s%s:%u\r\nConnection: close\r\nAccept: "
                     "application/xml\r\n\r\n",
                     path, strchr(host, ':') ? "[" : "", host, strchr(host, ':') ? "]" : "", port);
    if (n < 0 || (size_t)n >= sizeof(request) || strchr(path, '\r') || strchr(path, '\n')) {
        kv_fail("request exceeds bound");
        goto done;
    }
    size_t sent = 0;
    while (sent < (size_t)n) {
        int count = secure
                        ? mbedtls_ssl_write(&ssl, (unsigned char *)request + sent, (size_t)n - sent)
                        : send_bytes(&t, (unsigned char *)request + sent, (size_t)n - sent);
        if (count <= 0) {
            kv_fail("request write failed");
            goto done;
        }
        sent += (size_t)count;
    }
    size_t used = 0, offset = 0, size = SIZE_MAX;
    for (;;) {
        int count = secure ? mbedtls_ssl_read(&ssl, (unsigned char *)buffer + used, KV_LIMIT - used)
                           : recv_bytes(&t, (unsigned char *)buffer + used, KV_LIMIT - used);
        if (count == 0 || count == MBEDTLS_ERR_SSL_PEER_CLOSE_NOTIFY) {
            if (!offset || (size != SIZE_MAX && used - offset != size)) {
                kv_fail("truncated HTTP response");
                goto done;
            }
            break;
        }
        if (count < 0) {
            kv_fail("response read failed or timed out");
            goto done;
        }
        /* XML is text, so embedded NUL cannot be a valid response. */
        if (memchr(buffer + used, 0, (size_t)count)) {
            kv_fail("invalid response encoding");
            goto done;
        }
        used += (size_t)count;
        buffer[used] = 0;
        int parsed = body_bounds(buffer, used, &offset, &size);
        if (parsed < 0 || used == KV_LIMIT ||
            (offset && size != SIZE_MAX && used - offset > size)) {
            kv_fail("HTTP status, framing or size rejected");
            goto done;
        }
        if (!parsed && size != SIZE_MAX && used - offset == size)
            break;
    }
    *length = used - offset;
    memmove(buffer, buffer + offset, *length);
    buffer[*length] = 0;
    *response = buffer;
    buffer = NULL;
    result = 0;
    goto done;
tls_error:
    kv_fail("TLS certificate verification, exact pin or client authentication failed");
done:
    mbedtls_platform_zeroize(request, sizeof(request));
    free(buffer);
    if (t.fd >= 0)
        close(t.fd);
    mbedtls_ssl_free(&ssl);
    mbedtls_ssl_config_free(&config);
    mbedtls_x509_crt_free(&pin);
    return result;
}
