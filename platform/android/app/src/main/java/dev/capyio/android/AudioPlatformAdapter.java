package dev.capyio.android;

import dev.capyio.android.contract.ActualAudioFormat;

interface AudioPlatformAdapter {
    interface Listener {
        void onStarted(long generation, ActualAudioFormat actualFormat);

        void onFrames(long generation, long frames);

        void onStopped(long generation);

        void onFailed(long generation, String problemCode);
    }

    void start(long generation);

    void stop(long generation);
}
