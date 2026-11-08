// AppManager's executable is also the in-process NDL test entry point.
#define _XOPEN_SOURCE 700
#define _DEFAULT_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <ftw.h>
#include <limits.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

#ifdef KORRI_MEDIA_TEST
int korri_media_test_main(int argc, char **argv);
#endif

static pid_t brain_pid, shell_pid;
static volatile sig_atomic_t interrupted;
static char work[PATH_MAX];

static void fail(const char *message) {
  perror(message);
  exit(1);
}

static void environment(const char *name, const char *value) {
  if (setenv(name, value, 1) != 0)
    fail(name);
}

static void on_signal(int sig) {
  (void)sig;
  interrupted = 1;
}

static int remove_entry(const char *path, const struct stat *st, int type,
                        struct FTW *info) {
  (void)st;
  (void)type;
  (void)info;
  return remove(path);
}

static void cleanup(void) {
  // Retain child ownership until waitpid: never kill a reaped/reused PID.
  if (shell_pid > 0)
    kill(shell_pid, SIGTERM);
  if (brain_pid > 0)
    kill(brain_pid, SIGTERM);
  while (shell_pid > 0 && waitpid(shell_pid, NULL, 0) < 0 && errno == EINTR) {
  }
  while (brain_pid > 0 && waitpid(brain_pid, NULL, 0) < 0 && errno == EINTR) {
  }
  if (work[0] && nftw(work, remove_entry, 16, FTW_DEPTH | FTW_PHYS) != 0)
    perror("remove launch directory");
}

static void check_signal(void) {
  if (interrupted)
    exit(143); // Preserve launch.sh's TERM/HUP/INT status.
}

static void path_join(char *out, size_t size, const char *dir,
                      const char *name) {
  int length = snprintf(out, size, "%s/%s", dir, name);
  if (length < 0 || (size_t)length >= size) {
    errno = ENAMETOOLONG;
    fail("launch path");
  }
}

static char *read_file(const char *path) {
  FILE *file = fopen(path, "r");
  if (!file)
    fail(path);
  char *value = calloc(65537, 1);
  if (!value)
    fail("allocate launch data");
  size_t size = fread(value, 1, 65536, file);
  if (ferror(file) || size == 65536) {
    fprintf(stderr, "cannot read bounded launch data: %s\n", path);
    exit(1);
  }
  if (fclose(file) != 0)
    fail(path);
  // Command substitution in launch.sh strips trailing newlines.
  while (size && value[size - 1] == '\n')
    value[--size] = '\0';
  return value;
}

static pid_t spawn(char *const argv[], const char *connection,
                   const char *relays) {
  check_signal();
  pid_t pid = fork();
  if (pid < 0)
    fail("fork");
  if (pid == 0) {
    // Children inherit diagnostics, but not the launcher's signal handler.
    signal(SIGTERM, SIG_DFL);
    signal(SIGINT, SIG_DFL);
    signal(SIGHUP, SIG_DFL);
    if (connection) {
      int input = open("/dev/null", O_RDONLY);
      int output = open(connection, O_WRONLY | O_CREAT | O_TRUNC, 0600);
      if (input < 0 || output < 0 || dup2(input, STDIN_FILENO) < 0 ||
          dup2(output, STDOUT_FILENO) < 0)
        _exit(1);
      if (input > STDERR_FILENO)
        close(input);
      if (output > STDERR_FILENO)
        close(output);
    }
    if (relays && setenv("KORRID_RELAYS", relays, 1) != 0)
      _exit(1);
    execv(argv[0], argv);
    perror(argv[0]);
    _exit(127);
  }
  return pid;
}

static bool connection_ready(const char *path) {
  FILE *file = fopen(path, "r");
  if (!file) {
    if (errno == ENOENT)
      return false;
    fail(path);
  }
  char line[4096];
  bool ready = false;
  while (fgets(line, sizeof(line), file)) {
    // Read only these public values. No shell evaluation or capability export.
    const char *keys[] = {"KORRID_ADDRESS", "KORRID_SHELL_ORIGIN"};
    for (size_t i = 0; i < 2; ++i) {
      size_t length = strlen(keys[i]);
      if (strncmp(line, keys[i], length) == 0 && line[length] == '=' &&
          strchr(line, '\n')) {
        line[strcspn(line, "\n")] = '\0';
        environment(keys[i], line + length + 1);
        if (i == 1)
          ready = true;
      }
    }
  }
  if (ferror(file))
    fail(path);
  if (fclose(file) != 0)
    fail(path);
  return ready;
}

