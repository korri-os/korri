#include "offline-retirement.h"

#include "crypto.h"

#include <algorithm>
#include <array>
#include <boost/property_tree/json_parser.hpp>
#include <cerrno>
#include <climits>
#include <dirent.h>
#include <fcntl.h>
#include <memory>
#include <openssl/sha.h>
#include <set>
#include <sstream>
#include <sys/stat.h>
#include <sys/xattr.h>
#include <unistd.h>

namespace sunshine_offline_retirement {
  namespace kc = korri_certificate_control;

  namespace {
    class descriptor {
    public:
      explicit descriptor(int fd):
          fd_(fd) {
        if (fd < 0) {
          throw error("cannot open safe existing state path");
        }
      }

      ~descriptor() {
        ::close(fd_);
      }

      descriptor(const descriptor &) = delete;
      descriptor &operator=(const descriptor &) = delete;

      int get() const {
        return fd_;
      }

    private:
      int fd_;
    };

    struct stat metadata(int fd) {
      struct stat value {};
      if (::fstat(fd, &value) != 0) {
        throw error("cannot inspect state metadata");
      }
      return value;
    }

    bool same_inode(const struct stat &left, const struct stat &right) {
      return left.st_dev == right.st_dev && left.st_ino == right.st_ino;
    }

    bool same_file(const struct stat &left, const struct stat &right) {
      return same_inode(left, right) && left.st_mode == right.st_mode &&
             left.st_uid == right.st_uid && left.st_gid == right.st_gid &&
             left.st_nlink == right.st_nlink && left.st_size == right.st_size &&
             left.st_mtim.tv_sec == right.st_mtim.tv_sec && left.st_mtim.tv_nsec == right.st_mtim.tv_nsec &&
             left.st_ctim.tv_sec == right.st_ctim.tv_sec && left.st_ctim.tv_nsec == right.st_ctim.tv_nsec;
    }

    void no_extended_metadata(int fd) {
      // Atomic replacement cannot preserve arbitrary ACLs/xattrs. Refuse them,
      // including an unreadable metadata inventory, instead of silently dropping them.
      if (::flistxattr(fd, nullptr, 0) != 0) {
        throw error("extended metadata is unsupported");
      }
    }

    class state_directory {
    public:
      explicit state_directory(const std::string &path) {
        if (path.empty() || path.front() != '/' || path.size() >= PATH_MAX ||
            path.find('\0') != std::string::npos) {
          throw error("state path must be absolute and bounded");
        }
        fd_ = std::make_unique<descriptor>(::open("/", O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW));
        std::size_t start = 1;
        for (;;) {
          const auto end = path.find('/', start);
          const auto part = path.substr(start, end == std::string::npos ? end : end - start);
          if (part.empty() || part == "." || part == ".." || part.size() > NAME_MAX) {
            throw error("state path contains an unsafe component");
          }
          if (end == std::string::npos) {
            filename = part;
            break;
          }
          auto next = std::make_unique<descriptor>(::openat(
            fd_->get(),
            part.c_str(),
            O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW
          ));
          const auto info = metadata(next->get());
          const bool trusted_sticky = info.st_uid == 0 && (info.st_mode & S_ISVTX);
          if ((info.st_uid != 0 && info.st_uid != ::getuid()) ||
              ((info.st_mode & 0022) && !trusted_sticky) || (info.st_mode & (S_ISUID | S_ISGID))) {
            throw error("unsafe state ancestor ownership or permissions");
          }
          no_extended_metadata(next->get());
          fd_ = std::move(next);
          start = end + 1;
        }
        info = metadata(fd_->get());
        if (info.st_uid != ::getuid() || info.st_gid != ::getgid() || (info.st_mode & 07777) != 0700) {
          throw error("state directory must belong to the caller with mode 0700");
        }
        no_extended_metadata(fd_->get());
      }

      void reject_residue() const {
        // Separate open file description: enumeration must not advance the pinned
        // directory descriptor's offset across repeated checks.
        descriptor scan_fd(::openat(fd_->get(), ".", O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW));
        DIR *scan = ::fdopendir(::dup(scan_fd.get()));
        if (!scan) {
          throw error("cannot inspect interrupted transaction residue");
        }
        const auto close = [](DIR *value) {
          ::closedir(value);
        };
        std::unique_ptr<DIR, decltype(close)> entries(scan, close);
        std::size_t count = 0;
        for (;;) {
          errno = 0;
          const auto entry = ::readdir(entries.get());
          if (!entry) {
            if (errno != 0) {
              throw error("cannot inspect interrupted transaction residue");
            }
            break;
          }
          if (++count > 4096) {
            throw error("state directory entry bound exceeded");
          }
          if (std::string_view(entry->d_name).starts_with(filename + ".korri-")) {
            throw error("interrupted transaction residue requires offline inspection");
          }
        }
      }

