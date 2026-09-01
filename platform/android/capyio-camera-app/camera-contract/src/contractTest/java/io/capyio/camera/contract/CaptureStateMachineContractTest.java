package io.capyio.camera.contract;

import io.capyio.camera.contract.CaptureStateMachine.Effect;
import io.capyio.camera.contract.CaptureStateMachine.Event;
import io.capyio.camera.contract.CaptureStateMachine.State;

public final class CaptureStateMachineContractTest {
    private CaptureStateMachineContractTest() {}

    public static void main(String[] args) {
        permissionAndVisibilityLifecycleIsClosed();
        foregroundServiceOwnershipIgnoresActivityLifecycle();
        grantedPermissionStartsAndFailureCloses();
        frameObservationRejectsInvalidMetadata();
        cameraFacingSelectionIsDeterministicAndBounded();
        cameraInventoryIsBoundedAndJsonSafe();
        cameraSourceSelectionUsesVendorNeutralZoomTargets();
        cameraProgressWatchdogHasAClosedMonotonicBoundary();
        cameraTransportEndpointIsClosedAndLiteralOnly();
        qualityPresetIsBoundedAndCyclesDeterministically();
        loopbackConnectRetryIsFiniteAndExplicit();
        avcContractsAreBoundedAndImmutable();
        avcQueueDropsOldestWithoutBlocking();
        avcWireRecordsMatchRustGoldens();
        avcWireSessionDetectsLayoutsAndRecoversOnlyAtKeyFrames();
        System.out.println("Camera capture contract tests: PASS");
    }

    private static void foregroundServiceOwnershipIgnoresActivityLifecycle() {
        CaptureOwnershipStateMachine machine = new CaptureOwnershipStateMachine();
        expect(
                CaptureOwnershipStateMachine.Effect.START_FOREGROUND_SERVICE,
                machine.handle(CaptureOwnershipStateMachine.Event.USER_START_VISIBLE).effect());
        expect(
                CaptureOwnershipStateMachine.State.SERVICE_OWNED,
                machine.handle(CaptureOwnershipStateMachine.Event.SERVICE_STARTED).current());

        machine.handle(CaptureOwnershipStateMachine.Event.ACTIVITY_PAUSED);
        expect(CaptureOwnershipStateMachine.State.SERVICE_OWNED, machine.state());
        machine.handle(CaptureOwnershipStateMachine.Event.CONFIGURATION_CHANGED);
        expect(CaptureOwnershipStateMachine.State.SERVICE_OWNED, machine.state());
        machine.handle(CaptureOwnershipStateMachine.Event.ACTIVITY_RESUMED);
        expect(CaptureOwnershipStateMachine.State.SERVICE_OWNED, machine.state());

        expect(
                CaptureOwnershipStateMachine.Effect.STOP_SERVICE,
                machine.handle(CaptureOwnershipStateMachine.Event.USER_STOP).effect());
        expect(
                CaptureOwnershipStateMachine.State.STOPPED,
                machine.handle(CaptureOwnershipStateMachine.Event.SESSION_CLOSED).current());

        expect(
                CaptureOwnershipStateMachine.Effect.START_FOREGROUND_SERVICE,
                machine.handle(CaptureOwnershipStateMachine.Event.USER_START_VISIBLE).effect());
        expect(
                CaptureOwnershipStateMachine.Effect.STOP_SERVICE,
                machine.handle(CaptureOwnershipStateMachine.Event.SERVICE_FAILED).effect());
        expect(
                CaptureOwnershipStateMachine.State.STOPPED,
                machine.handle(CaptureOwnershipStateMachine.Event.SESSION_CLOSED).current());
    }

    private static void permissionAndVisibilityLifecycleIsClosed() {
        CaptureStateMachine machine = new CaptureStateMachine();
        expect(State.IDLE, machine.state());
        expect(Effect.REQUEST_PERMISSION, machine.handle(Event.USER_START_WITHOUT_PERMISSION).effect());
        expect(State.AWAITING_PERMISSION, machine.state());
        expect(Effect.OPEN_CAMERA, machine.handle(Event.PERMISSION_GRANTED).effect());
        expect(State.STARTING, machine.state());
        expect(State.STREAMING, machine.handle(Event.SESSION_STARTED).current());
        expect(Effect.CLOSE_CAMERA, machine.handle(Event.HOST_PAUSED).effect());
        expect(State.STOPPING, machine.state());
        expect(State.STOPPED, machine.handle(Event.SESSION_CLOSED).current());
    }

