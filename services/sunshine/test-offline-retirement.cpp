#include "crypto.h"
#include "korri_certificate_control.h"
#include "offline-retirement.h"

#include <array>
#include <boost/property_tree/json_parser.hpp>
#include <cassert>
#include <cerrno>
#include <cstdarg>
#include <cstdio>
#include <fcntl.h>
#include <filesystem>
#include <fstream>
#include <nlohmann/json.hpp>
#include <sstream>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <sys/xattr.h>
#include <unistd.h>

namespace fs = std::filesystem;
namespace kc = korri_certificate_control;
namespace retirement = sunshine_offline_retirement;

static std::string read(const fs::path &path) {
  std::ifstream stream(path, std::ios::binary);
  assert(stream.good());
  return {std::istreambuf_iterator<char>(stream), {}};
}

static void write(const fs::path &path, const std::string &data) {
  std::ofstream stream(path, std::ios::binary);
  stream << data;
  stream.close();
  assert(stream.good());
  assert(::chmod(path.c_str(), 0600) == 0);
}

static auto reopen(const fs::path &path) {
  boost::property_tree::ptree tree;
  boost::property_tree::read_json(path.string(), tree);
  return kc::parse_client_state(tree.get_child("root"), [] {
    return "imported";
  });
}

static void assert_trust(const fs::path &path, const std::string &certificate, bool accepted) {
  crypto::cert_chain_t chain;
  for (const auto &entry : reopen(path)) {
    chain.add(crypto::x509(entry.certificate));
  }
  auto parsed = crypto::x509(certificate);
  assert(parsed);
  assert((chain.verify(parsed.get()) == nullptr) == accepted);
}

// Linker interception observes real filesystem effects, also in killed children.
// The installed CLI has no fault switches. Its unchanged entry source is linked
// a second time into this test so output failures can share the same write audit.
enum class io_fault {
  none,
  temporary_close,
  rename_after,
  directory_open,
  directory_sync,
  directory_sync_after,
  directory_close,
  reopen,
  verification_read,
};

struct io_audit {
  bool active = false;
  io_fault fault = io_fault::none;
  bool triggered = false;
  int interruption = 0;
  int temporary_files = 0;
  int writes = 0;
  int renames = 0;
  int successful_renames = 0;
  int target_removals = 0;
  int file_syncs = 0;
  int directory_syncs = 0;
  int temporary_fd = -1;
  int synced_directory_fd = -1;
  int reopened_fd = -1;
};

static io_audit *audit = nullptr;

static bool auditing() {
  return audit && audit->active;
}

static bool fail(io_fault fault) {
  if (auditing() && audit->fault == fault && !audit->triggered) {
    audit->triggered = true;
    errno = EIO;
    return true;
  }
  return false;
}

static void interrupt_at(int stage) {
  if (auditing() && audit->interruption == stage) {
    ::raise(SIGSTOP);
  }
}

extern int sunshine_retirement_cli_main(int, char **);
extern "C" int __real_mkostemp(char *, int);
extern "C" int __real_open(const char *, int, ...);
extern "C" int __real_openat(int, const char *, int, ...);
extern "C" int __real_close(int);
extern "C" int __real_fsync(int);
extern "C" int __real_rename(const char *, const char *);
extern "C" int __real_unlink(const char *);
extern "C" int __real_fflush(FILE *);
extern "C" ssize_t __real_write(int, const void *, size_t);
extern "C" ssize_t __real_read(int, void *, size_t);

extern "C" int __wrap_mkostemp(char *pattern, int flags) {
  const auto fd = __real_mkostemp(pattern, flags);
  if (auditing()) {
    ++audit->temporary_files;
    audit->temporary_fd = fd;
  }
  return fd;
}

extern "C" ssize_t __wrap_write(int fd, const void *bytes, size_t size) {
  if (auditing()) {
    ++audit->writes;
    if (audit->interruption == 3 && size > 1) {
      const auto result = __real_write(fd, bytes, size / 2);
      assert(result > 0);
      interrupt_at(3);
      return result;
    }
  }
  return __real_write(fd, bytes, size);
}