      int fd() const {
        return fd_->get();
      }

      std::string filename;
      struct stat info {};

    private:
      std::unique_ptr<descriptor> fd_;
    };

    struct snapshot {
      std::string bytes;
      struct stat info;
    };

    snapshot read_state(const state_directory &directory) {
      descriptor fd(::openat(directory.fd(), directory.filename.c_str(), O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK));
      const auto info = metadata(fd.get());
      if (!S_ISREG(info.st_mode) || info.st_nlink != 1 || (info.st_mode & 07777) != 0600 ||
          info.st_uid != ::getuid() || info.st_gid != ::getgid() ||
          info.st_size <= 0 || info.st_size > static_cast<off_t>(max_state_bytes)) {
        throw error("state must be a bounded single-link caller-owned 0600 regular file");
      }
      no_extended_metadata(fd.get());
      std::string bytes;
      std::array<char, 8192> buffer {};
      for (;;) {
        const auto received = ::read(fd.get(), buffer.data(), buffer.size());
        if (received < 0 && errno == EINTR) {
          continue;
        }
        if (received < 0) {
          throw error("cannot read existing state");
        }
        if (received == 0) {
          break;
        }
        bytes.append(buffer.data(), static_cast<std::size_t>(received));
        if (bytes.size() > max_state_bytes) {
          throw error("state grew beyond its size bound");
        }
      }
      if (bytes.size() != static_cast<std::size_t>(info.st_size) || !same_file(info, metadata(fd.get()))) {
        throw error("state changed while reading");
      }
      return {std::move(bytes), info};
    }

    nlohmann::json checked_document(const std::string &bytes) {
      std::vector<std::set<std::string>> keys;
      auto callback = [&](int depth, nlohmann::json::parse_event_t event, nlohmann::json &value) {
        if (depth > 64) {
          throw error("state nesting bound exceeded");
        }
        if (event == nlohmann::json::parse_event_t::object_start) {
          keys.emplace_back();
        }
        if (event == nlohmann::json::parse_event_t::object_end) {
          keys.pop_back();
        }
        if (event == nlohmann::json::parse_event_t::key &&
            !keys.back().insert(value.get<std::string>()).second) {
          throw error("duplicate state keys are ambiguous");
        }
        // The producer serializer uses binary64. Reject fractional and overflow
        // numbers rather than silently changing unknown unrelated numeric content.
        if (event == nlohmann::json::parse_event_t::value && value.is_number_float()) {
          throw error("state contains numeric content not proven lossless by the producer");
        }
        return true;
      };
      auto document = nlohmann::json::parse(bytes, callback);
      if (!document.is_object() || !document.contains("root") || !document["root"].is_object() ||
          !document["root"].contains("uniqueid") || !document["root"]["uniqueid"].is_string()) {
        throw error("existing Sunshine host identity is missing or malformed");
      }
      const auto collection = [](const nlohmann::json &value) {
        // Boost.PropertyTree's historical writer represents an empty array as "".
        if (!value.is_array() && value != "") {
          throw error("invalid Sunshine client collection");
        }
      };
      const auto &root = document["root"];
      if (root.contains("named_devices")) {
        collection(root["named_devices"]);
        if (root["named_devices"].is_array()) {
          for (const auto &entry : root["named_devices"]) {
            if (!entry.is_object() || !entry.contains("name") || !entry["name"].is_string() ||
                !entry.contains("uuid") || !entry["uuid"].is_string() ||
                !entry.contains("cert") || !entry["cert"].is_string()) {
              throw error("invalid Sunshine named client record");
            }
          }
        }
      }
      if (root.contains("devices")) {
        collection(root["devices"]);
        if (root["devices"].is_array()) {
          for (const auto &entry : root["devices"]) {
            if (!entry.is_object()) {
              throw error("invalid Sunshine legacy client record");
            }
            if (entry.contains("certs")) {
              collection(entry["certs"]);
              if (entry["certs"].is_array()) {
                for (const auto &cert : entry["certs"]) {
                  if (!cert.is_string()) {
                    throw error("invalid Sunshine legacy certificate");
                  }
                }
              }
            }
          }
        }
      }
      return document;
    }