    private static void grantedPermissionStartsAndFailureCloses() {
        CaptureStateMachine machine = new CaptureStateMachine();
        expect(Effect.OPEN_CAMERA, machine.handle(Event.USER_START_WITH_PERMISSION).effect());
        expect(Effect.CLOSE_CAMERA, machine.handle(Event.FAILURE).effect());
        expect(State.ERROR, machine.state());
        expect(State.STOPPING, machine.handle(Event.USER_STOP).current());
        expect(State.STOPPED, machine.handle(Event.SESSION_CLOSED).current());

        expect(Effect.REQUEST_PERMISSION, machine.handle(Event.USER_START_WITHOUT_PERMISSION).effect());
        expect(State.STOPPED, machine.handle(Event.PERMISSION_DENIED).current());
    }

    private static void frameObservationRejectsInvalidMetadata() {
        FrameObservation observation = new FrameObservation(
                1280, 720, 1, 1, 90, FrameObservation.LensFacing.BACK);
        expect(1280, observation.width());
        expect(FrameObservation.LensFacing.BACK, observation.lensFacing());

        expectThrows(() -> new FrameObservation(
                1279, 720, 1, 1, 90, FrameObservation.LensFacing.BACK));
        expectThrows(() -> new FrameObservation(
                1280, 720, 1, 0, 90, FrameObservation.LensFacing.BACK));
        expectThrows(() -> new FrameObservation(
                1280, 720, 1, 1, 45, FrameObservation.LensFacing.BACK));
    }

    private static void cameraFacingSelectionIsDeterministicAndBounded() {
        expect(
                FrameObservation.LensFacing.FRONT,
                CameraFacingPolicy.toggle(FrameObservation.LensFacing.BACK));
        expect(
                FrameObservation.LensFacing.BACK,
                CameraFacingPolicy.toggle(FrameObservation.LensFacing.FRONT));
        expect(
                FrameObservation.LensFacing.FRONT,
                CameraFacingPolicy.select(
                        FrameObservation.LensFacing.FRONT,
                        java.util.List.of(
                                FrameObservation.LensFacing.BACK,
                                FrameObservation.LensFacing.FRONT)));
        expect(
                FrameObservation.LensFacing.BACK,
                CameraFacingPolicy.select(
                        FrameObservation.LensFacing.FRONT,
                        java.util.List.of(FrameObservation.LensFacing.BACK)));
        expectThrows(() -> CameraFacingPolicy.select(
                FrameObservation.LensFacing.BACK,
                java.util.List.of()));
        expectThrows(() -> CameraFacingPolicy.select(
                FrameObservation.LensFacing.BACK,
                java.util.Collections.nCopies(33, FrameObservation.LensFacing.BACK)));
    }

