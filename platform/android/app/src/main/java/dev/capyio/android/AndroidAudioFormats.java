package dev.capyio.android;

import android.media.AudioFormat;

final class AndroidAudioFormats {
    private AndroidAudioFormats() {}

    static String canonicalEncoding(int androidEncoding) {
        if (androidEncoding == AudioFormat.ENCODING_PCM_16BIT) {
            return "pcm_s16le";
        }
        throw new IllegalArgumentException("unsupported Android audio encoding");
    }

    static int bytesPerFrame(int androidEncoding, int channelCount) {
        if (androidEncoding != AudioFormat.ENCODING_PCM_16BIT
                || channelCount < 1
                || channelCount > 8) {
            throw new IllegalArgumentException("unsupported Android audio frame format");
        }
        return 2 * channelCount;
    }
}
