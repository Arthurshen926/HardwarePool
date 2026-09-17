#include "capyio_display_engine_bootstrap.h"

#include <algorithm>
#include <cstring>
#include <utility>

#ifdef CAPYIO_VIDEO_ONLY
#include "src/globals.h"
#include "src/platform/common.h"
#include "src/rtsp.h"
#include "src/video.h"

#include <chrono>
#include <condition_variable>
#include <future>
#include <memory>
#include <mutex>
#include <new>
#include <thread>
#endif

namespace capyio::display_engine {
namespace {
constexpr std::uint32_t minimum_timeout_millis = 100;
constexpr std::uint32_t maximum_timeout_millis = 10000;
constexpr std::uint32_t maximum_width = 8192;
constexpr std::uint32_t maximum_height = 8192;
constexpr std::uint32_t minimum_refresh_millihertz = 1000;
constexpr std::uint32_t maximum_refresh_millihertz = 500000;
constexpr std::uint32_t minimum_bitrate_kbps = 500;
constexpr std::uint32_t maximum_bitrate_kbps = 200000;

bool valid_timeout(const std::uint32_t timeout_millis) noexcept {
  return timeout_millis >= minimum_timeout_millis &&
         timeout_millis <= maximum_timeout_millis;
}

template<typename Value>
void secure_zero(Value &value) noexcept {
  volatile std::uint8_t *bytes =
      reinterpret_cast<volatile std::uint8_t *>(&value);
  for (std::size_t index = 0; index < sizeof(value); ++index) {
    bytes[index] = 0;
  }
}

template<std::size_t Size>
bool any_nonzero(const std::array<std::uint8_t, Size> &value) noexcept {
  return std::any_of(value.begin(), value.end(), [](const std::uint8_t byte) {
    return byte != 0;
  });
}

bool any_nonzero(const std::uint8_t (&value)[16]) noexcept {
  return std::any_of(std::begin(value), std::end(value),
                     [](const std::uint8_t byte) { return byte != 0; });
}

bool valid_capture_output(const std::uint8_t *value) noexcept {
  constexpr char prefix[] = "\\\\.\\DISPLAY";
  constexpr std::size_t prefix_length = sizeof(prefix) - 1;
  for (std::size_t index = 0; index < prefix_length; ++index) {
    if (value[index] != static_cast<std::uint8_t>(prefix[index])) {
      return false;
    }
  }
  std::size_t end = prefix_length;
  while (end < CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY && value[end] != 0) {
    if (value[end] < '0' || value[end] > '9' ||
        (end == prefix_length && value[end] == '0')) {
      return false;
    }
    ++end;
  }
  if (end == prefix_length ||
      end == CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY) {
    return false;
  }
  return std::all_of(
      value + end + 1,
      value + CAPYIO_ENGINE_CAPTURE_OUTPUT_UTF8_CAPACITY,
      [](const std::uint8_t byte) { return byte == 0; });
}

bool valid_port_namespace(const CapyIoDisplayEngineStartV2 &start) noexcept {
  if (start.rtsp_port < 21) {
    return false;
  }
  const auto base = static_cast<std::uint32_t>(start.rtsp_port) - 21;
  return base >= 1024 && base != 47989 && base >= 5 &&
         base + 10 <= UINT16_MAX && start.https_port == base - 5 &&
         start.video_port == base + 9 && start.control_port == base + 10;
}

bool valid_start(const CapyIoDisplayEngineStartV2 &start) noexcept {
  return start.struct_size == sizeof(start) &&
         start.abi_version == CAPYIO_DISPLAY_ENGINE_ABI_V2 &&
         any_nonzero(start.adapter_instance_id) &&
         any_nonzero(start.display_capability_id) &&
         valid_capture_output(start.capture_output_utf8) && start.width > 0 &&
         start.width <= maximum_width && start.height > 0 &&
         start.height <= maximum_height &&
         start.refresh_millihertz >= minimum_refresh_millihertz &&
         start.refresh_millihertz <= maximum_refresh_millihertz &&
         start.bitrate_kbps >= minimum_bitrate_kbps &&
         start.bitrate_kbps <= maximum_bitrate_kbps && start.codec >= 1 &&
         start.codec <= 3 && start.dynamic_range >= 1 &&
         start.dynamic_range <= 2 &&
         !(start.codec == 1 && start.dynamic_range == 2) &&
         valid_port_namespace(start);
}
}  // namespace

bool authorized_stream_selection_matches(
    const AuthorizedStreamSelection &authorized,
    const AnnouncedStreamSelection &announced) noexcept {
  return announced.width == authorized.width &&
         announced.height == authorized.height &&
         announced.refresh_millihertz == authorized.refresh_millihertz &&
         announced.codec == authorized.codec &&
         announced.dynamic_range == authorized.dynamic_range &&
         announced.bitrate_kbps == authorized.bitrate_kbps;
}

LaunchSessionResult build_launch_session_plan(
    const CapyIoDisplayEngineStartV2 &start,
    const LaunchSessionMaterial &material,
    LaunchSessionPlan &output) noexcept {
  // Never leave a prior session key visible through a reused output object on
  // validation failure.
  output = LaunchSessionPlan {};
  if (!valid_start(start)) {
    return LaunchSessionResult::invalid_start;
  }
  if (material.launch_session_id == 0 || !any_nonzero(material.key)) {
    return LaunchSessionResult::invalid_material;
  }

  LaunchSessionPlan plan {};
  plan.launch_session_id = material.launch_session_id;
  plan.key = material.key;
  plan.iv[0] = static_cast<std::uint8_t>(material.key_id >> 24);
  plan.iv[1] = static_cast<std::uint8_t>(material.key_id >> 16);
  plan.iv[2] = static_cast<std::uint8_t>(material.key_id >> 8);
  plan.iv[3] = static_cast<std::uint8_t>(material.key_id);
  plan.ping_payload = material.ping_payload;
  plan.control_connect_data = material.control_connect_data;
  std::copy(std::begin(start.display_capability_id),
            std::end(start.display_capability_id),
            plan.display_sink_identity.begin());
  std::copy(std::begin(start.capture_output_utf8),
            std::end(start.capture_output_utf8),
            plan.capture_output_utf8.begin());
  plan.selection = AuthorizedStreamSelection {
      start.width,
      start.height,
      start.refresh_millihertz,
      start.codec,
      start.dynamic_range,
      start.bitrate_kbps,
  };
  plan.encrypted_rtsp = true;
  plan.audio_enabled = false;
  plan.input_enabled = false;
  plan.upstream_virtual_display_enabled = false;
  plan.display_config_preapplied = true;
  output = std::move(plan);
  return LaunchSessionResult::ok;
}

Bootstrap::Bootstrap(BootstrapHooks hooks) noexcept : hooks_ {hooks} {}

BootstrapState Bootstrap::state() const noexcept {
  return state_;
}

bool Bootstrap::hooks_valid() const noexcept {
  return hooks_.start_mail != nullptr && hooks_.stop_mail != nullptr &&
         hooks_.start_platform != nullptr && hooks_.stop_platform != nullptr &&
         hooks_.probe_encoder != nullptr && hooks_.start_listener != nullptr &&
         hooks_.stop_listener != nullptr;
}

void Bootstrap::release_base_resources() noexcept {
  if (platform_owned_) {
    hooks_.stop_platform(hooks_.context);
    platform_owned_ = false;
  }
  if (mail_owned_) {
    hooks_.stop_mail(hooks_.context);
    mail_owned_ = false;
  }
}

BootstrapResult Bootstrap::start(const std::uint32_t timeout_millis) noexcept {
  if (!hooks_valid() || !valid_timeout(timeout_millis)) {
    return BootstrapResult::invalid_argument;
  }
  if (state_ != BootstrapState::idle || mail_owned_ || platform_owned_) {
    return BootstrapResult::invalid_state;
  }
  if (!hooks_.start_mail(hooks_.context)) {
    return BootstrapResult::mail_start_failed;
  }
  mail_owned_ = true;
  if (!hooks_.start_platform(hooks_.context)) {
    release_base_resources();
    return BootstrapResult::platform_start_failed;
  }
  platform_owned_ = true;
  if (!hooks_.probe_encoder(hooks_.context)) {
    release_base_resources();
    return BootstrapResult::encoder_probe_failed;
  }

  switch (hooks_.start_listener(hooks_.context, timeout_millis)) {
    case ListenerStartResult::ready:
      state_ = BootstrapState::running;
      return BootstrapResult::ok;
    case ListenerStartResult::bind_failed:
      release_base_resources();
      return BootstrapResult::listener_bind_failed;
    case ListenerStartResult::timeout_quiescent:
      release_base_resources();
      return BootstrapResult::timeout;
    case ListenerStartResult::timeout_owned:
      state_ = BootstrapState::cleanup_required;
      return BootstrapResult::timeout;
  }
  release_base_resources();
  return BootstrapResult::listener_bind_failed;
}

BootstrapResult Bootstrap::stop(const std::uint32_t timeout_millis) noexcept {
  if (!hooks_valid() || !valid_timeout(timeout_millis)) {
    return BootstrapResult::invalid_argument;
  }
  if ((state_ != BootstrapState::running &&
       state_ != BootstrapState::cleanup_required) ||
      !mail_owned_ || !platform_owned_) {
    return BootstrapResult::invalid_state;
  }
  if (!hooks_.stop_listener(hooks_.context, timeout_millis)) {
    return BootstrapResult::timeout;
  }
  release_base_resources();
  state_ = BootstrapState::idle;
  return BootstrapResult::ok;
}

}  // namespace capyio::display_engine

