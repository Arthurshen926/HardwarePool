package dev.capyio.android;

import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.os.Bundle;

import dev.capyio.android.lan.NativeLanMicrophoneSessionConfig;

import java.util.UUID;

/** Reads build-time trusted-lab microphone authority from private app metadata. */
final class NativeMicrophoneConfigLoader {
    private static final String PREFIX = "dev.capyio.android.MICROPHONE_LAB_";

    private NativeMicrophoneConfigLoader() {}

    static NativeLanMicrophoneSessionConfig load(Context context) {
        AndroidAudioPeer saved = AndroidAudioPeerStore.load(context, "microphone");
        if (saved != null) return fromPeer(saved);
        try {
            ApplicationInfo info = context.getPackageManager().getApplicationInfo(
                    context.getPackageName(), PackageManager.GET_META_DATA);
            Bundle metadata = info.metaData;
            if (metadata == null || !Boolean.parseBoolean(value(metadata, "ENABLED"))) {
                return null;
            }
            return new NativeLanMicrophoneSessionConfig(
                    value(metadata, "PEER_IPV4"),
                    positiveU16(metadata, "LOCAL_PORT"),
                    positiveU16(metadata, "PEER_PORT"),
                    UUID.fromString(value(metadata, "SESSION_ID")),
                    UUID.fromString(value(metadata, "ROUTE_ID")),
                    UUID.fromString(value(metadata, "STREAM_ID")),
                    positiveInt(metadata, "STREAM_EPOCH"));
        } catch (PackageManager.NameNotFoundException impossible) {
            throw new IllegalStateException("own package metadata is unavailable", impossible);
        }
    }

    static NativeLanMicrophoneSessionConfig fromPeer(AndroidAudioPeer peer) {
        return new NativeLanMicrophoneSessionConfig(peer.host(), peer.localPort(), peer.peerPort(),
                UUID.fromString("a2000000-0000-4000-8000-000000000001"),
                UUID.fromString("a2000000-0000-4000-8000-000000000002"),
                UUID.fromString("a2000000-0000-4000-8000-000000000003"), 1);
    }

    private static String value(Bundle metadata, String suffix) {
        Object value = metadata.get(PREFIX + suffix);
        if (value == null) {
            throw new IllegalArgumentException("native microphone metadata is incomplete");
        }
        String text = value.toString();
        if (text.isEmpty() || text.length() > 64) {
            throw new IllegalArgumentException("native microphone metadata is outside bounds");
        }
        return text;
    }

    private static int positiveU16(Bundle metadata, String suffix) {
        int value = positiveInt(metadata, suffix);
        if (value > 65_535) {
            throw new IllegalArgumentException("native microphone port exceeds u16");
        }
        return value;
    }

    private static int positiveInt(Bundle metadata, String suffix) {
        int value;
        try {
            value = Integer.parseInt(value(metadata, suffix));
        } catch (NumberFormatException malformed) {
            throw new IllegalArgumentException("native microphone integer is malformed", malformed);
        }
        if (value < 1) {
            throw new IllegalArgumentException("native microphone integer must be positive");
        }
        return value;
    }
}
