#pragma once

#include "korri_certificate_control.h"

#include <cstddef>
#include <stdexcept>
#include <string>
#include <string_view>

// Administrative module, not a Sunshine runtime or socket API. The caller must
// exclude every Sunshine/state writer and keep authority stopped on any error.
namespace sunshine_offline_retirement {
  inline constexpr std::size_t max_state_bytes = 16 * 1024 * 1024;

  struct request {
    std::string state_path;
    std::string expected_host_uuid;
    std::string expected_state_sha256;
  };

  struct result {
    std::size_t retired_clients;
    std::string state_sha256;
  };

  class error: public std::runtime_error {
  public:
    error(const char *message, bool attempted = false):
        std::runtime_error(message),
        mutation_attempted(attempted) {}

    const bool mutation_attempted;
  };

  std::string sha256(std::string_view bytes);
  result retire_all_clients(const request &input, korri_certificate_control::fault_plan *faults = nullptr);
}  // namespace sunshine_offline_retirement