#ifdef CAPYIO_VIDEO_ONLY
namespace capyio::display_engine {

struct UpstreamRuntime {
  std::unique_ptr<platf::deinit_t> platform;
  std::shared_ptr<rtsp_stream::launch_session_t> staged_launch_session;
  std::uint32_t raised_launch_session_id {0};
  std::thread listener_thread;
  std::mutex listener_gate;
  std::condition_variable listener_changed;
  bool readiness_reported {false};
  bool listener_ready {false};
  bool listener_done {false};
};

namespace {
std::string hex_bytes(const std::uint8_t *begin, const std::size_t size) {
  constexpr char digits[] = "0123456789abcdef";
  std::string result;
  result.resize(size * 2);
  for (std::size_t index = 0; index < size; ++index) {
    result[index * 2] = digits[begin[index] >> 4];
    result[index * 2 + 1] = digits[begin[index] & 0x0f];
  }
  return result;
}

std::string uuid_string(const std::array<std::uint8_t, 16> &value) {
  constexpr char digits[] = "0123456789abcdef";
  std::string result;
  result.reserve(36);
  for (std::size_t index = 0; index < value.size(); ++index) {
    if (index == 4 || index == 6 || index == 8 || index == 10) {
      result.push_back('-');
    }
    result.push_back(digits[value[index] >> 4]);
    result.push_back(digits[value[index] & 0x0f]);
  }
  return result;
}

void report_listener_ready(
    const bool ready, void *context) noexcept {
  auto &runtime = *static_cast<UpstreamRuntime *>(context);
  {
    std::lock_guard lock {runtime.listener_gate};
    if (!runtime.readiness_reported) {
      runtime.readiness_reported = true;
      runtime.listener_ready = ready;
    }
  }
  runtime.listener_changed.notify_all();
}

void clear_raised_launch_session(UpstreamRuntime &runtime) noexcept {
  if (runtime.raised_launch_session_id != 0) {
    rtsp_stream::launch_session_clear(runtime.raised_launch_session_id);
    runtime.raised_launch_session_id = 0;
  }
  discard_staged_upstream_launch_session(&runtime);
}

bool start_upstream_mail(void *) noexcept {
  if (mail::man) {
    return false;
  }
  try {
    mail::man = std::make_shared<safe::mail_raw_t>();
  } catch (...) {
    return false;
  }
  return static_cast<bool>(mail::man);
}

void stop_upstream_mail(void *) noexcept {
  mail::man.reset();
}

bool start_upstream_platform(void *context) noexcept {
  auto &runtime = *static_cast<UpstreamRuntime *>(context);
  if (runtime.platform) {
    return false;
  }
  try {
    runtime.platform = platf::init_display_only();
  } catch (...) {
    runtime.platform.reset();
  }
  return static_cast<bool>(runtime.platform);
}

void stop_upstream_platform(void *context) noexcept {
  static_cast<UpstreamRuntime *>(context)->platform.reset();
}

bool probe_upstream_encoder(void *) noexcept {
  try {
    return video::probe_encoders() == 0;
  } catch (...) {
    return false;
  }
}

ListenerStartResult start_upstream_listener(
    void *context, const std::uint32_t timeout_millis) noexcept {
  auto &runtime = *static_cast<UpstreamRuntime *>(context);
  if (runtime.listener_thread.joinable() || !mail::man ||
      !runtime.staged_launch_session || runtime.raised_launch_session_id != 0) {
    return ListenerStartResult::bind_failed;
  }
  {
    std::lock_guard lock {runtime.listener_gate};
    runtime.readiness_reported = false;
    runtime.listener_ready = false;
    runtime.listener_done = false;
  }
  runtime.raised_launch_session_id = runtime.staged_launch_session->id;
  try {
    rtsp_stream::launch_session_raise(runtime.staged_launch_session);
  } catch (...) {
    runtime.raised_launch_session_id = 0;
    return ListenerStartResult::bind_failed;
  }
  try {
    runtime.listener_thread = std::thread {[&runtime] {
      try {
        rtsp_stream::start_capyio(report_listener_ready, &runtime);
      } catch (...) {
        report_listener_ready(false, &runtime);
      }
      {
        std::lock_guard lock {runtime.listener_gate};
        runtime.listener_done = true;
      }
      runtime.listener_changed.notify_all();
    }};
  } catch (...) {
    clear_raised_launch_session(runtime);
    return ListenerStartResult::bind_failed;
  }

  const auto deadline = std::chrono::steady_clock::now() +
                        std::chrono::milliseconds {timeout_millis};
  std::unique_lock lock {runtime.listener_gate};
  if (!runtime.listener_changed.wait_until(lock, deadline, [&runtime] {
        return runtime.readiness_reported || runtime.listener_done;
      })) {
    lock.unlock();
    mail::man->event<bool>(mail::shutdown)->raise(true);
    return ListenerStartResult::timeout_owned;
  }
  const bool ready = runtime.readiness_reported && runtime.listener_ready;
  if (ready) {
    return ListenerStartResult::ready;
  }
  if (!runtime.listener_done &&
      !runtime.listener_changed.wait_until(lock, deadline, [&runtime] {
        return runtime.listener_done;
      })) {
    lock.unlock();
    mail::man->event<bool>(mail::shutdown)->raise(true);
    return ListenerStartResult::timeout_owned;
  }
  lock.unlock();
  runtime.listener_thread.join();
  clear_raised_launch_session(runtime);
  return ListenerStartResult::bind_failed;
}

bool stop_upstream_listener(
    void *context, const std::uint32_t timeout_millis) noexcept {
  auto &runtime = *static_cast<UpstreamRuntime *>(context);
  if (!runtime.listener_thread.joinable() || !mail::man) {
    return false;
  }
  mail::man->event<bool>(mail::shutdown)->raise(true);
  std::unique_lock lock {runtime.listener_gate};
  if (!runtime.listener_changed.wait_for(
          lock, std::chrono::milliseconds {timeout_millis}, [&runtime] {
            return runtime.listener_done;
          })) {
    return false;
  }
  lock.unlock();
  runtime.listener_thread.join();
  clear_raised_launch_session(runtime);
  return true;
}
}  // namespace

UpstreamRuntime *create_upstream_runtime() noexcept {
  return new (std::nothrow) UpstreamRuntime {};
}

bool destroy_upstream_runtime(UpstreamRuntime *runtime) noexcept {
  if (runtime == nullptr) {
    return true;
  }
  if (runtime->listener_thread.joinable() || runtime->platform ||
      runtime->staged_launch_session || runtime->raised_launch_session_id != 0) {
    return false;
  }
  delete runtime;
  return true;
}

BootstrapHooks upstream_bootstrap_hooks(UpstreamRuntime *runtime) noexcept {
  return BootstrapHooks {
      runtime,
      start_upstream_mail,
      stop_upstream_mail,
      start_upstream_platform,
      stop_upstream_platform,
      probe_upstream_encoder,
      start_upstream_listener,
      stop_upstream_listener,
  };
}

LaunchSessionResult stage_upstream_launch_session(
    UpstreamRuntime *runtime,
    const CapyIoDisplayEngineStartV2 &start,
    const LaunchSessionMaterial &material) noexcept {
  if (runtime == nullptr) {
    return LaunchSessionResult::invalid_state;
  }
  if (runtime->staged_launch_session) {
    return LaunchSessionResult::invalid_state;
  }

  LaunchSessionPlan plan {};
  struct PlanSecretWiper final {
    LaunchSessionPlan &plan;
    ~PlanSecretWiper() {
      secure_zero(plan.key);
      secure_zero(plan.iv);
    }
  } plan_secret_wiper {plan};
  const auto result = build_launch_session_plan(start, material, plan);
  if (result != LaunchSessionResult::ok) {
    return result;
  }

  try {
    auto session = std::make_shared<rtsp_stream::launch_session_t>();
    session->id = plan.launch_session_id;
    session->gcm_key.assign(plan.key.begin(), plan.key.end());
    session->iv.assign(plan.iv.begin(), plan.iv.end());
    session->av_ping_payload =
        hex_bytes(plan.ping_payload.data(), plan.ping_payload.size());
    session->control_connect_data = plan.control_connect_data;
    session->device_name = "CapyIO Display Sink";
    // The private GameStream session is scoped to the authorized Display Sink
    // Capability, not to a process-global Sunshine client identity.
    session->unique_id = uuid_string(plan.display_sink_identity);
    session->client_uuid = session->unique_id;
    session->client_name = session->device_name;
    session->perm = crypto::PERM::view;
    session->appid = 0;
    session->input_only = false;
    session->host_audio = false;
    session->width = static_cast<int>(plan.selection.width);
    session->height = static_cast<int>(plan.selection.height);
    session->fps = static_cast<int>(plan.selection.refresh_millihertz);
    session->gcmap = 0;
    session->surround_info = 0;
    session->continuous_audio = false;
    session->enable_hdr = plan.selection.dynamic_range == 2;
    session->prefer_sdr_10bit = false;
    session->force_sdr = plan.selection.dynamic_range == 1;
    session->enable_sops = false;
    session->client_vrr_requested = false;
    session->client_display_mode_override = true;
    session->client_display_refresh_millihz =
        plan.selection.refresh_millihertz;
    session->client_requests_virtual_display = false;
    session->client_virtual_display_override = false;
    session->virtual_display = false;
    session->scale_factor = 100;
    session->virtual_display_failed = false;
    session->virtual_display_request_resolved = true;
    session->virtual_display_mode_override =
        config::video_t::virtual_display_mode_e::disabled;
    session->output_name_override = reinterpret_cast<const char *>(
        plan.capture_output_utf8.data());
    session->display_config_preapplied = plan.display_config_preapplied;
    session->capyio_authorized_selection = plan.selection;
    session->capyio_selection_authorized = true;
    session->framegen_refresh_multiplier = 1;
    session->rtsp_cipher.emplace(session->gcm_key, false);
    session->rtsp_url_scheme = "rtspenc://";
    session->rtsp_iv_counter = 0;
#ifdef _WIN32
    std::promise<
        rtsp_stream::launch_session_t::display_helper_gate_status_e>
        gate;
    gate.set_value(
        rtsp_stream::launch_session_t::display_helper_gate_status_e::proceed);
    session->display_helper_gate = gate.get_future().share();
#endif
    runtime->staged_launch_session = std::move(session);
  } catch (...) {
    return LaunchSessionResult::allocation_failed;
  }
  return LaunchSessionResult::ok;
}

bool has_staged_upstream_launch_session(
    const UpstreamRuntime *runtime) noexcept {
  return runtime != nullptr &&
         static_cast<bool>(runtime->staged_launch_session);
}

bool request_upstream_key_frame(UpstreamRuntime *runtime) noexcept {
  if (runtime == nullptr || runtime->raised_launch_session_id == 0 ||
      !runtime->listener_thread.joinable() || !mail::man) {
    return false;
  }
  try {
    const auto idr = mail::man->event<bool>(mail::idr);
    if (!idr) {
      return false;
    }
    idr->raise(true);
    return true;
  } catch (...) {
    return false;
  }
}

void discard_staged_upstream_launch_session(UpstreamRuntime *runtime) noexcept {
  if (runtime != nullptr && runtime->staged_launch_session) {
    auto &session = *runtime->staged_launch_session;
    session.rtsp_cipher.reset();
    std::fill(session.gcm_key.begin(), session.gcm_key.end(), 0);
    std::fill(session.iv.begin(), session.iv.end(), 0);
    runtime->staged_launch_session.reset();
  }
}

}  // namespace capyio::display_engine
#endif
