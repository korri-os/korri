/* Compile against the same WPE flags as main.c, then run:
 * security /absolute/path/to/korri-portal-wpe
 * The ACL policy test needs no root privileges. */
#define main launcher_main
#include "../src/main.c"
#undef main
#include <arpa/inet.h>
#include <poll.h>
#include <sys/wait.h>

static const char* launcher;

static void write_le16(unsigned char* target, guint16 value)
{
    value = GUINT16_TO_LE(value);
    memcpy(target, &value, sizeof(value));
}

static void write_le32(unsigned char* target, guint32 value)
{
    value = GUINT32_TO_LE(value);
    memcpy(target, &value, sizeof(value));
}

static void systemd_acl(unsigned char* acl, uid_t uid)
{
    write_le32(acl, 2);
    const guint16 tags[] = { 1, 2, 4, 16, 32 };
    const guint16 permissions[] = { 4, 4, 0, 4, 0 };
    for (size_t i = 0; i < G_N_ELEMENTS(tags); i++) {
        write_le16(acl + 4 + 8 * i, tags[i]);
        write_le16(acl + 6 + 8 * i, permissions[i]);
        write_le32(acl + 8 + 8 * i, i == 1 ? uid : G_MAXUINT32);
    }
}

static void acl_parser(void)
{
    unsigned char acl[52];
    systemd_acl(acl, 1234);
    g_assert_true(credential_acl_is_private(acl, 44, 1234));
    g_assert_false(credential_acl_is_private(acl, 44, 1235));
    g_assert_false(credential_acl_is_private(acl, 43, 1234));
    g_assert_false(credential_acl_is_private(acl, 52, 1234));
    /* Every changed byte, including the header, must fail closed. */
    for (size_t i = 0; i < 44; i++) {
        acl[i] ^= 1;
        g_assert_false(credential_acl_is_private(acl, 44, 1234));
        acl[i] ^= 1;
    }
    /* Reject grants to the owning group and to other users. */
    write_le16(acl + 6 + 8 * 2, 4);
    g_assert_false(credential_acl_is_private(acl, 44, 1234));
    systemd_acl(acl, 1234);
    write_le16(acl + 6 + 8 * 4, 4);
    g_assert_false(credential_acl_is_private(acl, 44, 1234));
}

static void credential_acl_policy(void)
{
    g_autofree char* path = NULL;
    int fd = g_file_open_tmp("korri-credential-acl-XXXXXX", &path, NULL);
    g_assert_cmpint(fd, >=, 0);
    g_assert_cmpint(fchmod(fd, 0600), ==, 0);
    struct stat st;
    g_assert_cmpint(fstat(fd, &st), ==, 0);
    g_assert_true(credential_is_private(fd, &st));
    g_assert_cmpint(fchmod(fd, 0640), ==, 0);
    g_assert_cmpint(fstat(fd, &st), ==, 0);
    g_assert_false(credential_is_private(fd, &st));

    unsigned char acl[44];
    systemd_acl(acl, geteuid());
    /* The kernel stores the real ACL. Only the owner metadata is supplied
     * explicitly, because this unprivileged test cannot chown a file to root. */
    g_assert_cmpint(fsetxattr(fd, "system.posix_acl_access", acl, sizeof(acl), 0), ==, 0);
    g_assert_cmpint(fstat(fd, &st), ==, 0);
    g_assert_cmpint(st.st_mode & 0777, ==, 0440);
    st.st_uid = 0;
    g_assert_true(credential_is_private(fd, &st));
    st.st_mode = S_IFREG | 0640;
    g_assert_false(credential_is_private(fd, &st));
    st.st_mode = S_IFREG | 0440;
    st.st_uid = geteuid() + 1;
    g_assert_false(credential_is_private(fd, &st));
    st.st_uid = 0;
    write_le16(acl + 6 + 8 * 2, 4);
    g_assert_cmpint(fsetxattr(fd, "system.posix_acl_access", acl, sizeof(acl), 0), ==, 0);
    g_assert_false(credential_is_private(fd, &st));
    close(fd);
    g_assert_cmpint(unlink(path), ==, 0);
}

