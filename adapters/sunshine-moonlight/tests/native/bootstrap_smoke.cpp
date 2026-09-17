#include "capyio_display_engine_bootstrap.h"

#include <array>
#include <cstdint>
#include <cstring>
#include <iostream>
#include <string>

namespace {
using capyio::display_engine::Bootstrap;
using capyio::display_engine::BootstrapHooks;
using capyio::display_engine::BootstrapResult;
using capyio::display_engine::BootstrapState;
using capyio::display_engine::AnnouncedStreamSelection;
using capyio::display_engine::AuthorizedStreamSelection;
using capyio::display_engine::ListenerStartResult;
using capyio::display_engine::LaunchSessionMaterial;
using capyio::display_engine::LaunchSessionPlan;
using capyio::display_engine::LaunchSessionResult;

struct Fixture {
  std::string calls;
  bool mail_ok {true};
  bool platform_ok {true};
  bool encoder_ok {true};
  ListenerStartResult listener_result {ListenerStartResult::ready};
  bool listener_stop_ok {true};
  std::uint32_t observed_timeout {0};
};

bool start_mail(void *context) noexcept {
  auto &fixture = *static_cast<Fixture *>(context);
  fixture.calls += 'M';
  return fixture.mail_ok;
}

void stop_mail(void *context) noexcept {
  static_cast<Fixture *>(context)->calls += 'm';
}

bool start_platform(void *context) noexcept {
  auto &fixture = *static_cast<Fixture *>(context);
  fixture.calls += 'P';
  return fixture.platform_ok;
}

void stop_platform(void *context) noexcept {
  static_cast<Fixture *>(context)->calls += 'p';
}

bool probe_encoder(void *context) noexcept {
  auto &fixture = *static_cast<Fixture *>(context);
  fixture.calls += 'E';
  return fixture.encoder_ok;
}

ListenerStartResult start_listener(
    void *context, const std::uint32_t timeout_millis) noexcept {
  auto &fixture = *static_cast<Fixture *>(context);
  fixture.calls += 'R';
  fixture.observed_timeout = timeout_millis;
  return fixture.listener_result;
}

bool stop_listener(
    void *context, const std::uint32_t timeout_millis) noexcept {
  auto &fixture = *static_cast<Fixture *>(context);
  fixture.calls += 'L';
  fixture.observed_timeout = timeout_millis;
  return fixture.listener_stop_ok;
}

BootstrapHooks hooks(Fixture &fixture) noexcept {
  return BootstrapHooks {
      &fixture,
      start_mail,
      stop_mail,
      start_platform,
      stop_platform,
      probe_encoder,
      start_listener,
      stop_listener,
  };
}

bool expect(const bool condition, const char *message) {
  if (!condition) {
    std::cerr << message << '\n';
  }
  return condition;
}

CapyIoDisplayEngineStartV2 valid_start() {
  CapyIoDisplayEngineStartV2 start {};
  start.struct_size = sizeof(start);
  start.abi_version = CAPYIO_DISPLAY_ENGINE_ABI_V2;
  start.adapter_instance_id[0] = 1;
  for (std::uint8_t index = 0; index < 16; ++index) {
    start.display_capability_id[index] = static_cast<std::uint8_t>(index + 1);
  }
  std::memcpy(start.capture_output_utf8, "\\\\.\\DISPLAY2", 12);
  start.width = 2640;
  start.height = 1216;
  start.refresh_millihertz = 120000;
  start.codec = 2;
  start.dynamic_range = 2;
  start.bitrate_kbps = 45000;
  start.https_port = 49995;
  start.video_port = 50009;
  start.control_port = 50010;
  start.rtsp_port = 50021;
  return start;
}

LaunchSessionMaterial valid_material() {
  LaunchSessionMaterial material {};
  material.launch_session_id = 7;
  for (std::uint8_t index = 0; index < 16; ++index) {
    material.key[index] = static_cast<std::uint8_t>(0xa0 + index);
  }
  material.key_id = 0x12345678;
  for (std::uint8_t index = 0; index < 8; ++index) {
    material.ping_payload[index] = static_cast<std::uint8_t>(0x10 + index);
  }
  material.control_connect_data = 0x90abcdef;
  return material;
}
}  // namespace

