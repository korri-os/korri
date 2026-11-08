/* SPDX-License-Identifier: GPL-3.0-or-later */
#define _GNU_SOURCE
#include "internal.h"
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <mbedtls/asn1write.h>
#include <mbedtls/oid.h>
#include <mbedtls/pem.h>
#include <mbedtls/platform_util.h>
#include <mbedtls/rsa.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

/* pthread TLS avoids an ELF __tls_get_addr import from the TV loader, whose
 * original binary is not in the supplied ABI evidence. Messages are literals. */
static pthread_key_t error_key;
static pthread_once_t error_once = PTHREAD_ONCE_INIT;
static int error_key_ready;
static void make_error_key(void) { error_key_ready = pthread_key_create(&error_key, NULL) == 0; }
int kv_fail(const char *message) {
    pthread_once(&error_once, make_error_key);
    if (error_key_ready)
        pthread_setspecific(error_key, message);
    return -1;
}
const char *kv_error(void) {
    pthread_once(&error_once, make_error_key);
    const char *message = error_key_ready ? pthread_getspecific(error_key) : NULL;
    return message ? message : "viewer operation failed";
}

/* The SDK predates getrandom's libc wrapper and syscall header, but the TV's
 * kernel implements it. No weak seed and no /dev/random dependency in the jail. */
#ifndef SYS_getrandom
#if defined(__arm__)
#define SYS_getrandom 384
#elif defined(__x86_64__)
#define SYS_getrandom 318
#else
#error Supply the Linux getrandom syscall number for this architecture
#endif
#endif
int kv_random(void *unused, unsigned char *out, size_t length) {
    (void)unused;
    while (length) {
        ssize_t n = syscall(SYS_getrandom, out, length, 0);
        if (n < 0 && errno == EINTR)
            continue;
        if (n <= 0)
            return -1;
        out += n;
        length -= (size_t)n;
    }
    return 0;
}
int mbedtls_hardware_poll(void *unused, unsigned char *out, size_t length, size_t *written) {
    int result = kv_random(unused, out, length);
    *written = result ? 0 : length;
    return result;
}
static int read_fd(int fd, char *out, size_t capacity) {
    size_t used = 0;
    while (used < capacity - 1) {
        ssize_t n = read(fd, out + used, capacity - 1 - used);
        if (n < 0 && errno == EINTR)
            continue;
        if (n < 0)
            return kv_fail("identity read failed");
        if (!n) {
            out[used] = 0;
            return used ? 0 : kv_fail("empty identity file");
        }
        used += (size_t)n;
    }
    unsigned char extra;
    ssize_t n;
    do {
        n = read(fd, &extra, 1);
    } while (n < 0 && errno == EINTR);
    if (n == 0) {
        out[used] = 0;
        return 0;
    }
    return kv_fail("identity file exceeds bound");
}
int kv_read_file(const char *path, char *out, size_t capacity) {
    int fd = open(path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK);
    if (fd < 0)
        return kv_fail("cannot open certificate file");
    struct stat st;
    int result = fstat(fd, &st) || !S_ISREG(st.st_mode) ? kv_fail("certificate must be regular")
                                                        : read_fd(fd, out, capacity);
    close(fd);
    return result;
}
/* Walk from / using descriptors. Refuse symlinks in every ancestor, and ..
 * components. Root-owned system ancestors are allowed, as in webos-korrid. */
