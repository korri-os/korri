// Compile against the FULL patched Sunshine translation unit. Platform effects
// are stubbed; packet dispatch, stream context, authority reader, timer executor,
// serialization and Unix socket transport below are the release implementation.
#include "src/input.cpp"

#include <cassert>
#include <fstream>
#include <iostream>
#include <poll.h>
#include <sys/wait.h>

thread_pool_util::ThreadPool task_pool;
bool display_cursor = true;
namespace config { input_t input {}; }
namespace mail { safe::mail_t man = std::make_shared<safe::mail_raw_t>(); }
boost::log::sources::severity_logger<int> verbose, debug, info, warning, error, fatal;

namespace platf {
  static unsigned native_allocations = 0;
  void freeInput(void *) {}
  std::unique_ptr<client_input_t> allocate_client_input_context(input_t &) { return {}; }
  void move_mouse(input_t &, int, int) {}
  void abs_mouse(input_t &, const touch_port_t &, float, float) {}
  void button_mouse(input_t &, int, bool) {}
  void scroll(input_t &, int) {}
  void hscroll(input_t &, int) {}
  void keyboard_update(input_t &, uint16_t, bool, uint8_t) {}
  void gamepad_update(input_t &, int, const gamepad_state_t &) {}
  void unicode(input_t &, char *, int) {}
  void touch_update(client_input_t *, const touch_port_t &, const touch_input_t &) {}
  void pen_update(client_input_t *, const touch_port_t &, const pen_input_t &) {}
  void gamepad_touch(input_t &, const gamepad_touch_t &) {}
  void gamepad_motion(input_t &, const gamepad_motion_t &) {}
  void gamepad_battery(input_t &, const gamepad_battery_t &) {}
  int alloc_gamepad(input_t &, const gamepad_id_t &, const gamepad_arrival_t &, feedback_queue_t) {
    ++native_allocations;
    return -1;
  }
  void free_gamepad(input_t &, int) {}
}

using clock_type = std::chrono::steady_clock;
using json = nlohmann::json;
struct received_t { json envelope; clock_type::time_point time; };
static std::string directory, socket_path;
static int listener = -1;
static std::vector<received_t> received;
static std::string launch(32, 'a'), token(64, 'b');
static std::uint64_t generation = 1;