static void missing_wayland(void)
{
    g_autofree char* directory = g_dir_make_tmp("korri-wayland-failure-XXXXXX", NULL);
    g_assert_nonnull(directory);
    g_autofree char* path = g_build_filename(directory, "KORRID_RPC_CAPABILITY", NULL);
    g_assert_true(g_file_set_contents(path, "test-capability", -1, NULL));
    g_assert_cmpint(chmod(path, 0600), ==, 0);
    g_autofree char* notify_path = g_build_filename(directory, "notify", NULL);
    int notify = socket(AF_UNIX, SOCK_DGRAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0);
    g_assert_cmpint(notify, >=, 0);
    struct sockaddr_un notify_address = { .sun_family = AF_UNIX };
    g_assert_cmpuint(strlen(notify_path), <, sizeof(notify_address.sun_path));
    memcpy(notify_address.sun_path, notify_path, strlen(notify_path) + 1);
    g_assert_cmpint(bind(notify, (struct sockaddr*)&notify_address, sizeof(notify_address)), ==, 0);

    int server = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
    g_assert_cmpint(server, >=, 0);
    struct sockaddr_in address = { .sin_family = AF_INET, .sin_addr.s_addr = htonl(INADDR_LOOPBACK) };
    g_assert_cmpint(bind(server, (struct sockaddr*)&address, sizeof(address)), ==, 0);
    socklen_t length = sizeof(address);
    g_assert_cmpint(getsockname(server, (struct sockaddr*)&address, &length), ==, 0);
    g_assert_cmpint(listen(server, 1), ==, 0);
    g_autofree char* test_origin = g_strdup_printf("http://127.0.0.1:%u", ntohs(address.sin_port));
    g_autofree char* url = g_strconcat(test_origin, "/", NULL);
    pid_t browser = fork();
    g_assert_cmpint(browser, >=, 0);
    if (!browser) {
        g_setenv("HOME", directory, TRUE);
        g_setenv("XDG_RUNTIME_DIR", directory, TRUE);
        g_setenv("CREDENTIALS_DIRECTORY", directory, TRUE);
        g_setenv("KORRID_ADDRESS", "127.0.0.1:7887", TRUE);
        g_setenv("KORRID_PORTAL_ORIGIN", test_origin, TRUE);
        g_setenv("KORRI_WEB_SURFACE_URL", url, TRUE);
        g_setenv("NOTIFY_SOCKET", notify_path, TRUE);
        g_setenv("WPE_PLATFORM", "wayland", TRUE);
        g_setenv("WAYLAND_DISPLAY", "missing-wayland-socket", TRUE);
        /* This conflicting engine setting must not override the selection. */
        g_setenv("WPE_DISPLAY", "wpe-display-headless", TRUE);
        execl(launcher, launcher, NULL);
        _exit(127);
    }
    /* A real loopback page consumes both getters if the engine falls back to
     * headless. It must never reach this server with a missing Wayland socket. */
    struct pollfd descriptors[] = { { server, POLLIN, 0 }, { notify, POLLIN, 0 } };
    int status = 0;
    gboolean exited = FALSE;
    gboolean requested = FALSE;
    gboolean notified = FALSE;
    for (int i = 0; i < 300; i++) {
        if (waitpid(browser, &status, WNOHANG) == browser) {
            exited = TRUE;
            break;
        }
        g_assert_cmpint(poll(descriptors, 2, 10), >=, 0);
        if (descriptors[0].revents & POLLIN) {
            requested = TRUE;
            int client = accept4(server, NULL, NULL, SOCK_CLOEXEC);
            g_assert_cmpint(client, >=, 0);
            const char response[] = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n"
                                    "<script>KorriRpc.korridPort();KorriRpc.korridCapability()</script>";
            g_assert_cmpint(send(client, response, sizeof(response) - 1, MSG_NOSIGNAL), ==, sizeof(response) - 1);
            close(client);
        }
        if (descriptors[1].revents & POLLIN) {
            notified = TRUE;
            break;
        }
    }
    if (!exited) {
        kill(browser, SIGTERM);
        g_assert_cmpint(waitpid(browser, &status, 0), ==, browser);
    }
    close(server);
    close(notify);
    g_assert_cmpint(unlink(path), ==, 0);
    g_assert_cmpint(unlink(notify_path), ==, 0);
    if (rmdir(directory))
        g_test_message("Browser-created data retained in %s", directory);
    g_assert_false(notified);
    g_assert_false(requested);
    g_assert_true(exited);
    g_assert_true(WIFEXITED(status));
    g_assert_cmpint(WEXITSTATUS(status), ==, 1);
}

int main(int argc, char** argv)
{
    struct rlimit no_core = { 0, 0 };
    g_assert_cmpint(setrlimit(RLIMIT_CORE, &no_core), ==, 0);
    g_assert_cmpint(argc, >=, 2);
    launcher = argv[1];
    /* Remove the launcher argument before GLib parses test options. */
    for (int i = 1; i < argc; i++)
        argv[i] = argv[i + 1];
    argc--;
    g_test_init(&argc, &argv, NULL);
    g_test_add_func("/credential/acl-parser", acl_parser);
    g_test_add_func("/credential/systemd-acl-policy", credential_acl_policy);
    g_test_add_func("/display/missing-wayland", missing_wayland);
    return g_test_run();
}