static int open_directory(const char *path) {
    if (!path || path[0] != '/' || strlen(path) > 4095)
        return kv_fail("identity path must be absolute and bounded");
    char copy[4096];
    strcpy(copy, path);
    int fd = open("/", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    char *save = NULL, *part = strtok_r(copy, "/", &save);
    if (!part) {
        close(fd);
        return kv_fail("identity cannot be filesystem root");
    }
    while (part) {
        char *next = strtok_r(NULL, "/", &save);
        if (!strcmp(part, ".") || !strcmp(part, "..")) {
            close(fd);
            return kv_fail("invalid identity path component");
        }
        if (!next && mkdirat(fd, part, 0700) && errno != EEXIST) {
            close(fd);
            return kv_fail("cannot create identity directory");
        }
        int child = openat(fd, part, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
        close(fd);
        if (child < 0)
            return kv_fail("identity path must contain real directories");
        fd = child;
        part = next;
    }
    return fd;
}
static int regular_owned(int fd) {
    struct stat st;
    return fstat(fd, &st) == 0 && S_ISREG(st.st_mode) && st.st_uid == geteuid() && st.st_nlink == 1;
}
static int write_new(int directory, const char *name, const char *data) {
    int fd = openat(directory, name, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
    if (fd < 0)
        return kv_fail("cannot create identity file; no automatic rotation");
    size_t size = strlen(data), used = 0;
    int result = 0;
    while (used < size) {
        ssize_t n = write(fd, data + used, size - used);
        if (n < 0 && errno == EINTR)
            continue;
        if (n <= 0) {
            result = -1;
            break;
        }
        used += (size_t)n;
    }
    if (fsync(fd))
        result = -1;
    close(fd);
    return result ? kv_fail("identity write failed; partial identity needs explicit recovery") : 0;
}
/* PEM_write_PrivateKey in stock Embedded emits unencrypted PKCS#8. mbedTLS
 * emits PKCS#1 for RSA, so wrap that DER in PKCS#8 PrivateKeyInfo explicitly. */
static int stock_private_key(mbedtls_pk_context *key, unsigned char pem[8192]) {
    unsigned char der[8192];
    int result = -1;
    int length = mbedtls_pk_write_key_der(key, der, sizeof(der));
    if (length <= 0)
        goto done;
    unsigned char *p = der + sizeof(der) - (size_t)length;
    int n = mbedtls_asn1_write_len(&p, der, (size_t)length);
    if (n < 0)
        goto done;
    n = mbedtls_asn1_write_tag(&p, der, MBEDTLS_ASN1_OCTET_STRING);
    if (n < 0)
        goto done;
    n = mbedtls_asn1_write_algorithm_identifier(&p, der, MBEDTLS_OID_PKCS1_RSA,
                                                MBEDTLS_OID_SIZE(MBEDTLS_OID_PKCS1_RSA), 0);
    if (n < 0 || mbedtls_asn1_write_int(&p, der, 0) < 0)
        goto done;
    size_t total = (size_t)(der + sizeof(der) - p);
    if (mbedtls_asn1_write_len(&p, der, total) < 0 ||
        mbedtls_asn1_write_tag(&p, der, MBEDTLS_ASN1_CONSTRUCTED | MBEDTLS_ASN1_SEQUENCE) < 0)
        goto done;
    total = (size_t)(der + sizeof(der) - p);
    size_t written;
    result = mbedtls_pem_write_buffer("-----BEGIN PRIVATE KEY-----\n",
                                      "-----END PRIVATE KEY-----\n", p, total, pem, 8192, &written);
done:
    mbedtls_platform_zeroize(der, sizeof(der));
    return result;
}
static int generate(kv_identity *id) {
    char key[8192] = {0}, before[16], after[16];
    mbedtls_x509write_cert cert;
    mbedtls_x509write_crt_init(&cert);
    int result = -1;
    time_t now = time(NULL);
    struct tm tm;
    if (now <= 0 || !gmtime_r(&now, &tm) || !strftime(before, sizeof(before), "%Y%m%d%H%M%S", &tm))
        goto done;
    /* Compute certificate validity in calendar fields: adding ten years of
     * seconds to the TV's 32-bit time_t would overflow before 2038. */
    tm.tm_year += 10;
    int year = tm.tm_year + 1900;
    if (tm.tm_mon == 1 && tm.tm_mday == 29 &&
        (year % 4 != 0 || (year % 100 == 0 && year % 400 != 0)))
        tm.tm_mday = 28;
    if (!strftime(after, sizeof(after), "%Y%m%d%H%M%S", &tm))
        goto done;
    if (mbedtls_pk_setup(&id->key, mbedtls_pk_info_from_type(MBEDTLS_PK_RSA)) ||
        mbedtls_rsa_gen_key(mbedtls_pk_rsa(id->key), kv_random, NULL, 2048, 65537))
        goto done;
    mbedtls_x509write_crt_set_version(&cert, MBEDTLS_X509_CRT_VERSION_3);
    mbedtls_x509write_crt_set_md_alg(&cert, MBEDTLS_MD_SHA256);
    mbedtls_x509write_crt_set_subject_key(&cert, &id->key);
    mbedtls_x509write_crt_set_issuer_key(&cert, &id->key);
    unsigned char serial = 0;
    if (mbedtls_x509write_crt_set_serial_raw(&cert, &serial, 1) ||
        mbedtls_x509write_crt_set_subject_name(&cert, "CN=NVIDIA GameStream Client") ||
        mbedtls_x509write_crt_set_issuer_name(&cert, "CN=NVIDIA GameStream Client") ||
        mbedtls_x509write_crt_set_validity(&cert, before, after) ||
        mbedtls_x509write_crt_pem(&cert, (unsigned char *)id->pem, sizeof(id->pem), kv_random,
                                  NULL) ||
        stock_private_key(&id->key, (unsigned char *)key))
        goto done;
    unsigned char unique[8];
    if (kv_random(NULL, unique, sizeof(unique)))
        goto done;
    for (size_t i = 0; i < sizeof(unique); i++)
        snprintf(id->uniqueid + i * 2, 3, "%02X", unique[i]);
    /* Only the three owner-approved stock files. A power loss can leave a
     * partial identity; fail closed on the next open rather than rotate trust. */
    if (write_new(id->directory, "key.pem", key) ||
        write_new(id->directory, "client.pem", id->pem) ||
        write_new(id->directory, "uniqueid.dat", id->uniqueid) || fsync(id->directory))
        goto done;
    result = 0;
done:
    mbedtls_platform_zeroize(key, sizeof(key));
    mbedtls_x509write_crt_free(&cert);
    return result
               ? kv_fail("identity generation failed; never automatically replace existing files")
               : 0;
}
int kv_identity_open(const char *path, kv_identity **out) {
    *out = NULL;
    kv_identity *id = calloc(1, sizeof(*id));
    if (!id)
        return kv_fail("allocation failed");
    id->directory = -1;
    mbedtls_pk_init(&id->key);
    mbedtls_x509_crt_init(&id->certificate);
    id->directory = open_directory(path);
    struct stat st;
    if (id->directory < 0)
        goto failed;
    if (fstat(id->directory, &st) || st.st_uid != geteuid() ||
        flock(id->directory, LOCK_EX | LOCK_NB)) {
        kv_fail("identity directory must be caller-owned and not already in use");
        goto failed;
    }
    const char *names[] = {"client.pem", "key.pem", "uniqueid.dat"};
    int files[] = {-1, -1, -1};
    int count = 0, invalid = 0;
    DIR *dir = fdopendir(dup(id->directory));
    if (!dir) {
        kv_fail("cannot inspect identity directory");
        goto failed;
    }
    struct dirent *entry;
    errno = 0;
    while ((entry = readdir(dir))) {
        if (!strcmp(entry->d_name, ".") || !strcmp(entry->d_name, ".."))
            continue;
        int index = -1;
        for (int i = 0; i < 3; i++)
            if (!strcmp(entry->d_name, names[i]))
                index = i;
        if (index < 0) {
            invalid = 1;
            break;
        }
        int fd =
            openat(id->directory, entry->d_name, O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK);
        if (fd < 0 || !regular_owned(fd)) {
            if (fd >= 0)
                close(fd);
            invalid = 1;
            break;
        }
        files[index] = fd;
        count++;
    }
    if (errno)
        invalid = 1;
    closedir(dir);
    if (count && count != 3)
        invalid = 1;
    if (invalid)
        kv_fail("identity must contain only three caller-owned singly-linked regular stock files");
    /* Audit everything first, then chmod through held descriptors. */
    if (!invalid && fchmod(id->directory, 0700))
        invalid = 1;
    for (int i = 0; !invalid && i < 3 && count; i++)
        if (fchmod(files[i], 0600))
            invalid = 1;
    char key[8192] = {0};
    if (!invalid && count) {
        if (read_fd(files[0], id->pem, sizeof(id->pem)) || read_fd(files[1], key, sizeof(key)) ||
            read_fd(files[2], id->uniqueid, sizeof(id->uniqueid)))
            invalid = 1;
        if (!invalid && mbedtls_pk_parse_key(&id->key, (unsigned char *)key, strlen(key) + 1, NULL,
                                             0, kv_random, NULL))
            invalid = 1;
    }
    mbedtls_platform_zeroize(key, sizeof(key));
    for (int i = 0; i < 3; i++)
        if (files[i] >= 0)
            close(files[i]);
    if (invalid)
        goto failed;
    if (!count && generate(id))
        goto failed;
    if (strlen(id->uniqueid) != 16 || (strspn)(id->uniqueid, "0123456789abcdefABCDEF") != 16 ||
        mbedtls_x509_crt_parse(&id->certificate, (unsigned char *)id->pem, strlen(id->pem) + 1) ||
        id->certificate.next || !mbedtls_pk_can_do(&id->key, MBEDTLS_PK_RSA) ||
        mbedtls_pk_get_bitlen(&id->key) != 2048 ||
        mbedtls_pk_check_pair(&id->certificate.pk, &id->key, kv_random, NULL)) {
        kv_fail("invalid stock identity or certificate/key mismatch; no automatic rotation");
        goto failed;
    }
    *out = id;
    return 0;
failed:
    kv_identity_free(id);
    return -1;
}
const char *kv_identity_certificate(const kv_identity *id) { return id->pem; }
void kv_identity_free(kv_identity *id) {
    if (!id)
        return;
    mbedtls_pk_free(&id->key);
    mbedtls_x509_crt_free(&id->certificate);
    if (id->directory >= 0)
        close(id->directory);
    mbedtls_platform_zeroize(id, sizeof(*id));
    free(id);
}