extern "C" int __wrap_fsync(int fd) {
  struct stat info {};
  assert(::fstat(fd, &info) == 0);
  const bool directory = S_ISDIR(info.st_mode);
  interrupt_at(1);
  if (directory && fail(io_fault::directory_sync)) {
    return -1;
  }
  const auto result = __real_fsync(fd);
  if (auditing() && result == 0) {
    if (directory) {
      ++audit->directory_syncs;
      audit->synced_directory_fd = fd;
      interrupt_at(4);
      if (fail(io_fault::directory_sync_after)) {
        return -1;
      }
    } else {
      ++audit->file_syncs;
    }
  }
  return result;
}

extern "C" int __wrap_close(int fd) {
  const auto result = __real_close(fd);
  if (auditing() && result == 0) {
    if (fd == audit->temporary_fd) {
      audit->temporary_fd = -1;
      if (fail(io_fault::temporary_close)) {
        assert(audit->file_syncs == 1);
        return -1;
      }
    }
    if (fd == audit->synced_directory_fd) {
      audit->synced_directory_fd = -1;
      if (fail(io_fault::directory_close)) {
        assert(audit->directory_syncs == 1);
        return -1;
      }
    }
  }
  return result;
}

extern "C" int __wrap_rename(const char *from, const char *to) {
  if (auditing()) {
    ++audit->renames;
  }
  const int result = __real_rename(from, to);
  if (auditing() && result == 0) {
    ++audit->successful_renames;
    interrupt_at(2);
    if (fail(io_fault::rename_after)) {
      return -1;
    }
  }
  return result;
}

extern "C" int __wrap_unlink(const char *path) {
  if (auditing() && fs::path(path).filename() == "sunshine_state.json") {
    ++audit->target_removals;
  }
  return __real_unlink(path);
}

extern "C" int __wrap_open(const char *path, int flags, ...) {
  if (auditing() && audit->successful_renames && (flags & O_DIRECTORY) && fail(io_fault::directory_open)) {
    return -1;
  }
  if (flags & O_CREAT) {
    va_list args;
    va_start(args, flags);
    const auto mode = va_arg(args, mode_t);
    va_end(args);
    return __real_open(path, flags, mode);
  }
  return __real_open(path, flags);
}

extern "C" int __wrap_openat(int parent, const char *path, int flags, ...) {
  const bool reopening = auditing() && audit->successful_renames && !(flags & O_DIRECTORY);
  if (reopening && fail(io_fault::reopen)) {
    return -1;
  }
  int fd;
  if (flags & O_CREAT) {
    va_list args;
    va_start(args, flags);
    const auto mode = va_arg(args, mode_t);
    va_end(args);
    fd = __real_openat(parent, path, flags, mode);
  } else {
    fd = __real_openat(parent, path, flags);
  }
  if (reopening && fd >= 0) {
    audit->reopened_fd = fd;
    interrupt_at(5);
  }
  return fd;
}

extern "C" ssize_t __wrap_read(int fd, void *bytes, size_t size) {
  if (auditing() && fd == audit->reopened_fd && fail(io_fault::verification_read)) {
    return -1;
  }
  return __real_read(fd, bytes, size);
}

extern "C" int __wrap_fflush(FILE *stream) {
  if (stream == stdout) {
    interrupt_at(6);
  }
  const auto result = __real_fflush(stream);
  if (stream == stdout && result == 0) {
    interrupt_at(7);
  }
  return result;
}

