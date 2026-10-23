#define _GNU_SOURCE
#include <wpe/webkit.h>
#include <wpe/wpe-platform.h>
#include <glib-unix.h>
#include <arpa/inet.h>
#include <fcntl.h>
#include <stddef.h>
#include <sys/random.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>

static GMainLoop *loop;
static char *origin, *notify_socket, *nonce;
static gboolean ready;
static int exit_status = 1;

static void fail(void)
{
    /* Never include engine errors, URIs, script messages or credentials. */
    fputs("korri-portal-wpe: startup or browser failure\n", stderr);
    if (loop)
        g_main_loop_quit(loop);
    else
        exit(1);
}

static gboolean stop(gpointer data)
{
    (void)data;
    exit_status = 0;
    g_main_loop_quit(loop);
    return G_SOURCE_REMOVE;
}

static gboolean deadline(gpointer data)
{
    (void)data;
    if (!ready)
        fail();
    return G_SOURCE_REMOVE;
}

static gboolean same_origin(const char *url)
{
    g_autoptr(GUri) uri = url ? g_uri_parse(url, G_URI_FLAGS_NONE, NULL) : NULL;
    if (!uri || g_uri_get_userinfo(uri) || !g_uri_get_host(uri) ||
        g_strcmp0(g_uri_get_scheme(uri), "http") ||
        g_strcmp0(g_uri_get_host(uri), "127.0.0.1"))
        return FALSE;
    g_autofree char *candidate = g_strdup_printf("http://127.0.0.1:%d", g_uri_get_port(uri));
    return !strcmp(candidate, origin);
}

static char *credential(void)
{
    const char *directory = g_getenv("CREDENTIALS_DIRECTORY");
    if (!directory || directory[0] != '/')
        return NULL;
    g_autofree char *path = g_build_filename(directory, "KORRID_RPC_CAPABILITY", NULL);
    int fd = open(path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK);
    if (fd < 0)
        return NULL;
    struct stat st;
    char buffer[4097];
    ssize_t length = -1;
    if (!fstat(fd, &st) && S_ISREG(st.st_mode) && st.st_uid == geteuid() &&
        !(st.st_mode & 077) && st.st_size > 0 && st.st_size <= 4096)
        length = read(fd, buffer, sizeof(buffer));
    close(fd);
    if (length <= 0 || length > 4096)
        return NULL;
    if (buffer[length - 1] == '\n')
        length--;
    if (!length)
        return NULL;
    /* The capability producer uses an ASCII bearer token. Reject control
     * characters (including a second newline), NUL and JS metacharacters. */
    for (ssize_t i = 0; i < length; i++)
        if (!(g_ascii_isalnum(buffer[i]) || buffer[i] == '-' || buffer[i] == '_' ||
              buffer[i] == '.' || buffer[i] == '~' || buffer[i] == '+' ||
              buffer[i] == '/' || buffer[i] == '='))
            return NULL;
    return g_strndup(buffer, length);
}

static gboolean notify_ready(void)
{
    if (!notify_socket)
        return TRUE;
    struct sockaddr_un address = { .sun_family = AF_UNIX };
    size_t length = strlen(notify_socket);
    if (!length || length >= sizeof(address.sun_path) ||
        (notify_socket[0] != '/' && notify_socket[0] != '@'))
        return FALSE;
    memcpy(address.sun_path, notify_socket, length + 1);
    socklen_t size = offsetof(struct sockaddr_un, sun_path) + length + 1;
    if (notify_socket[0] == '@') {
        address.sun_path[0] = '\0';
        size--;
    }
    int fd = socket(AF_UNIX, SOCK_DGRAM | SOCK_CLOEXEC, 0);
    if (fd < 0)
        return FALSE;
    gboolean result = sendto(fd, "READY=1", 7, MSG_NOSIGNAL,
                             (struct sockaddr *)&address, size) == 7;
    close(fd);
    return result;
}

static void message(WebKitUserContentManager *manager, JSCValue *value, gpointer data)
{
    (void)manager;
    (void)data;
    if (ready || !jsc_value_is_string(value))
        return;
    g_autofree char *text = jsc_value_to_string(value);
    if (strcmp(text, nonce))
        return;
    if (!notify_ready()) {
        fail();
        return;
    }
    ready = TRUE;
}