    private static void cameraInventoryIsBoundedAndJsonSafe() {
        CameraInventory.Camera back = new CameraInventory.Camera(
                "0\"wide",
                CameraInventory.LensFacing.BACK,
                "full",
                90,
                java.util.List.of("2", "3"),
                java.util.List.of(
                        new CameraInventory.PhysicalLens(
                                "2", java.util.List.of(1_900), 7_600, 5_700),
                        new CameraInventory.PhysicalLens(
                                "3", java.util.List.of(6_700), 5_600, 4_200)),
                java.util.List.of(1_900, 6_700),
                600,
                10_000,
                java.util.List.of(new CameraInventory.Size(1280, 720)));
        CameraInventory.Camera front = new CameraInventory.Camera(
                "1",
                CameraInventory.LensFacing.FRONT,
                "limited",
                270,
                java.util.List.of(),
                java.util.List.of(),
                java.util.List.of(2_200),
                null,
                null,
                java.util.List.of(new CameraInventory.Size(640, 480)));
        CameraInventory inventory = new CameraInventory(
                java.util.List.of(back, front),
                java.util.List.of(java.util.List.of("0\"wide", "1")));
        String json = inventory.toJson();
        expect(true, json.startsWith("{\"version\":1,\"cameras\":["));
        expect(true, json.contains("\"id\":\"0\\\"wide\""));
        expect(true, json.contains("\"physicalIds\":[\"2\",\"3\"]"));
        expect(true, json.contains("\"sensorSizeMicroMm\":[7600,5700]"));
        expect(true, json.contains("\"concurrentGroups\":[[\"0\\\"wide\",\"1\"]]"));
        expectThrows(() -> new CameraInventory(
                java.util.Collections.nCopies(
                        CameraInventory.MAX_CAMERAS + 1,
                        back),
                java.util.List.of()));
        expectThrows(() -> new CameraInventory.Camera(
                "0",
                CameraInventory.LensFacing.BACK,
                "full",
                45,
                java.util.List.of(),
                java.util.List.of(),
                java.util.List.of(),
                null,
                null,
                java.util.List.of()));
        expectThrows(() -> new CameraInventory.Camera(
                "0",
                CameraInventory.LensFacing.BACK,
                "full",
                90,
                java.util.List.of("2"),
                java.util.List.of(new CameraInventory.PhysicalLens(
                        "3", java.util.List.of(2_000), null, null)),
                java.util.List.of(),
                null,
                null,
                java.util.List.of()));
        expectThrows(() -> new CameraInventory(
                java.util.List.of(back),
                java.util.List.of(java.util.List.of("0"))));
    }

    private static void cameraSourceSelectionUsesVendorNeutralZoomTargets() {
        CameraInventory.Camera back = new CameraInventory.Camera(
                "0",
                CameraInventory.LensFacing.BACK,
                "full",
                90,
                java.util.List.of("2", "3"),
                java.util.List.of(
                        new CameraInventory.PhysicalLens(
                                "2", java.util.List.of(1_900), 7_600, 5_700),
                        new CameraInventory.PhysicalLens(
                                "3", java.util.List.of(6_700), 5_600, 4_200)),
                java.util.List.of(1_900, 6_700),
                600,
                10_000,
                java.util.List.of(new CameraInventory.Size(1280, 720)));
        CameraInventory inventory = new CameraInventory(
                java.util.List.of(back), java.util.List.of());
        java.util.List<CameraSourceSelection> sources =
                CameraSourceSelection.enumerate(inventory);
        expect(4, sources.size());
        expect("0@auto", sources.get(0).key());
        expect("0@0.600x", sources.get(1).key());
        expect(600, sources.get(1).targetZoomRatioMilli());
        expect("0@1.000x", sources.get(2).key());
        expect("0@2.000x", sources.get(3).key());
        expect(sources.get(0), CameraSourceSelection.next(null, sources));
        expect(sources.get(1), CameraSourceSelection.next(sources.get(0), sources));
        expect(true, CameraSourceSelection.next(sources.get(3), sources) == null);
        expect(true, CameraSourceSelection.next(
                new CameraSourceSelection(
                        "9", CameraInventory.LensFacing.BACK, null),
                sources) == null);
        expectThrows(() -> CameraSourceSelection.next(null, java.util.List.of()));
        expectThrows(() -> new CameraSourceSelection(
                "0", CameraInventory.LensFacing.BACK, 0));
    }

    private static void cameraProgressWatchdogHasAClosedMonotonicBoundary() {
        expect(1_000, CameraProgressWatchdog.CHECK_INTERVAL_MILLIS);
        expect(5_000, CameraProgressWatchdog.STALL_TIMEOUT_MILLIS);
        expect(false, CameraProgressWatchdog.isExpired(5_999, 1_000));
        expect(true, CameraProgressWatchdog.isExpired(6_000, 1_000));
        expectThrows(() -> CameraProgressWatchdog.isExpired(-1, 0));
        expectThrows(() -> CameraProgressWatchdog.isExpired(1, 2));
    }