int main(int argc, char **argv) {
  umask(077);
  struct rlimit core = {0, 0};
  if (setrlimit(RLIMIT_CORE, &core) != 0)
    fail("disable core dumps");
  if (geteuid() == 0) {
    fprintf(stderr, "korri-shell: launcher refuses root\n");
    return 1;
  }
  if (argc > 1 && strcmp(argv[1], "media-test") == 0) {
#ifdef KORRI_MEDIA_TEST
    // Same process and executable path as appinfo main; no LS2 role edits.
    unsetenv("CREDENTIALS_DIRECTORY");
    unsetenv("KORRID_ADDRESS");
    unsetenv("KORRID_SHELL_ORIGIN");
    return korri_media_test_main(argc - 1, argv + 1);
#else
    fprintf(stderr, "media-test is only available in the ARM package\n");
    return 2;
#endif
  }
  int arg = 1;
  bool fixture = argc > arg && strcmp(argv[arg], "--fixture") == 0;
  if (fixture)
    ++arg;
  const char *surfaces = "zone,pico";
  if (argc > arg && strcmp(argv[arg], "--surfaces") == 0) {
    if (argc != arg + 2) {
      fprintf(stderr, "usage: korri-webos [--fixture] [--surfaces ID,ID]\n");
      return 2;
    }
    surfaces = argv[arg + 1];
    if (strcmp(surfaces, "zone") && strcmp(surfaces, "pico") &&
        strcmp(surfaces, "zone,pico") && strcmp(surfaces, "pico,zone")) {
      fprintf(stderr, "only packaged surfaces zone and pico are available\n");
      return 2;
    }
  }
  // /proc/self/exe is unambiguous even when AppManager uses a relative argv[0].
  char app[PATH_MAX];
  ssize_t length = readlink("/proc/self/exe", app, sizeof(app) - 1);
  if (length < 0 || (size_t)length >= sizeof(app) - 1)
    fail("launcher executable path");
  app[length] = '\0';
  char *slash = strrchr(app, '/');
  if (!slash)
    return 1;
  *slash = '\0'; // bin
  slash = strrchr(app, '/');
  if (!slash)
    return 1;
  *slash = '\0'; // app
  if (chdir(app) != 0)
    fail(app);
  const char *tmp = getenv("TMPDIR");
  if (!tmp || !*tmp)
    tmp = "/tmp";
  char path[PATH_MAX];
  path_join(path, sizeof(path), tmp, "korri-shell-launch.log");
  int log = open(path, O_WRONLY | O_CREAT | O_APPEND, 0600);
  if (log < 0 || dup2(log, STDERR_FILENO) < 0)
    fail("launcher stderr log");
  if (log != STDERR_FILENO)
    close(log);
  time_t now = time(NULL);
  fprintf(stderr, "korri-shell launch %s", ctime(&now));
  path_join(path, sizeof(path), tmp, "korri-shell.XXXXXXXX");
  if (!mkdtemp(path))
    fail("private launch directory");
  strcpy(work, path);
  if (atexit(cleanup) != 0)
    fail("register launch cleanup");
  struct sigaction action = {.sa_handler = on_signal};
  sigemptyset(&action.sa_mask);
  if (sigaction(SIGTERM, &action, NULL) || sigaction(SIGHUP, &action, NULL) ||
      sigaction(SIGINT, &action, NULL))
    fail("launcher signal handlers");
  path_join(path, sizeof(path), work, "credentials");
  if (mkdir(path, 0700) != 0)
    fail(path);
  environment("CREDENTIALS_DIRECTORY", path);
  environment("APPID", "org.korri.shell");
  environment("SLINT_SCALE_FACTOR", "1.5");
  unsetenv("KORRID_RELAYS");
  char connection[PATH_MAX];
  path_join(connection, sizeof(connection), work, "connection");
  char *relays = fixture ? NULL : read_file("product-relays.json");
  char *brain_args[] = {fixture ? "./bin/fixture-korrid" : "./bin/webos-korrid",
                        fixture ? "--games" : "--credentials",
                        fixture ? "12" : path,
                        fixture ? "--credentials" : NULL,
                        fixture ? path : NULL,
                        NULL};
  brain_pid = spawn(brain_args, connection, relays);
  free(relays);
  environment("KORRID_ADDRESS", "");
  environment("KORRID_SHELL_ORIGIN", "");
  int attempt = 0;
  while (!connection_ready(connection)) {
    check_signal();
    if (waitpid(brain_pid, NULL, WNOHANG) == brain_pid) {
      brain_pid = 0;
      fprintf(stderr, "korrid exited during startup\n");
      return 1;
    }
    if (++attempt >= 100) {
      fprintf(stderr, "korrid startup timed out\n");
      return 1;
    }
    struct timespec delay = {.tv_nsec = 100000000};
    nanosleep(&delay, NULL);
  }
  char *shell_args[] = {"./bin/korri-shell",
                        "--packages",
                        "packages",
                        "--cache",
                        "cache",
                        "--installed",
                        "--surfaces",
                        (char *)surfaces,
                        "--placement",
                        "fullscreen",
                        NULL};
  shell_pid = spawn(shell_args, NULL, NULL);
  int status;
  while (waitpid(shell_pid, &status, 0) < 0) {
    check_signal();
    if (errno != EINTR)
      fail("wait for shell");
  }
  shell_pid = 0;
  check_signal();
  return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}