static gboolean permission(WebKitWebView *view, WebKitPermissionRequest *request, gpointer data)
{
    (void)view;
    (void)data;
    webkit_permission_request_deny(request);
    return TRUE;
}

static gboolean policy(WebKitWebView *view, WebKitPolicyDecision *decision,
                       WebKitPolicyDecisionType type, gpointer data)
{
    (void)view;
    (void)data;
    if (type == WEBKIT_POLICY_DECISION_TYPE_NEW_WINDOW_ACTION) {
        webkit_policy_decision_ignore(decision);
        return TRUE;
    }
    if (type == WEBKIT_POLICY_DECISION_TYPE_NAVIGATION_ACTION) {
        WebKitNavigationAction *action = webkit_navigation_policy_decision_get_navigation_action(
            WEBKIT_NAVIGATION_POLICY_DECISION(decision));
        const char *uri = webkit_uri_request_get_uri(webkit_navigation_action_get_request(action));
        if (!same_origin(uri)) {
            webkit_policy_decision_ignore(decision);
            return TRUE;
        }
    }
    return FALSE;
}

static WebKitWebView *popup(WebKitWebView *view, WebKitNavigationAction *action, gpointer data)
{
    (void)view;
    (void)action;
    (void)data;
    return NULL;
}

static void terminated(WebKitWebView *view, WebKitWebProcessTerminationReason reason, gpointer data)
{
    (void)view;
    (void)reason;
    (void)data;
    fail();
}

static void scrub_environment(void)
{
    /* Keep only native platform/session inputs. In particular, do not inherit
     * WEBKIT_* debug/sandbox overrides, LD_*, credentials or NOTIFY_SOCKET. */
    static const char *allowed[] = { "HOME", "XDG_RUNTIME_DIR", "WAYLAND_DISPLAY",
        "WPE_PLATFORM", "DBUS_SESSION_BUS_ADDRESS", "LANG", "LC_ALL", "TZ", NULL };
    g_auto(GStrv) names = g_listenv();
    for (char **name = names; *name; name++) {
        gboolean keep = FALSE;
        for (const char **item = allowed; *item; item++)
            keep |= !strcmp(*name, *item);
        if (!keep)
            g_unsetenv(*name);
    }
}