    std::vector<kc::named_certificate> clients(const std::string &bytes) {
      std::istringstream stream(bytes);
      boost::property_tree::ptree tree;
      boost::property_tree::read_json(stream, tree);
      // Imported UUIDs never leave memory: every imported client is retired.
      return kc::parse_client_state(tree.get_child("root"), [] {
        return std::string {};
      });
    }

    class working_directory {
    public:
      explicit working_directory(int fd):
          previous_(::open(".", O_RDONLY | O_DIRECTORY | O_CLOEXEC)) {
        if (::fchdir(fd) != 0) {
          throw error("cannot pin transaction directory");
        }
      }

      ~working_directory() {
        if (::fchdir(previous_.get()) != 0) {
          ::_exit(3);
        }
      }

    private:
      descriptor previous_;
    };
  }  // namespace

  std::string sha256(std::string_view bytes) {
    std::array<unsigned char, SHA256_DIGEST_LENGTH> digest {};
    if (!SHA256(reinterpret_cast<const unsigned char *>(bytes.data()), bytes.size(), digest.data())) {
      throw error("state digest unavailable");
    }
    constexpr char hex[] = "0123456789abcdef";
    std::string result;
    for (const auto byte : digest) {
      result += hex[byte >> 4];
      result += hex[byte & 15];
    }
    return result;
  }

  result retire_all_clients(const request &input, kc::fault_plan *faults) {
    bool attempted = false;
    try {
      if (::getuid() != ::geteuid() || ::getgid() != ::getegid()) {
        throw error("run directly as the existing state owner, not set-id");
      }
      if (input.expected_host_uuid.empty() || input.expected_host_uuid.size() > 128 ||
          input.expected_state_sha256.size() != 64 ||
          !std::all_of(input.expected_state_sha256.begin(), input.expected_state_sha256.end(), [](char ch) {
            return (ch >= '0' && ch <= '9') || (ch >= 'a' && ch <= 'f');
          })) {
        throw error("exact public host and lowercase SHA-256 state evidence are required");
      }
      state_directory directory(input.state_path);
      directory.reject_residue();
      const auto before = read_state(directory);
      if (sha256(before.bytes) != input.expected_state_sha256) {
        throw error("expected state digest mismatch");
      }
      auto document = checked_document(before.bytes);
      if (document["root"]["uniqueid"] != input.expected_host_uuid) {
        throw error("expected host UUID mismatch");
      }
      const auto old_clients = clients(before.bytes);
      for (const auto &client : old_clients) {
        (void) kc::validate_single_certificate(client.certificate);
      }
      const auto candidate = kc::build_state_document(before.bytes, input.expected_host_uuid, {});
      if (candidate.size() > max_state_bytes) {
        throw error("replacement exceeds state size bound");
      }
      document["root"].erase("devices");
      document["root"]["named_devices"] = nlohmann::json::array();
      if (checked_document(candidate) != document || !clients(candidate).empty()) {
        throw error("producer replacement did not preserve unrelated state");
      }
      crypto::cert_chain_t empty_chain;
      for (const auto &client : old_clients) {
        auto certificate = crypto::x509(client.certificate);
        if (!certificate || empty_chain.verify(certificate.get()) == nullptr) {
          throw error("producer trust verifier did not reject retired clients");
        }
      }
      // The producer transaction is pathname-based. Pin a fully checked private
      // parent as cwd and give it only a basename. External exclusive quiescence
      // remains mandatory: no file API can exclude a concurrent same-UID writer.
      state_directory current_path(input.state_path);
      if (!same_inode(directory.info, current_path.info)) {
        throw error("state directory changed");
      }
      directory.reject_residue();
      const auto current = read_state(directory);
      if (!same_file(before.info, current.info) || current.bytes != before.bytes) {
        throw error("state changed before commit");
      }
      working_directory pinned(directory.fd());
      attempted = true;
      if (!kc::persist_offline_retirement(directory.filename, candidate, faults)) {
        throw error("producer transaction failed; keep all authority stopped", true);
      }
      const auto after = read_state(directory);
      state_directory reopened_path(input.state_path);
      if (!same_inode(directory.info, reopened_path.info) || after.bytes != candidate ||
          !clients(after.bytes).empty()) {
        throw error("post-commit verification failed; keep all authority stopped", true);
      }
      return {old_clients.size(), sha256(after.bytes)};
    } catch (const error &failure) {
      throw error(failure.what(), attempted || failure.mutation_attempted);
    } catch (...) {
      // Parser and crypto diagnostics may contain private bytes. Never expose them.
      throw error(attempted ? "retirement result uncertain; keep all authority stopped" : "invalid existing Sunshine state; no mutation attempted", attempted);
    }
  }
}  // namespace sunshine_offline_retirement