int main() {
  bool ok = true;

  Fixture success;
  Bootstrap running {hooks(success)};
  ok &= expect(running.start(750) == BootstrapResult::ok,
               "complete bootstrap failed");
  ok &= expect(running.state() == BootstrapState::running &&
                   success.calls == "MPER" &&
                   success.observed_timeout == 750,
               "start ownership order or timeout forwarding changed");
  ok &= expect(running.start(750) == BootstrapResult::invalid_state,
               "second start was accepted");
  success.listener_stop_ok = false;
  ok &= expect(running.stop(500) == BootstrapResult::timeout &&
                   running.state() == BootstrapState::running &&
                   success.calls == "MPERL",
               "stop timeout released owned resources");
  success.listener_stop_ok = true;
  ok &= expect(running.stop(1000) == BootstrapResult::ok &&
                   running.state() == BootstrapState::idle &&
                   success.calls == "MPERLLpm",
               "successful stop did not release in reverse order");

  Fixture mail_failure;
  mail_failure.mail_ok = false;
  Bootstrap no_mail {hooks(mail_failure)};
  ok &= expect(no_mail.start(1000) == BootstrapResult::mail_start_failed &&
                   mail_failure.calls == "M",
               "mail failure released an unowned resource");

  Fixture platform_failure;
  platform_failure.platform_ok = false;
  Bootstrap no_platform {hooks(platform_failure)};
  ok &= expect(
      no_platform.start(1000) == BootstrapResult::platform_start_failed &&
          platform_failure.calls == "MPm",
      "platform failure rollback order changed");

  Fixture encoder_failure;
  encoder_failure.encoder_ok = false;
  Bootstrap no_encoder {hooks(encoder_failure)};
  ok &= expect(
      no_encoder.start(1000) == BootstrapResult::encoder_probe_failed &&
          encoder_failure.calls == "MPEpm",
      "encoder failure rollback order changed");

  Fixture bind_failure;
  bind_failure.listener_result = ListenerStartResult::bind_failed;
  Bootstrap no_listener {hooks(bind_failure)};
  ok &= expect(
      no_listener.start(1000) == BootstrapResult::listener_bind_failed &&
          bind_failure.calls == "MPERpm",
      "listener bind failure rollback order changed");

  Fixture start_timeout;
  start_timeout.listener_result = ListenerStartResult::timeout_quiescent;
  Bootstrap timed_out {hooks(start_timeout)};
  ok &= expect(timed_out.start(1000) == BootstrapResult::timeout &&
                   timed_out.state() == BootstrapState::idle &&
                   start_timeout.calls == "MPERpm",
               "quiescent start timeout retained resources");

  Fixture owned_timeout;
  owned_timeout.listener_result = ListenerStartResult::timeout_owned;
  Bootstrap needs_cleanup {hooks(owned_timeout)};
  ok &= expect(needs_cleanup.start(1000) == BootstrapResult::timeout &&
                   needs_cleanup.state() == BootstrapState::cleanup_required &&
                   owned_timeout.calls == "MPER",
               "owned start timeout released live dependencies");
  ok &= expect(needs_cleanup.stop(1000) == BootstrapResult::ok &&
                   needs_cleanup.state() == BootstrapState::idle &&
                   owned_timeout.calls == "MPERLpm",
               "owned start timeout could not be cleaned up");

  Fixture invalid;
  Bootstrap invalid_timeout {hooks(invalid)};
  ok &= expect(invalid_timeout.start(99) == BootstrapResult::invalid_argument &&
                   invalid.calls.empty(),
               "invalid timeout invoked a hook");

  auto start = valid_start();
  auto material = valid_material();
  LaunchSessionPlan plan {};
  ok &= expect(
      capyio::display_engine::build_launch_session_plan(
          start, material, plan) == LaunchSessionResult::ok,
      "valid authenticated launch session was rejected");
  const std::array<std::uint8_t, 16> expected_iv {
      0x12, 0x34, 0x56, 0x78, 0, 0, 0, 0,
      0, 0, 0, 0, 0, 0, 0, 0};
  ok &= expect(
      plan.launch_session_id == material.launch_session_id &&
          plan.key == material.key && plan.iv == expected_iv &&
          plan.ping_payload == material.ping_payload &&
          plan.control_connect_data == material.control_connect_data &&
          plan.display_sink_identity.front() == 1 &&
          plan.display_sink_identity.back() == 16 &&
          plan.selection.width == 2640 && plan.selection.height == 1216 &&
          plan.selection.refresh_millihertz == 120000 &&
          plan.selection.codec == 2 && plan.selection.dynamic_range == 2 &&
          plan.selection.bitrate_kbps == 45000 && plan.encrypted_rtsp &&
          !plan.audio_enabled && !plan.input_enabled &&
          !plan.upstream_virtual_display_enabled &&
          plan.display_config_preapplied,
      "launch plan did not preserve exact selection and display-only policy");

  const AnnouncedStreamSelection exact_announce {
      2640, 1216, 120000, 2, 2, 45000};
  ok &= expect(
      capyio::display_engine::authorized_stream_selection_matches(
          plan.selection, exact_announce),
      "exact RTSP selection did not match authorization");
  auto mismatched_announce = exact_announce;
  ++mismatched_announce.width;
  ok &= expect(
      !capyio::display_engine::authorized_stream_selection_matches(
          plan.selection, mismatched_announce),
      "RTSP width mismatch was accepted");
  mismatched_announce = exact_announce;
  mismatched_announce.refresh_millihertz = 119999;
  ok &= expect(
      !capyio::display_engine::authorized_stream_selection_matches(
          plan.selection, mismatched_announce),
      "RTSP refresh mismatch was accepted");
  mismatched_announce = exact_announce;
  mismatched_announce.codec = 1;
  ok &= expect(
      !capyio::display_engine::authorized_stream_selection_matches(
          plan.selection, mismatched_announce),
      "RTSP codec mismatch was accepted");
  mismatched_announce = exact_announce;
  mismatched_announce.dynamic_range = 1;
  ok &= expect(
      !capyio::display_engine::authorized_stream_selection_matches(
          plan.selection, mismatched_announce),
      "RTSP dynamic-range mismatch was accepted");
  mismatched_announce = exact_announce;
  mismatched_announce.bitrate_kbps = 44999;
  ok &= expect(
      !capyio::display_engine::authorized_stream_selection_matches(
          plan.selection, mismatched_announce),
      "RTSP bitrate mismatch was accepted");

  auto missing_material = material;
  missing_material.key.fill(0);
  ok &= expect(
      capyio::display_engine::build_launch_session_plan(
          start, missing_material, plan) ==
              LaunchSessionResult::invalid_material &&
          plan.key == std::array<std::uint8_t, 16> {},
      "zero session key was accepted or prior material remained visible");
  missing_material = material;
  missing_material.launch_session_id = 0;
  ok &= expect(
      capyio::display_engine::build_launch_session_plan(
          start, missing_material, plan) ==
          LaunchSessionResult::invalid_material,
      "zero launch-session identity was accepted");

  auto invalid_start = start;
  invalid_start.width = 8193;
  ok &= expect(
      capyio::display_engine::build_launch_session_plan(
          invalid_start, material, plan) == LaunchSessionResult::invalid_start,
      "oversized width was accepted");
  invalid_start = start;
  invalid_start.refresh_millihertz = 500001;
  ok &= expect(
      capyio::display_engine::build_launch_session_plan(
          invalid_start, material, plan) == LaunchSessionResult::invalid_start,
      "oversized refresh rate was accepted");
  invalid_start = start;
  invalid_start.bitrate_kbps = 200001;
  ok &= expect(
      capyio::display_engine::build_launch_session_plan(
          invalid_start, material, plan) == LaunchSessionResult::invalid_start,
      "oversized bitrate was accepted");
  invalid_start = start;
  invalid_start.codec = 1;
  ok &= expect(
      capyio::display_engine::build_launch_session_plan(
          invalid_start, material, plan) == LaunchSessionResult::invalid_start,
      "HDR over H.264 was accepted");
  invalid_start = start;
  ++invalid_start.video_port;
  ok &= expect(
      capyio::display_engine::build_launch_session_plan(
          invalid_start, material, plan) == LaunchSessionResult::invalid_start,
      "inconsistent port namespace was accepted");

  return ok ? 0 : 1;
}