int main(int argc, char **argv)
{
    (void)argv;
    struct rlimit limit = { 0, 0 };
    if (argc != 1 || setrlimit(RLIMIT_CORE, &limit))
        fail();
    umask(077);
    g_set_prgname("korri-portal-wpe");
    g_autofree char *capability = credential();
    origin = g_strdup(g_getenv("KORRID_PORTAL_ORIGIN"));
    g_autofree char *url = g_strdup(g_getenv("KORRI_WEB_SURFACE_URL"));
    const char *address = g_getenv("KORRID_ADDRESS");
    g_autoptr(GSocketAddress) socket_address = address ? g_inet_socket_address_new_from_string(address, 0) : NULL;
    if (!capability || !origin || !url || !socket_address || !same_origin(url))
        fail();
    guint port = g_inet_socket_address_get_port(G_INET_SOCKET_ADDRESS(socket_address));
    g_autoptr(GUri) origin_uri = g_uri_parse(origin, G_URI_FLAGS_NONE, NULL);
    g_autofree char *canonical = origin_uri ? g_strdup_printf("http://127.0.0.1:%d", g_uri_get_port(origin_uri)) : NULL;
    if (!port || !canonical || strcmp(canonical, origin) || g_uri_get_port(origin_uri) <= 0 ||
        g_uri_get_port(origin_uri) > 65535 || !g_str_has_prefix(url, origin) || url[strlen(origin)] != '/')
        fail();
    const char *home = g_getenv("HOME"), *runtime = g_getenv("XDG_RUNTIME_DIR");
    const char *platform = g_getenv("WPE_PLATFORM"), *wayland = g_getenv("WAYLAND_DISPLAY");
    if (!home || home[0] != '/' || !runtime || runtime[0] != '/' ||
        (platform && strcmp(platform, "headless") && strcmp(platform, "wayland")) ||
        ((!platform || strcmp(platform, "headless")) && (!wayland || !*wayland)))
        fail();
    notify_socket = g_strdup(g_getenv("NOTIFY_SOCKET"));
    unsigned char random[32];
    if (getrandom(random, sizeof(random), 0) != sizeof(random))
        fail();
    nonce = g_malloc(65);
    for (size_t i = 0; i < sizeof(random); i++)
        g_snprintf(nonce + i * 2, 3, "%02x", random[i]);
    scrub_environment();

    /* WPE requires a syntactically valid GApplication ID (a dot is mandatory).
     * This also supplies its xdg_toplevel app_id; no public setter exists. */
    g_autoptr(GApplication) app = g_application_new("org.korri.portal", G_APPLICATION_NON_UNIQUE);
    g_application_set_default(app);
    g_autoptr(WebKitUserContentManager) manager = webkit_user_content_manager_new();
    g_signal_connect(manager, "script-message-received::korriReady", G_CALLBACK(message), NULL);
    if (!webkit_user_content_manager_register_script_message_handler(manager, "korriReady", NULL))
        fail();
    g_autofree char *script = g_strdup_printf(
        "(()=>{if(window!==top||location.origin!=='%s')return;"
        "const send=window.webkit.messageHandlers.korriReady.postMessage.bind(window.webkit.messageHandlers.korriReady);"
        "let used=0;const mark=b=>{const old=used;used|=b;if(used===3&&old!==3)send('%s')};"
        "const bridge=Object.freeze({korridPort:()=>{mark(1);return %u},"
        "korridCapability:()=>{mark(2);return '%s'}});"
        "Object.defineProperty(window,'KorriRpc',{value:bridge,enumerable:false,writable:false,configurable:false});})()",
        origin, nonce, port, capability);
    const char *allow[] = { NULL, NULL };
    g_autofree char *pattern = g_strconcat(origin, "/*", NULL);
    allow[0] = pattern;
    WebKitUserScript *user_script = webkit_user_script_new(script,
        WEBKIT_USER_CONTENT_INJECT_TOP_FRAME, WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START, allow, NULL);
    webkit_user_content_manager_add_script(manager, user_script);
    webkit_user_script_unref(user_script);
    memset(capability, 0, strlen(capability));

    g_autoptr(WebKitSettings) settings = webkit_settings_new();
    webkit_settings_set_enable_developer_extras(settings, FALSE);
    webkit_settings_set_enable_write_console_messages_to_stdout(settings, FALSE);
    webkit_settings_set_javascript_can_open_windows_automatically(settings, FALSE);
    g_autoptr(WebKitNetworkSession) session = webkit_network_session_new_ephemeral();
    WebKitWebView *view = WEBKIT_WEB_VIEW(g_object_new(WEBKIT_TYPE_WEB_VIEW,
        "user-content-manager", manager, "settings", settings, "network-session", session, NULL));
    g_signal_connect(view, "permission-request", G_CALLBACK(permission), NULL);
    g_signal_connect(view, "decide-policy", G_CALLBACK(policy), NULL);
    g_signal_connect(view, "create", G_CALLBACK(popup), NULL);
    g_signal_connect(view, "web-process-terminated", G_CALLBACK(terminated), NULL);
    WebKitColor transparent = { 0, 0, 0, 0 };
    webkit_web_view_set_background_color(view, &transparent);
    WPEView *wpe_view = webkit_web_view_get_wpe_view(view);
    if (!wpe_view)
        fail();
    WPEToplevel *toplevel = wpe_view_get_toplevel(wpe_view);
    if (toplevel)
        wpe_toplevel_fullscreen(toplevel);
    loop = g_main_loop_new(NULL, FALSE);
    g_unix_signal_add(SIGTERM, stop, NULL);
    g_unix_signal_add(SIGINT, stop, NULL);
    g_timeout_add_seconds(15, deadline, NULL);
    webkit_web_view_load_uri(view, url);
    g_main_loop_run(loop);
    webkit_web_view_stop_loading(view);
    webkit_web_view_terminate_web_process(view);
    g_object_unref(view);
    g_main_loop_unref(loop);
    g_free(origin);
    g_free(nonce);
    g_free(notify_socket);
    return exit_status;
}