static void barrier() { task_pool.push([] {}).get(); }
static void sidecar() {
  const auto path = directory + "/sunshine-active-launch.json";
  {
    std::ofstream out(path + ".new");
    out << json {{"launchId", launch}, {"mirrorToken", token}, {"generation", generation}};
  }
  assert(::chmod((path + ".new").c_str(), 0640) == 0);
  assert(::rename((path + ".new").c_str(), path.c_str()) == 0);
}
static void listen_at_path(int backlog = 32) {
  listener = ::socket(AF_UNIX, SOCK_SEQPACKET | SOCK_NONBLOCK | SOCK_CLOEXEC, 0);
  assert(listener >= 0);
  sockaddr_un addr {};
  addr.sun_family = AF_UNIX;
  std::strncpy(addr.sun_path, socket_path.c_str(), sizeof(addr.sun_path) - 1);
  assert(::bind(listener, reinterpret_cast<sockaddr *>(&addr), sizeof(addr)) == 0);
  assert(::listen(listener, backlog) == 0);
}
static void drain() {
  if (listener < 0) { return; }
  for (;;) {
    int peer = ::accept4(listener, nullptr, nullptr, SOCK_CLOEXEC | SOCK_NONBLOCK);
    if (peer < 0) { assert(errno == EAGAIN || errno == EWOULDBLOCK); break; }
    char buffer[2049];
    pollfd readable {peer, POLLIN, 0};
    assert(::poll(&readable, 1, 250) == 1);
    auto size = ::recv(peer, buffer, sizeof(buffer), 0);
    // The producer closes every one-frame connection before its task completes.
    assert(size > 0 && size <= 2048 && buffer[size - 1] == '\n');
    auto envelope = json::parse(buffer, buffer + size);
    assert(envelope.size() == 2 && envelope.contains("mirrorToken") && envelope.contains("frame"));
    received.push_back({std::move(envelope), clock_type::now()});
    ::close(peer);
  }
}
static void collect(std::chrono::milliseconds duration) {
  auto end = clock_type::now() + duration;
  do { barrier(); drain(); std::this_thread::sleep_for(5ms); } while (clock_type::now() < end);
  barrier(); drain();
}
static void clear() { barrier(); drain(); received.clear(); }
static std::size_t count(std::string_view kind) {
  return std::count_if(received.begin(), received.end(), [&](const auto &r) { return r.envelope["frame"]["kind"] == kind; });
}
static auto stream() { return input::alloc(std::make_shared<safe::mail_raw_t>()); }
template<class Packet>
static void submit(std::shared_ptr<input::input_t> &ctx, const Packet &packet) {
  const auto *bytes = reinterpret_cast<const std::uint8_t *>(&packet);
  input::passthrough(ctx, std::vector<std::uint8_t>(bytes, bytes + sizeof(packet)));
  barrier();
}
static void state(std::shared_ptr<input::input_t> &ctx, unsigned buttons = 0, short number = 0, short mask = 1) {
  NV_MULTI_CONTROLLER_PACKET packet {};
  packet.header.magic = util::endian::little<std::uint32_t>(MULTI_CONTROLLER_MAGIC_GEN5);
  packet.controllerNumber = number;
  packet.activeGamepadMask = mask;
  packet.buttonFlags = buttons;
  packet.leftStickY = -32768;
  packet.rightTrigger = 42;
  submit(ctx, packet);
}
static void arrival(std::shared_ptr<input::input_t> &ctx, unsigned number = 0) {
  SS_CONTROLLER_ARRIVAL_PACKET packet {};
  packet.header.magic = util::endian::little<std::uint32_t>(SS_CONTROLLER_ARRIVAL_MAGIC);
  packet.controllerNumber = number;
  submit(ctx, packet);
}
static void stop_stream(std::shared_ptr<input::input_t> &ctx) { input::stop(ctx); barrier(); drain(); ctx.reset(); barrier(); }