    private static void cameraTransportEndpointIsClosedAndLiteralOnly() {
        CameraTransportEndpoint adb = CameraTransportEndpoint.fromUserInput("  ");
        expect(CameraTransportEndpoint.Mode.ADB_REVERSE, adb.mode());
        expect("127.0.0.1", adb.host());
        expect(38_173, adb.port());
        byte[] copied = adb.addressBytes();
        copied[0] = 1;
        expect((byte) 127, adb.addressBytes()[0]);

        expect(
                "192.168.7.10",
                CameraTransportEndpoint.fromUserInput(" 192.168.7.10 ").host());
        expect(
                CameraTransportEndpoint.Mode.TRUSTED_LAN,
                CameraTransportEndpoint.fromUserInput("10.0.0.2").mode());
        expect(
                CameraTransportEndpoint.Mode.TRUSTED_LAN,
                CameraTransportEndpoint.fromUserInput("172.31.255.254").mode());
        expect(
                CameraTransportEndpoint.Mode.TRUSTED_LAN,
                CameraTransportEndpoint.fromUserInput("100.64.0.1").mode());
        expect(
                CameraTransportEndpoint.Mode.TRUSTED_LAN,
                CameraTransportEndpoint.fromUserInput("100.127.255.254").mode());
        expect(
                CameraTransportEndpoint.Mode.TRUSTED_LAN,
                CameraTransportEndpoint.fromUserInput("169.254.8.9").mode());

        expectThrows(() -> CameraTransportEndpoint.fromUserInput(null));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput("localhost"));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput("127.0.0.1"));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput("172.32.0.1"));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput("100.128.0.1"));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput("8.8.8.8"));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput("224.0.0.1"));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput("192.168.01.2"));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput("192.168.1"));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput("192.168.1.2:38173"));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput("fd00::1"));
        expectThrows(() -> CameraTransportEndpoint.fromUserInput(" ".repeat(33)));
    }

    private static void loopbackConnectRetryIsFiniteAndExplicit() {
        expect(true, LoopbackConnectRetryPolicy.mayAttempt(1));
        expect(true, LoopbackConnectRetryPolicy.mayAttempt(120));
        expect(false, LoopbackConnectRetryPolicy.mayAttempt(121));
        expect(true, LoopbackConnectRetryPolicy.shouldRetryAfterFailure(1));
        expect(false, LoopbackConnectRetryPolicy.shouldRetryAfterFailure(120));
        expect(500, LoopbackConnectRetryPolicy.CONNECT_TIMEOUT_MILLIS);
        expect(500, LoopbackConnectRetryPolicy.RETRY_DELAY_MILLIS);
        expectThrows(() -> LoopbackConnectRetryPolicy.shouldRetryAfterFailure(0));
    }

    private static void qualityPresetIsBoundedAndCyclesDeterministically() {
        expect(2_000_000, AvcQualityPreset.ECONOMY.bitrateForDimensions(1280, 720));
        expect(4_000_000, AvcQualityPreset.BALANCED.bitrateForDimensions(1280, 720));
        expect(6_000_000, AvcQualityPreset.CLEAR.bitrateForDimensions(1280, 720));
        expect(1_000_000, AvcQualityPreset.ECONOMY.bitrateForDimensions(640, 720));
        expect(AvcQualityPreset.BALANCED, AvcQualityPreset.ECONOMY.next());
        expect(AvcQualityPreset.CLEAR, AvcQualityPreset.BALANCED.next());
        expect(AvcQualityPreset.ECONOMY, AvcQualityPreset.CLEAR.next());
        expectThrows(() -> AvcQualityPreset.BALANCED.bitrateForDimensions(0, 720));
        expectThrows(() -> AvcQualityPreset.BALANCED.bitrateForDimensions(4097, 720));
    }

    private static void avcContractsAreBoundedAndImmutable() {
        AvcEncoderConfig config = AvcEncoderConfig.baseline720p30();
        expect(1280, config.width());
        expect(2, config.queueCapacity());
        expectThrows(() -> new AvcEncoderConfig(1279, 720, 30, 4_000_000, 0, 1, 4));
        expectThrows(() -> new AvcEncoderConfig(1280, 720, 0, 4_000_000, 0, 1, 4));
        expectThrows(() -> new AvcEncoderConfig(1280, 720, 30, 1, 0, 1, 4));
        expectThrows(() -> new AvcEncoderConfig(1280, 720, 30, 4_000_000, 45, 1, 4));

        byte[] bytes = new byte[] {1, 2, 3};
        EncodedAvcAccessUnit unit =
                new EncodedAvcAccessUnit(1, 10, false, true, false, bytes);
        bytes[0] = 99;
        expect((byte) 1, unit.payloadView().get());
        expectThrows(() -> new EncodedAvcAccessUnit(0, 10, false, false, false, bytes));
        expectThrows(() -> new EncodedAvcAccessUnit(1, 10, false, false, false, new byte[0]));

        AvcCodecConfig codecConfig = new AvcCodecConfig(new byte[] {0, 0, 0, 1}, new byte[0]);
        expect(4, codecConfig.csd0View().remaining());
        expectThrows(() -> new AvcCodecConfig(new byte[0], new byte[0]));
    }

    private static void avcQueueDropsOldestWithoutBlocking() {
        BoundedAvcAccessUnitQueue queue = new BoundedAvcAccessUnitQueue(2);
        expect(new BoundedAvcAccessUnitQueue.OfferResult(true, false), queue.offer(accessUnit(1)));
        expect(new BoundedAvcAccessUnitQueue.OfferResult(true, false), queue.offer(accessUnit(2)));
        expect(new BoundedAvcAccessUnitQueue.OfferResult(true, true), queue.offer(accessUnit(3)));
        expect(2L, queue.poll().orElseThrow().sequence());
        expect(3L, queue.poll().orElseThrow().sequence());
        expect(true, queue.poll().isEmpty());
    }

    private static void avcWireRecordsMatchRustGoldens() {
        byte[] streamId = new byte[16];
        for (int index = 0; index < streamId.length; index++) {
            streamId[index] = (byte) index;
        }
        AvcWireRecordEncoder.StreamKey stream =
                new AvcWireRecordEncoder.StreamKey(streamId, 2);
        AvcEncoderConfig encoderConfig = AvcEncoderConfig.baseline720p30();
        AvcCodecConfig codecConfig = new AvcCodecConfig(
                new byte[] {0, 0, 0, 1, 0x67, 0x64, 0, 0x1f},
                new byte[] {0, 0, 0, 1, 0x68, (byte) 0xee, 0x3c, (byte) 0x80});
        byte[] configRecord = AvcWireRecordEncoder.encodeConfig(
                stream,
                encoderConfig,
                AvcWireRecordEncoder.Layout.ANNEX_B,
                AvcWireRecordEncoder.Layout.ANNEX_B,
                codecConfig);
        expect(
                "434156430101010000380000000102030405060708090a0b0c0d0e0f"
                        + "0000000000000002000000000000000000000000000000000000002c"
                        + "050002d0001e0000003d090001010101010000000000000800000008"
                        + "000000016764001f0000000168ee3c80",
                hex(configRecord));

        EncodedAvcAccessUnit unit = new EncodedAvcAccessUnit(
                7,
                200_000,
                false,
                true,
                false,
                new byte[] {0, 0, 0, 1, 0x65, (byte) 0x88, (byte) 0x84});
        expect(
                "434156430101020500380000000102030405060708090a0b0c0d0e0f"
                        + "000000000000000200000000000000070000000000030d4000000007"
                        + "00000001658884",
                hex(AvcWireRecordEncoder.encodeAccessUnit(stream, unit, true)));

        expectThrows(() -> new AvcWireRecordEncoder.StreamKey(new byte[16], 1));
        expectThrows(() -> AvcWireRecordEncoder.encodeConfig(
                stream,
                encoderConfig,
                AvcWireRecordEncoder.Layout.AVC_DECODER_CONFIGURATION_RECORD,
                AvcWireRecordEncoder.Layout.ANNEX_B,
                codecConfig));
        EncodedAvcAccessUnit codecConfigUnit =
                new EncodedAvcAccessUnit(1, 0, true, false, false, new byte[] {1});
        expectThrows(() -> AvcWireRecordEncoder.encodeAccessUnit(
                stream, codecConfigUnit, false));
    }

    private static void avcWireSessionDetectsLayoutsAndRecoversOnlyAtKeyFrames() {
        byte[] streamId = new byte[16];
        streamId[15] = 7;
        AvcWireSessionEncoder session = new AvcWireSessionEncoder(
                new AvcWireRecordEncoder.StreamKey(streamId, 9),
                AvcEncoderConfig.baseline720p30());
        session.setCodecConfig(new AvcCodecConfig(
                new byte[] {0, 0, 0, 1, 0x67, 0x64, 0, 0x1f},
                new byte[] {0, 0, 0, 1, 0x68, (byte) 0xee, 0x3c, (byte) 0x80}));

        expect(0, session.encode(new EncodedAvcAccessUnit(
                1, 1_000, true, false, false, new byte[] {0, 0, 0, 1, 0x67})).size());
        expect(0, session.encode(new EncodedAvcAccessUnit(
                2, 2_000, false, false, false, new byte[] {0, 0, 0, 1, 0x41})).size());
        java.util.List<byte[]> first = session.encode(new EncodedAvcAccessUnit(
                3, 3_000, false, true, false, new byte[] {0, 0, 0, 1, 0x65}));
        expect(2, first.size());
        expect(1, first.get(0)[6] & 0xff);
        expect(2, first.get(1)[6] & 0xff);
        expect(0x05, first.get(1)[7] & 0xff);

        expect(1, session.encode(new EncodedAvcAccessUnit(
                4, 4_000, false, false, false, new byte[] {0, 0, 0, 1, 0x41})).size());
        expect(0, session.encode(new EncodedAvcAccessUnit(
                6, 6_000, false, false, false, new byte[] {0, 0, 0, 1, 0x41})).size());
        java.util.List<byte[]> recovered = session.encode(new EncodedAvcAccessUnit(
                7, 7_000, false, true, false, new byte[] {0, 0, 0, 1, 0x65}));
        expect(1, recovered.size());
        expect(0x05, recovered.get(0)[7] & 0xff);
        expect(7L, session.lastSentSequence());

        AvcWireSessionEncoder lengthPrefixed = new AvcWireSessionEncoder(
                new AvcWireRecordEncoder.StreamKey(streamId, 10),
                AvcEncoderConfig.baseline720p30());
        lengthPrefixed.setCodecConfig(new AvcCodecConfig(
                new byte[] {1, 0x64, 0, 0x1f, (byte) 0xff, (byte) 0xe1, 0},
                new byte[0]));
        java.util.List<byte[]> lengthRecords = lengthPrefixed.encode(
                new EncodedAvcAccessUnit(
                        1,
                        1_000,
                        false,
                        true,
                        false,
                        new byte[] {0, 0, 0, 2, 0x65, 1}));
        expect(2, lengthRecords.size());
        expect(2, lengthRecords.get(0)[56 + 12] & 0xff);
        expect(3, lengthRecords.get(0)[56 + 13] & 0xff);

        expectThrows(() -> session.encode(new EncodedAvcAccessUnit(
                7, 8_000, false, true, false, new byte[] {0, 0, 0, 1, 0x65})));
    }

    private static EncodedAvcAccessUnit accessUnit(long sequence) {
        return new EncodedAvcAccessUnit(
                sequence, sequence * 1_000, false, sequence == 1, false, new byte[] {(byte) sequence});
    }

    private static void expect(Object expected, Object actual) {
        if (!expected.equals(actual)) {
            throw new AssertionError("expected " + expected + " but got " + actual);
        }
    }

    private static void expectThrows(Runnable action) {
        try {
            action.run();
        } catch (IllegalArgumentException expected) {
            return;
        }
        throw new AssertionError("expected IllegalArgumentException");
    }

    private static String hex(byte[] bytes) {
        StringBuilder output = new StringBuilder(bytes.length * 2);
        for (byte value : bytes) {
            output.append(String.format("%02x", value & 0xff));
        }
        return output.toString();
    }
}