int main(int argc, char **argv) {
  assert(argc == 7 || (argc == 8 && std::string_view(argv[7]) == "--sandbox-no-xattrs"));
  audit = static_cast<io_audit *>(::mmap(nullptr, sizeof(io_audit), PROT_READ | PROT_WRITE, MAP_SHARED | MAP_ANONYMOUS, -1, 0));
  assert(audit != MAP_FAILED);
  *audit = {};
  const auto start_audit = [] {
    *audit = {};
    audit->active = true;
  };
  const auto assert_single_replacement = [] {
    assert(!audit->active);
    assert(audit->temporary_files == 1 && audit->writes == 1);
    assert(audit->renames == 1 && audit->successful_renames == 1);
    assert(audit->target_removals == 0);
  };
  const auto certificate = read(argv[1]);
  const auto second = read(argv[2]);
  const auto server = read(argv[3]);
  const auto key = read(argv[4]);
  std::array<char, 64> pattern {};
  const std::string prefix = std::string(argv[6]) + "/sunshine-retirement-XXXXXX";
  assert(prefix.size() < pattern.size());
  std::copy(prefix.begin(), prefix.end(), pattern.begin());
  assert(::mkdtemp(pattern.data()));
  const fs::path directory(pattern.data());
  const auto state = directory / "sunshine_state.json";
  const auto credentials = directory / "server.crt";
  const auto private_key = directory / "server.key";
  write(credentials, server);
  write(private_key, key);
  write(directory / "apps.json", "unchanged games\n");
  write(directory / "history", "unchanged history\n");
  struct stat cert_before {}, key_before {};
  assert(::lstat(credentials.c_str(), &cert_before) == 0);
  assert(::lstat(private_key.c_str(), &key_before) == 0);

  // The state shape comes from patch 0020's build_state_document and Sunshine's
  // load_state legacy devices/certs reader, not a production-state copy.
  auto original_json = nlohmann::json::parse(kc::build_state_document(R"({"username":"unchanged","password":"synthetic-hash","salt":"synthetic-salt","root":{"unknown":{"value":7}},"history":[1,2,3]})", "aabbccdd-0000-4000-8000-112233445566", {{"manual", "client-1", certificate, {}}}));
  original_json["root"]["devices"] = nlohmann::json::array({{{"uniqueid", "old-client"}, {"certs", nlohmann::json::array({second})}}});
  const auto original = original_json.dump() + "\n";
  const auto candidate = kc::build_state_document(original, original_json["root"]["uniqueid"].get<std::string>(), {});
  retirement::request request {state.string(), original_json["root"]["uniqueid"], retirement::sha256(original)};
  const auto reset = [&] {
    write(state, original);
  };
  const auto refuse = [&](const retirement::request &input) {
    bool rejected = false;
    try {
      (void) retirement::retire_all_clients(input);
    } catch (const retirement::error &error) {
      rejected = !error.mutation_attempted;
    }
    assert(rejected);
    assert(read(state) == original);
  };

  reset();
  assert(reopen(state).size() == 2);
  assert_trust(state, certificate, true);
  assert_trust(state, second, true);
  auto wrong = request;
  wrong.expected_host_uuid = "another-host";
  refuse(wrong);
  wrong = request;
  wrong.expected_state_sha256 = std::string(64, '0');
  refuse(wrong);

  for (const auto mode : {0644, 0660, 0400, 01600}) {
    assert(::chmod(state.c_str(), mode) == 0);
    refuse(request);
    assert(::chmod(state.c_str(), 0600) == 0);
  }
  assert(::chmod(directory.c_str(), 0755) == 0);
  refuse(request);
  assert(::chmod(directory.c_str(), 0700) == 0);
  fs::create_hard_link(state, directory / "hardlink");
  refuse(request);
  fs::remove(directory / "hardlink");
  fs::rename(state, directory / "actual");
  fs::create_symlink("actual", state);
  refuse(request);
  fs::remove(state);
  fs::rename(directory / "actual", state);
  fs::create_directory_symlink(directory, directory.string() + "-link");
  wrong = request;
  wrong.state_path = directory.string() + "-link/sunshine_state.json";
  refuse(wrong);
  fs::remove(directory.string() + "-link");
  write(directory / "sunshine_state.json.korri-residue", "interrupted\n");
  refuse(request);
  fs::remove(directory / "sunshine_state.json.korri-residue");
  fs::rename(state, directory / "original");
  for (const bool fifo : {false, true}) {
    if (fifo) {
      assert(::mkfifo(state.c_str(), 0600) == 0);
    }
    bool rejected = false;
    try {
      (void) retirement::retire_all_clients(request);
    } catch (const retirement::error &error) {
      rejected = !error.mutation_attempted;
    }
    assert(rejected);
    if (fifo) {
      fs::remove(state);
    } else {
      assert(!fs::exists(state));
    }
    assert(read(directory / "original") == original);
  }
  fs::rename(directory / "original", state);
  if (argc == 7) {
    assert(::setxattr(state.c_str(), "user.retirement-test", "x", 1, 0) == 0);
    refuse(request);
    assert(::removexattr(state.c_str(), "user.retirement-test") == 0);
  }

  for (const auto &malformed : std::vector<std::string> {
         "{",
         "[]",
         "{}",
         R"({"root":{"uniqueid":"a","named_devices":42}})",
         R"({"root":{"uniqueid":"a","named_devices":[]},"duplicate":1,"duplicate":2})",
         R"({"root":{"uniqueid":"a"},"large":18446744073709551616})",
         R"({"root":{"uniqueid":"a"},"fraction":0.1})",
         std::string(65, '[') + "0" + std::string(65, ']'),
         kc::build_state_document("{}", request.expected_host_uuid, {{"broken", "id", "not a certificate", {}}}),
         std::string(retirement::max_state_bytes + 1, 'x'),
       }) {
    write(state, malformed);
    auto bad = request;
    bad.expected_state_sha256 = retirement::sha256(malformed);
    bool rejected = false;
    try {
      (void) retirement::retire_all_clients(bad);
    } catch (const retirement::error &error) {
      rejected = !error.mutation_attempted;
    }
    assert(rejected);
    assert(read(state) == malformed);
  }

  const auto expect_uncertain = [&](kc::fault_plan *fault = nullptr) {
    bool rejected = false;
    try {
      (void) retirement::retire_all_clients(request, fault);
    } catch (const retirement::error &error) {
      rejected = error.mutation_attempted;
    }
    audit->active = false;
    assert(rejected);
  };
  const auto assert_retired = [&] {
    // Exact bytes and the real reopened crypto verifier, not just an empty count.
    assert(read(state) == candidate);
    assert_trust(state, certificate, false);
    assert_trust(state, second, false);
  };
  for (const auto stage : {kc::fault_stage::write, kc::fault_stage::file_sync, kc::fault_stage::rename}) {
    reset();
    start_audit();
    kc::fault_plan fault {stage};
    expect_uncertain(&fault);
    assert(fault.triggered && !fault.recovery_triggered);
    assert(audit->temporary_files == 1 && audit->successful_renames == 0 && audit->target_removals == 0);
    assert(read(state) == original);
    assert_trust(state, certificate, true);
    assert_trust(state, second, true);
  }
  reset();
  start_audit();
  audit->fault = io_fault::temporary_close;
  expect_uncertain();
  assert(audit->triggered && audit->file_syncs == 1 && audit->renames == 0);
  assert(audit->temporary_files == 1 && audit->writes == 1 && audit->target_removals == 0);
  assert(read(state) == original);
  assert_trust(state, certificate, true);
  assert_trust(state, second, true);

  for (const auto stage : {kc::fault_stage::none, kc::fault_stage::restore_write, kc::fault_stage::restore_file_sync, kc::fault_stage::restore_rename, kc::fault_stage::restore_directory_sync}) {
    reset();
    start_audit();
    kc::fault_plan fault {kc::fault_stage::directory_sync, stage};
    expect_uncertain(&fault);
    assert(fault.triggered && !fault.recovery_triggered);
    assert_single_replacement();
    assert_retired();
  }
  for (const auto fault : {io_fault::rename_after, io_fault::directory_open, io_fault::directory_sync, io_fault::directory_sync_after, io_fault::directory_close, io_fault::reopen, io_fault::verification_read}) {
    reset();
    start_audit();
    audit->fault = fault;
    expect_uncertain();
    assert(audit->triggered);
    assert_single_replacement();
    assert_retired();
    if (fault == io_fault::directory_sync_after || fault == io_fault::directory_close || fault == io_fault::reopen || fault == io_fault::verification_read) {
      assert(audit->directory_syncs == 1);
    }
  }
  // Offline retirement has no activation or restoration phase. Online faults
  // retain their original meaning in the unchanged test-certificate-control.
  for (const auto stage : {kc::fault_stage::activation, kc::fault_stage::restore_write, kc::fault_stage::restore_file_sync, kc::fault_stage::restore_rename, kc::fault_stage::restore_directory_sync}) {
    reset();
    start_audit();
    kc::fault_plan fault {stage};
    assert(retirement::retire_all_clients(request, &fault).retired_clients == 2);
    audit->active = false;
    assert(!fault.triggered && !fault.recovery_triggered);
    assert_single_replacement();
    assert_retired();
  }

  for (int stage : {1, 2, 3, 4, 5}) {
    reset();
    start_audit();
    audit->interruption = stage;
    const pid_t child = ::fork();
    assert(child >= 0);
    if (child == 0) {
      (void) retirement::retire_all_clients(request);
      ::_exit(99);
    }
    int status = 0;
    assert(::waitpid(child, &status, WUNTRACED) == child);
    assert(WIFSTOPPED(status));
    assert(::kill(child, SIGKILL) == 0);
    assert(::waitpid(child, &status, 0) == child && WIFSIGNALED(status) && WTERMSIG(status) == SIGKILL);
    audit->active = false;
    const bool renamed = stage == 2 || stage == 4 || stage == 5;
    assert(read(state) == (renamed ? candidate : original));
    assert_trust(state, certificate, !renamed);
    assert_trust(state, second, !renamed);
    if (renamed) {
      assert_single_replacement();
    } else {
      assert(audit->temporary_files == 1 && audit->writes == 1 && audit->renames == 0 && audit->target_removals == 0);
      refuse(request);
      for (const auto &entry : fs::directory_iterator(directory)) {
        if (entry.path().filename().string().starts_with("sunshine_state.json.korri-")) {
          struct stat temporary_info {};
          assert(::lstat(entry.path().c_str(), &temporary_info) == 0);
          assert(S_ISREG(temporary_info.st_mode) && (temporary_info.st_mode & 07777) == 0600);
          assert(temporary_info.st_uid == ::getuid() && temporary_info.st_gid == ::getgid() && temporary_info.st_nlink == 1);
          if (stage == 3) {
            assert(read(entry.path()) == candidate.substr(0, candidate.size() / 2));
          }
          fs::remove(entry);
        }
      }
    }
  }

  reset();
  const auto result = retirement::retire_all_clients(request);
  assert(result.retired_clients == 2);
  assert(result.state_sha256 == retirement::sha256(read(state)));
  assert(read(state) == candidate);
  assert(reopen(state).empty());
  assert_trust(state, certificate, false);
  assert_trust(state, second, false);
  auto preserved = nlohmann::json::parse(read(state));
  original_json["root"].erase("devices");
  original_json["root"]["named_devices"] = nlohmann::json::array();
  assert(preserved == original_json);
  struct stat state_after {}, cert_after {}, key_after {};
  assert(::lstat(state.c_str(), &state_after) == 0);
  assert((state_after.st_mode & 07777) == 0600);
  assert(state_after.st_uid == ::getuid() && state_after.st_gid == ::getgid());
  assert(::lstat(credentials.c_str(), &cert_after) == 0);
  assert(::lstat(private_key.c_str(), &key_after) == 0);
  for (const auto &[before, after] : {std::pair {cert_before, cert_after}, std::pair {key_before, key_after}}) {
    assert(before.st_dev == after.st_dev && before.st_ino == after.st_ino);
    assert(before.st_mode == after.st_mode && before.st_uid == after.st_uid && before.st_gid == after.st_gid);
    assert(before.st_nlink == after.st_nlink && before.st_size == after.st_size);
    assert(before.st_mtim.tv_sec == after.st_mtim.tv_sec && before.st_mtim.tv_nsec == after.st_mtim.tv_nsec);
    assert(before.st_ctim.tv_sec == after.st_ctim.tv_sec && before.st_ctim.tv_nsec == after.st_ctim.tv_nsec);
  }
  assert(read(credentials) == server && read(private_key) == key);
  assert(read(directory / "apps.json") == "unchanged games\n");
  assert(read(directory / "history") == "unchanged history\n");
  request.expected_state_sha256 = result.state_sha256;
  assert(retirement::retire_all_clients(request).retired_clients == 0);
  const auto historical_empty = kc::build_state_document("{}", request.expected_host_uuid, {});
  auto historical_json = nlohmann::json::parse(historical_empty);
  historical_json["root"]["devices"] = "";
  historical_json["root"]["named_devices"] = "";
  write(state, historical_json.dump());
  request.expected_state_sha256 = retirement::sha256(read(state));
  assert(retirement::retire_all_clients(request).retired_clients == 0);
  assert(reopen(state).empty());

  // Exercise the installed command, not a test-only argument parser.
  reset();
  enum class cli_entry {
    installed,
    audited
  };
  const auto run_cli = [&](std::vector<std::string> arguments, cli_entry entry = cli_entry::installed, int output_fd = STDOUT_FILENO) {
    assert(std::fflush(nullptr) == 0);
    const pid_t cli = ::fork();
    assert(cli >= 0);
    if (cli == 0) {
      assert(::signal(SIGPIPE, SIG_DFL) != SIG_ERR);
      if (output_fd != STDOUT_FILENO) {
        assert(::dup2(output_fd, STDOUT_FILENO) == STDOUT_FILENO);
        assert(::close(output_fd) == 0);
      }
      std::vector<char *> values;
      for (auto &argument : arguments) {
        values.push_back(argument.data());
      }
      values.push_back(nullptr);
      if (entry == cli_entry::audited) {
        ::_exit(sunshine_retirement_cli_main(static_cast<int>(arguments.size()), values.data()));
      }
      ::execv(argv[5], values.data());
      ::_exit(98);
    }
    int status = 0;
    assert(::waitpid(cli, &status, WUNTRACED) == cli);
    if (WIFSTOPPED(status)) {
      assert(audit->active && (audit->interruption == 6 || audit->interruption == 7));
      assert(::kill(cli, SIGKILL) == 0);
      assert(::waitpid(cli, &status, 0) == cli);
    }
    audit->active = false;
    return status;
  };
  const auto exit_code = [](int status) {
    assert(WIFEXITED(status));
    return WEXITSTATUS(status);
  };
  std::vector<std::string> arguments {argv[5], "--state", state.string(), "--expect-host-uuid", request.expected_host_uuid, "--expect-state-sha256", retirement::sha256(original)};
  assert(exit_code(run_cli(arguments)) == 2);
  assert(read(state) == original);
  arguments.push_back("--exclusive-quiescence-confirmed");
  auto duplicate = arguments;
  duplicate.push_back("--exclusive-quiescence-confirmed");
  assert(exit_code(run_cli(duplicate)) == 2);
  assert(read(state) == original);
  auto mismatch = arguments;
  mismatch[6] = std::string(64, '0');
  assert(exit_code(run_cli(mismatch)) == 2);
  assert(read(state) == original);
  assert(exit_code(run_cli(arguments)) == 0);
  assert_retired();

  for (const auto fault : {io_fault::directory_close, io_fault::reopen}) {
    reset();
    start_audit();
    audit->fault = fault;
    assert(exit_code(run_cli(arguments, cli_entry::audited)) == 3);
    assert(audit->triggered && audit->directory_syncs == 1);
    assert_single_replacement();
    assert_retired();
  }
  // Real output errors in both the installed executable and the same CLI source
  // with syscall auditing. libc's output writes are not part of the state audit.
  for (const auto entry : {cli_entry::installed, cli_entry::audited}) {
    reset();
    const int full = ::open("/dev/full", O_WRONLY | O_CLOEXEC);
    assert(full >= 0);
    if (entry == cli_entry::audited) {
      start_audit();
    }
    assert(exit_code(run_cli(arguments, entry, full)) == 3);
    assert(::close(full) == 0);
    assert_retired();
    if (entry == cli_entry::audited) {
      assert_single_replacement();
    }

    reset();
    int pipe_fds[2];
    assert(::pipe2(pipe_fds, O_CLOEXEC) == 0);
    assert(::close(pipe_fds[0]) == 0);
    if (entry == cli_entry::audited) {
      start_audit();
    }
    const auto status = run_cli(arguments, entry, pipe_fds[1]);
    assert(WIFSIGNALED(status) && WTERMSIG(status) == SIGPIPE);
    assert(::close(pipe_fds[1]) == 0);
    assert_retired();
    if (entry == cli_entry::audited) {
      assert_single_replacement();
    }
  }
  for (const int stage : {6, 7}) {
    reset();
    const int output = ::open("/dev/null", O_WRONLY | O_CLOEXEC);
    assert(output >= 0);
    start_audit();
    audit->interruption = stage;
    const auto status = run_cli(arguments, cli_entry::audited, output);
    assert(WIFSIGNALED(status) && WTERMSIG(status) == SIGKILL);
    assert(::close(output) == 0);
    assert(audit->directory_syncs == 1);
    assert_single_replacement();
    assert_retired();
  }
  fs::remove_all(directory);
  assert(::munmap(audit, sizeof(io_audit)) == 0);
  audit = nullptr;
  std::puts("PASS: pre-rename OLD; post-rename NEW; one candidate write/rename; no restoration; real reopened verifier; CLI uncertainty/output/interruption");
}
