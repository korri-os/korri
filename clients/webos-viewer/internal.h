/* SPDX-License-Identifier: GPL-3.0-or-later */
#ifndef KV_INTERNAL_H
#define KV_INTERNAL_H
#include "viewer.h"
#include <mbedtls/pk.h>
#include <mbedtls/x509_crt.h>
#define KV_LIMIT (128 * 1024)
struct kv_identity {
    int directory;
    mbedtls_x509_crt certificate;
    mbedtls_pk_context key;
    char pem[8192], uniqueid[17];
};
int kv_fail(const char *message);
int kv_random(void *unused, unsigned char *out, size_t length);
int kv_read_file(const char *path, char *out, size_t capacity);
int kv_http(kv_identity *identity, const char *host, unsigned port, const char *pin,
            const char *path, char **response, size_t *length);
#endif
