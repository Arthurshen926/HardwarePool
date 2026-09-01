package dev.capyio.android;

import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.os.Bundle;

import dev.capyio.android.lan.NativeLanSpeakerSessionConfig;

import java.util.UUID;

/** Reads build-time trusted-lab speaker authority from private app metadata. */
final class NativeSpeakerConfigLoader {
    private static final String PREFIX = "dev.capyio.android.SPEAKER_LAB_";

    private NativeSpeakerConfigLoader() {}

    static NativeLanSpeakerSessionConfig load(Context context) {
        try {
            ApplicationInfo info = context.getPackageManager().getApplicationInfo(
                    context.getPackageName(), PackageManager.GET_META_DATA);
            Bundle metadata = info.metaData;
            if (metadata == null || !Boolean.parseBoolean(value(metadata, "ENABLED"))) {
                return null;
            }
            return new NativeLanSpeakerSessionConfig(
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

    private static String value(Bundle metadata, String suffix) {
        Object value = metadata.get(PREFIX + suffix);
        if (value == null) {
            throw new IllegalArgumentException("native speaker metadata is incomplete");
        }
        String text = value.toString();
        if (text.isEmpty() || text.length() > 64) {
            throw new IllegalArgumentException("native speaker metadata is outside bounds");
        }
        return text;
    }

    private static int positiveU16(Bundle metadata, String suffix) {
        int value = positiveInt(metadata, suffix);
        if (value > 65_535) {
            throw new IllegalArgumentException("native speaker port exceeds u16");
        }
        return value;
    }

    private static int positiveInt(Bundle metadata, String suffix) {
        int value;
        try {
            value = Integer.parseInt(value(metadata, suffix));
        } catch (NumberFormatException malformed) {
            throw new IllegalArgumentException("native speaker integer is malformed", malformed);
        }
        if (value < 1) {
            throw new IllegalArgumentException("native speaker integer must be positive");
        }
        return value;
    }
}