int main() {
  assert(::geteuid() == 0); // Run in the off-device Nix VM, not fakeroot.
  char temporary[] = "/tmp/sunshine-presence-XXXXXX";
  auto path = ::mkdtemp(temporary);
  assert(path);
  directory = path;
  socket_path = directory + "/mirror.sock";
  assert(::setenv("KORRI_INPUT_SEAT_RUNTIME_DIR", directory.c_str(), 1) == 0);
  assert(::setenv("KORRI_INPUT_SEAT_MIRROR_SOCKET", socket_path.c_str(), 1) == 0);
  boost::log::core::get()->set_logging_enabled(false);
  config::input.controller = true;
  config::input.back_button_timeout = -1ms;
  sidecar();
  listen_at_path();
  task_pool.start(1); // Exactly the upstream executor topology.

  // Arrival is not a neutral baseline. First held state must remain held.
  auto ctx = stream();
  arrival(ctx);
  collect(300ms);
  assert(count("source-connected") == 1 && count("source-state") == 0);
  state(ctx, 0x1000);
  drain();
  assert(count("source-state") == 1);
  assert(received.back().envelope["frame"]["buttons"] == 0x1000);
  assert(received.back().envelope["frame"]["leftStickY"] == -32768);
  assert(received.back().envelope["frame"]["rightTrigger"] == 42);
  stop_stream(ctx);
  clear();

  // No incoming client packets for > receiver's 1250ms expiry. Actual periodic
  // current frames retain a neutral baseline; the next one-shot press is one
  // rising edge, never a new Connected baseline or a synthesized repeat edge.
  ctx = stream();
  state(ctx);
  collect(1600ms);
  assert(count("source-connected") == 1);
  assert(count("source-state") >= 8);
  auto last = received.front().time;
  for (const auto &r : received) {
    assert(r.time - last < 250ms);
    last = r.time;
    assert(r.envelope["mirrorToken"] == token);
    assert(r.envelope["frame"]["launchId"] == launch);
  }
  state(ctx, 0x1000);
  collect(450ms);
  unsigned edges = 0, previous = 0;
  for (const auto &r : received) {
    if (r.envelope["frame"]["kind"] != "source-state") { continue; }
    unsigned buttons = r.envelope["frame"]["buttons"];
    if (buttons & ~previous) { ++edges; }
    previous = buttons;
  }
  assert(edges == 1 && count("source-connected") == 1);
  stop_stream(ctx);
  clear();

  // Stop while the input executor is occupied, before a due timer can run.
  // The atomic stop gate must work before its queued retirement executes.
  ctx = stream();
  state(ctx);
  clear();
  std::promise<void> entered, release;
  auto released = release.get_future();
  auto blocked = task_pool.push([&] { entered.set_value(); released.wait(); });
  entered.get_future().wait();
  std::this_thread::sleep_for(230ms);
  // Queue a real held packet BEFORE retirement. Ordinary tasks precede timers,
  // so this packet must see the atomic gate, not merely timer cancellation.
  NV_MULTI_CONTROLLER_PACKET pending {};
  pending.header.magic = util::endian::little<std::uint32_t>(MULTI_CONTROLLER_MAGIC_GEN5);
  pending.controllerNumber = 0;
  pending.activeGamepadMask = 1;
  pending.buttonFlags = 0x1000;
  const auto *pending_bytes = reinterpret_cast<const std::uint8_t *>(&pending);
  input::passthrough(ctx, std::vector<std::uint8_t>(pending_bytes, pending_bytes + sizeof(pending)));
  input::stop(ctx);
  release.set_value();
  blocked.get();
  collect(450ms);
  assert(count("source-state") == 0 && count("source-disconnected") == 1);
  state(ctx, 0x1000); // Already queued/late packets cannot revive a dead stream.
  collect(220ms);
  assert(count("source-state") == 0);
  ctx.reset();
  clear();

  // Destruction must not be prevented by a recurring timer's capture.
  ctx = stream();
  state(ctx);
  std::weak_ptr<input::input_t> weak = ctx;
  clear();
  ctx.reset();
  collect(450ms);
  assert(weak.expired());
  assert(count("source-disconnected") == 1 && count("source-state") == 0);
  clear();

  // Capture at allocation, not at first arrival/packet/timer. Replace each
  // authority component independently. No old stream may adopt the new one.
  for (unsigned field = 0; field < 3; ++field) {
    ctx = stream();
    state(ctx, 0x1000);
    auto not_yet_used = stream();
    const auto old_token = token, old_launch = launch;
    clear();
    if (field == 0) { ++generation; }
    if (field == 1) { token.assign(64, 'c'); }
    if (field == 2) { launch.assign(32, 'd'); }
    sidecar();
    collect(250ms);
    state(ctx, 0x1000);
    state(not_yet_used, 0x1000);
    collect(250ms);
    assert(count("source-state") == 0);
    for (const auto &r : received) {
      assert(r.envelope["mirrorToken"] == old_token);
      assert(r.envelope["frame"]["launchId"] == old_launch);
      assert(r.envelope["frame"]["kind"] == "source-disconnected");
    }
    stop_stream(ctx);
    stop_stream(not_yet_used);
    clear();
    auto fresh = stream();
    state(fresh);
    drain();
    assert(count("source-state") == 1);
    assert(received.back().envelope["mirrorToken"] == token);
    stop_stream(fresh);
    clear();
  }

  // Missing authority at allocation cannot be adopted by a later first packet.
  assert(::unlink((directory + "/sunshine-active-launch.json").c_str()) == 0);
  ctx = stream();
  sidecar();
  state(ctx, 0x1000);
  collect(250ms);
  assert(received.empty());
  stop_stream(ctx);
  clear();

  // Disappearance is terminal even if exactly the same authority returns.
  ctx = stream();
  state(ctx);
  clear();
  assert(::unlink((directory + "/sunshine-active-launch.json").c_str()) == 0);
  collect(250ms);
  sidecar();
  state(ctx, 0x1000);
  collect(250ms);
  assert(count("source-state") == 0);
  stop_stream(ctx);
  clear();

  // Mask removal covers other controller indices too. No stale refresh.
  ctx = stream();
  state(ctx, 0, 0, 3);
  state(ctx, 0, 1, 3);
  clear();
  state(ctx, 0, 1, 2);
  collect(450ms);
  assert(count("source-disconnected") == 1);
  for (const auto &r : received) {
    if (r.envelope["frame"]["kind"] == "source-state") { assert(r.envelope["frame"]["controllerNumber"] == 1); }
  }
  stop_stream(ctx);
  clear();

  // Existing launch+controllerNumber identity cannot merge two live streams.
  ctx = stream();
  auto collision = stream();
  state(ctx);
  clear();
  state(collision, 0x1000);
  state(ctx, 0x1000);
  collect(450ms);
  assert(count("source-state") == 0 && count("source-disconnected") == 1);
  stop_stream(ctx);
  stop_stream(collision);
  clear();

  // No native fallback and no wait if the real receiver is absent/full.
  ::close(listener); listener = -1;
  assert(::unlink(socket_path.c_str()) == 0);
  ctx = stream();
  auto start = clock_type::now();
  state(ctx, 0x1000);
  assert(clock_type::now() - start < 250ms);
  listen_at_path(1);
  for (unsigned n = 0; n < 100; ++n) { state(ctx, n & 1 ? 0x1000 : 0); }
  assert(clock_type::now() - start < 1s);
  drain(); clear();
  collect(450ms);
  assert(count("source-state") >= 2); // A still-live source recovers transport.
  stop_stream(ctx);
  assert(platf::native_allocations == 0);
  clear();

  // Kernel SO_PEERCRED, not mocked stat ownership, rejects a local impostor.
  ::close(listener); listener = -1;
  assert(::unlink(socket_path.c_str()) == 0);
  assert(::chmod(directory.c_str(), 0777) == 0);
  int ready[2], done[2];
  assert(::pipe(ready) == 0 && ::pipe(done) == 0);
  sockaddr_un address {};
  address.sun_family = AF_UNIX;
  std::strncpy(address.sun_path, socket_path.c_str(), sizeof(address.sun_path) - 1);
  auto child = ::fork();
  assert(child >= 0);
  if (child == 0) {
    // Only syscalls after fork from a multithreaded process.
    ::close(ready[0]); ::close(done[1]);
    if (::setgid(65534) || ::setuid(65534)) { ::_exit(10); }
    int fd = ::socket(AF_UNIX, SOCK_SEQPACKET | SOCK_CLOEXEC, 0);
    if (fd < 0 || ::bind(fd, reinterpret_cast<sockaddr *>(&address), sizeof(address)) || ::listen(fd, 8)) { ::_exit(11); }
    char ch = 'r';
    if (::write(ready[1], &ch, 1) != 1) { ::_exit(12); }
    int peer = ::accept(fd, nullptr, nullptr);
    char bytes[2048];
    auto size = ::recv(peer, bytes, sizeof(bytes), 0);
    ch = size == 0 ? 'y' : 'n';
    ::write(ready[1], &ch, 1);
    ::read(done[0], &ch, 1);
    ::_exit(size == 0 ? 0 : 13);
  }
  ::close(ready[1]); ::close(done[0]);
  char ch;
  assert(::read(ready[0], &ch, 1) == 1 && ch == 'r');
  ctx = stream();
  state(ctx, 0x1000);
  assert(::read(ready[0], &ch, 1) == 1 && ch == 'y');
  stop_stream(ctx);
  assert(::write(done[1], "x", 1) == 1);
  int status;
  assert(::waitpid(child, &status, 0) == child && WIFEXITED(status) && WEXITSTATUS(status) == 0);
  ::close(ready[0]); ::close(done[1]);
  assert(platf::native_allocations == 0);

  barrier();
  task_pool.stop(); task_pool.join();
  std::filesystem::remove_all(directory);
  std::cout << "actual patched Sunshine input: presence/lifetime/authority/socket regressions passed\n";
}
