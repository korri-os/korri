#include "offline-retirement.h"

#include <cstdio>
#include <string_view>
#include <sys/prctl.h>
#include <sys/resource.h>

namespace {
  constexpr const char *usage =
    "sunshine-retire-all-clients --exclusive-quiescence-confirmed --state ABSOLUTE_PATH "
    "--expect-host-uuid UUID --expect-state-sha256 SHA256\n"
    "OFFLINE ONLY. Run as the existing state UID/GID with every Sunshine/state writer excluded.\n"
    "Retires ALL clients. Does not open separate server credential, library or history files, or network endpoints.\n"
    "Exit 0: durable replacement verified. Exit 2: refused before mutation.\n"
    "Exit 3 or interruption: keep authority stopped; inspect state, never restore old trust automatically.\n";
}

int main(int argc, char **argv) {
  if (argc == 2 && std::string_view(argv[1]) == "--help") {
    std::fputs(usage, stdout);
    return 0;
  }
  sunshine_offline_retirement::request input;
  bool quiesced = false;
  for (int index = 1; index < argc; ++index) {
    const std::string_view argument(argv[index]);
    if (argument == "--exclusive-quiescence-confirmed" && !quiesced) {
      quiesced = true;
      continue;
    }
    std::string *destination = nullptr;
    if (argument == "--state") {
      destination = &input.state_path;
    }
    if (argument == "--expect-host-uuid") {
      destination = &input.expected_host_uuid;
    }
    if (argument == "--expect-state-sha256") {
      destination = &input.expected_state_sha256;
    }
    if (!destination || !destination->empty() || ++index >= argc || argv[index][0] == '\0') {
      std::fputs(usage, stderr);
      return 2;
    }
    *destination = argv[index];
  }
  if (!quiesced || input.state_path.empty() || input.expected_host_uuid.empty() || input.expected_state_sha256.empty()) {
    std::fputs(usage, stderr);
    return 2;
  }
  const rlimit no_core {0, 0};
  if (::setrlimit(RLIMIT_CORE, &no_core) != 0 || ::prctl(PR_SET_DUMPABLE, 0) != 0) {
    std::fputs("cannot disable private-state process dumps; no mutation attempted\n", stderr);
    return 2;
  }
  try {
    const auto result = sunshine_offline_retirement::retire_all_clients(input);
    if (std::printf("retired_clients=%zu state_sha256=%s\n", result.retired_clients, result.state_sha256.c_str()) < 0 ||
        std::fflush(stdout) != 0) {
      return 3;
    }
    return 0;
  } catch (const sunshine_offline_retirement::error &error) {
    std::fprintf(stderr, "%s\n", error.what());
    return error.mutation_attempted ? 3 : 2;
  } catch (...) {
    std::fputs("retirement result uncertain; keep all authority stopped\n", stderr);
    return 3;
  }
}
