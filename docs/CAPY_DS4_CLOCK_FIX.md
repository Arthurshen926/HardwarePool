# DS4 USB motion clock compatibility fix

Date: 2026-09-29. Status: implementation, physical acceptance pending.

## Evidence and scope

The user reports that a real DS4 over USB/IP shows motion in the same online
tester. CapyIO's current VIIPER executable is upstream v0.7.0, commit
`6b71b148a2243fab77ee1a46f4e22e00bd7d5a04`, SHA256
`1868d682f4cc6d62349bbccbf0727b05d3eb6e22027ac34f0f1d9b1de56f2ddc`.
Its `device/dualshock4/device.go::nextReportTimestamp` returns only the last
report interval, but writes it into the USB report's cumulative sensor clock.
This is a concrete report compatibility defect, not proof of the sole cause
of the user's observed absence of motion.

The local patch retains a monotonic start time and computes elapsed 16/3-us
ticks, retaining the initial 188 offset. USB serialization retains normal
16-bit wrap. The input wire and CapyIO IMU timestamps remain unchanged.
The probe separately records gyro/accel ranges and unsigned timestamp deltas;
generic changed-report counts are not motion acceptance evidence.

## External boundary and reproduction

- Upstream: https://github.com/Alia5/VIIPER, tag v0.7.0, GPL-3.0.
- Exact source commit above; source checkout and binaries remain local ignored
  artifacts under `target/physical-lab`, not linked into Core or distributed.
- Tracked modification: `platform/windows/capyio-input/tools/viiper-ds4-clock.patch`.
- Apply with `git apply` to the exact upstream checkout.
- Go 1.26.2 official Windows amd64 archive SHA256:
  `98eb3570bade15cb826b0909338df6cc6d2cf590bc39c471142002db3832b708`.
- Copy `platform/windows/capyio-input/tools/viiper-clock-regression_test.go`
  into the checkout's `device/dualshock4` and run
  `go test -mod=readonly ./device/dualshock4 -run TestCapyioReportTimestampIsCumulative -count=1`.
- Build: `go build -mod=readonly -trimpath -tags release -ldflags "-X main.Version=v0.7.0 -X main.Commit=6b71b148-capyio-ds4-clock -X github.com/Alia5/VIIPER/internal/codegen/common.Version=0.7.0" -o ../viiper-ds4-motion-fixed.exe ./cmd/viiper`.
- API compatibility remains v0.7.0; the executable commit label and this record
  identify the local patch. This is not an unmodified upstream release.
- Only the previously used loopback endpoints and disabled auto-attach flags
  may be used. No driver installation, boot policy or pairing changes.

Keep the original executable untouched for rollback. Stop the owned DS4 through
CapyIO before stopping the exact VIIPER process. Restart the original executable
with its recorded arguments to roll back; do not detach unrelated USB/IP devices.

## Acceptance still required

- Counter advances across report intervals and wraps without frequent backward jumps.
- A confirmed physical rotation produces clearly changing raw gyro values.
- The user's DS4-capable consumer responds correctly. Do not substitute browser
  Gamepad API stick movement for native gyro acceptance.

## Deployment evidence

The fixed-clock test and local build passed. API version metadata was initially
missing; read-only preflight correctly rejected that intermediate build. The
final build supplies the API compatibility version without relaxing the host's
version check. Final SHA256:
`e8981fbed5f361f7287e7554d8b62f937c8220d248d0078fb897c64e1d8994c7`.

After the user confirmed DS4 and CapyIO closed, exact old process 113752 was
stopped. The patched helper now runs as process 41596 using the same loopback
arguments. The original executable remains untouched. Read-only preflight
reports VIIPER ready, USB/IP ready, zero exports and no problem. This is helper
readiness, not yet physical motion acceptance. The rebuilt desktop includes
received-motion values and clarification next to DS4 controls; it was replaced
while closed. Android remains 0.5.20 and requires no packet-format update.

After the user reopened DS4, a 40-second native HID read received 66,684 reports.
The corrected counter advanced 33,148 times, with zero backward/long-gap deltas.
Raw gyro ranges were [-8,9], [-1,1], [-1,1]; acceleration ranges [20,42],
[3381,3749], [3529,3647]. The user was asked to rotate during that interval,
but confirmation of overlap is outstanding. Clock behavior passed; deliberate
rotation amplitude and the online tester must not be marked passed.
