package dev.capyio.android;

import android.media.AudioFormat;
import android.media.AudioRecord;
import android.media.MediaRecorder;

import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.lan.NativeLanMicrophoneSessionConfig;
import dev.capyio.android.lan.NativeLanPacketQueue;
import dev.capyio.android.lan.NativeLanPcmPacketizer;
import dev.capyio.android.lan.NativeLanSenderWorker;
import dev.capyio.android.lan.NativeLanUdpEndpoint;

import java.io.IOException;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

/** Android microphone Source backed by the trusted-lab native LAN sender. */
final class MicrophoneSourceAdapter implements AudioPlatformAdapter {
    static final String PROBLEM_BUSY = "CAPY.ANDROID.MIC_BUSY";
    static final String PROBLEM_PERMISSION = "CAPY.ANDROID.MIC_PERMISSION_DENIED";
    static final String PROBLEM_UNSUPPORTED = "CAPY.ANDROID.MIC_FORMAT_UNSUPPORTED";
    static final String PROBLEM_START = "CAPY.ANDROID.MIC_START_FAILED";
    static final String PROBLEM_READ = "CAPY.ANDROID.MIC_READ_FAILED";
    static final String PROBLEM_CONFIG = "CAPY.ANDROID.MIC_CONFIG_MISSING";
    static final String PROBLEM_NETWORK = "CAPY.ANDROID.MIC_NETWORK_FAILED";

    private static final int REQUESTED_SAMPLE_RATE = 48_000;
    private static final int REQUESTED_CHANNEL_MASK = AudioFormat.CHANNEL_IN_MONO;
    private static final int REQUESTED_ENCODING = AudioFormat.ENCODING_PCM_16BIT;
    private static final int TEN_MILLISECONDS_FRAMES = REQUESTED_SAMPLE_RATE / 100;
    private static final int MAX_WORK_BUFFER_BYTES = 256 * 1024;
    private static final int MAX_RECORD_BUFFER_BYTES = 512 * 1024;
    private static final int PACKET_QUEUE_CAPACITY = 128;
    private static final int PACKET_QUEUE_BYTES = 256 * 1024;
    private static final int SOCKET_TIMEOUT_MILLIS = 50;

    private final Listener listener;
    private final NativeLanMicrophoneSessionConfig config;
    private final AtomicBoolean running = new AtomicBoolean();
    private final AtomicLong completionGeneration = new AtomicLong();

    private volatile AudioRecord recorder;
    private volatile Thread worker;
    private volatile NativeLanSenderWorker senderWorker;
    private volatile NativeLanPcmPacketizer packetizer;
    private volatile NativeLanPacketQueue packetQueue;
    private volatile NativeLanUdpEndpoint endpoint;
    private volatile long activeGeneration;

    MicrophoneSourceAdapter(
            Listener listener,
            NativeLanMicrophoneSessionConfig config) {
        this.listener = listener;
        this.config = config;
    }

    @Override
    public synchronized void start(long generation) {
        if (worker != null) {
            listener.onFailed(generation, PROBLEM_BUSY);
            return;
        }
        if (config == null) {
            listener.onFailed(generation, PROBLEM_CONFIG);
            return;
        }
        completionGeneration.set(generation);
        activeGeneration = generation;
        running.set(true);
        Thread nextWorker = new Thread(() -> capture(generation), "capyio-microphone-source");
        nextWorker.setDaemon(true);
        worker = nextWorker;
        nextWorker.start();
    }

    @Override
    public void stop(long generation) {
        completionGeneration.set(generation);
        running.set(false);
        AudioRecord current = recorder;
        NativeLanSenderWorker sender = senderWorker;
        if (sender != null) {
            sender.stop();
        }
        if (current != null) {
            try {
                current.stop();
            } catch (IllegalStateException ignored) {
                // The capture thread still owns release and the stopped completion.
            }
        } else if (worker == null) {
            listener.onStopped(generation);
        }
    }

    private void capture(long startGeneration) {
        boolean failed = false;
        AtomicBoolean mediaFailed = new AtomicBoolean();
        AudioRecord localRecorder = null;
        NativeLanSenderWorker localSender = null;
        try {
            int minimumBytes = AudioRecord.getMinBufferSize(
                    REQUESTED_SAMPLE_RATE,
                    REQUESTED_CHANNEL_MASK,
                    REQUESTED_ENCODING);
            int requestedChunkBytes = TEN_MILLISECONDS_FRAMES * 2;
            int workBufferBytes = Math.max(minimumBytes, requestedChunkBytes);
            if (minimumBytes <= 0 || workBufferBytes > MAX_WORK_BUFFER_BYTES) {
                failed = true;
                listener.onFailed(startGeneration, PROBLEM_UNSUPPORTED);
                return;
            }
            int recordBufferBytes = Math.min(workBufferBytes * 2, MAX_RECORD_BUFFER_BYTES);
            localRecorder = new AudioRecord.Builder()
                    .setAudioSource(MediaRecorder.AudioSource.DEFAULT)
                    .setAudioFormat(new AudioFormat.Builder()
                            .setSampleRate(REQUESTED_SAMPLE_RATE)
                            .setChannelMask(REQUESTED_CHANNEL_MASK)
                            .setEncoding(REQUESTED_ENCODING)
                            .build())
                    .setBufferSizeInBytes(recordBufferBytes)
                    .build();
            recorder = localRecorder;
            if (localRecorder.getState() != AudioRecord.STATE_INITIALIZED) {
                failed = true;
                listener.onFailed(startGeneration, PROBLEM_START);
                return;
            }

            AudioFormat platformFormat = localRecorder.getFormat();
            if (platformFormat.getSampleRate() != NativeLanMicrophoneSessionConfig.SAMPLE_RATE
                    || platformFormat.getChannelCount()
                            != NativeLanMicrophoneSessionConfig.CHANNELS
                    || platformFormat.getEncoding() != REQUESTED_ENCODING) {
                failed = true;
                listener.onFailed(startGeneration, PROBLEM_UNSUPPORTED);
                return;
            }
            int bytesPerFrame = AndroidAudioFormats.bytesPerFrame(
                    platformFormat.getEncoding(),
                    platformFormat.getChannelCount());
            byte[] workBuffer = new byte[workBufferBytes];
            ActualAudioFormat actualFormat = new ActualAudioFormat(
                    platformFormat.getSampleRate(),
                    platformFormat.getChannelCount(),
                    AndroidAudioFormats.canonicalEncoding(platformFormat.getEncoding()),
                    Math.max(1, workBuffer.length / bytesPerFrame));

            NativeLanPacketQueue queue = new NativeLanPacketQueue(
                    config.binding, PACKET_QUEUE_CAPACITY, PACKET_QUEUE_BYTES);
            NativeLanUdpEndpoint endpoint = NativeLanUdpEndpoint.bind(
                    config.localAddress,
                    config.peerAddress,
                    config.binding,
                    SOCKET_TIMEOUT_MILLIS);
            NativeLanPcmPacketizer packetizer = new NativeLanPcmPacketizer(
                    config.binding,
                    queue,
                    NativeLanMicrophoneSessionConfig.SAMPLE_RATE,
                    NativeLanMicrophoneSessionConfig.CHANNELS,
                    NativeLanMicrophoneSessionConfig.BYTES_PER_SAMPLE,
                    NativeLanMicrophoneSessionConfig.FRAME_DURATION_MICROS,
                    0,
                    0,
                    System.nanoTime() / 1_000L);
            NativeLanSenderWorker sender = new NativeLanSenderWorker(
                    endpoint,
                    queue,
                    problem -> onMediaFailure(startGeneration, mediaFailed, problem));
            localSender = sender;
            this.packetizer = packetizer;
            packetQueue = queue;
            this.endpoint = endpoint;
            senderWorker = sender;
            sender.start();

            localRecorder.startRecording();
            if (localRecorder.getRecordingState() != AudioRecord.RECORDSTATE_RECORDING) {
                failed = true;
                listener.onFailed(startGeneration, PROBLEM_START);
                return;
            }
            listener.onStarted(startGeneration, actualFormat);
            android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_AUDIO);

            while (running.get()) {
                int bytesRead = localRecorder.read(
                        workBuffer,
                        0,
                        workBuffer.length,
                        AudioRecord.READ_BLOCKING);
                if (bytesRead > 0) {
                    packetizer.push(workBuffer, 0, bytesRead);
                    listener.onFrames(startGeneration, bytesRead / bytesPerFrame);
                } else if (bytesRead < 0 && running.get()) {
                    failed = true;
                    listener.onFailed(startGeneration, PROBLEM_READ);
                    break;
                }
            }
        } catch (IOException networkFailure) {
            failed = true;
            listener.onFailed(startGeneration, PROBLEM_NETWORK);
        } catch (SecurityException denied) {
            failed = true;
            listener.onFailed(startGeneration, PROBLEM_PERMISSION);
        } catch (IllegalArgumentException unsupported) {
            failed = true;
            listener.onFailed(startGeneration, PROBLEM_UNSUPPORTED);
        } catch (IllegalStateException startFailure) {
            failed = true;
            listener.onFailed(startGeneration, PROBLEM_START);
        } finally {
            running.set(false);
            if (localSender != null) {
                localSender.stop();
            }
            if (localRecorder != null) {
                try {
                    if (localRecorder.getRecordingState() == AudioRecord.RECORDSTATE_RECORDING) {
                        localRecorder.stop();
                    }
                } catch (IllegalStateException ignored) {
                    // Release remains mandatory after an asynchronous device failure.
                }
                localRecorder.release();
            }
            synchronized (this) {
                recorder = null;
                worker = null;
                if (senderWorker == localSender) {
                    senderWorker = null;
                    packetizer = null;
                    packetQueue = null;
                    endpoint = null;
                }
                activeGeneration = 0;
            }
            if (!failed && !mediaFailed.get()) {
                listener.onStopped(completionGeneration.get());
            }
        }
    }

    synchronized TransportMetrics transportMetrics() {
        NativeLanPcmPacketizer currentPacketizer = packetizer;
        NativeLanPacketQueue currentQueue = packetQueue;
        NativeLanUdpEndpoint currentEndpoint = endpoint;
        if (currentPacketizer == null || currentQueue == null || currentEndpoint == null) {
            return TransportMetrics.EMPTY;
        }
        NativeLanPcmPacketizer.Stats packetizerStats = currentPacketizer.stats();
        NativeLanPacketQueue.Stats queueStats = currentQueue.stats();
        NativeLanUdpEndpoint.Metrics endpointMetrics = currentEndpoint.metrics();
        return new TransportMetrics(
                packetizerStats.emittedPackets,
                endpointMetrics.packetsSent,
                endpointMetrics.datagramsSent,
                queueStats.fullPacketDrops + queueStats.fullByteDrops,
                packetizerStats.bufferedBytes);
    }

    static final class TransportMetrics {
        static final TransportMetrics EMPTY = new TransportMetrics(0, 0, 0, 0, 0);

        final long packetsEmitted;
        final long packetsSent;
        final long datagramsSent;
        final long packetsDropped;
        final int bufferedBytes;

        private TransportMetrics(
                long packetsEmitted,
                long packetsSent,
                long datagramsSent,
                long packetsDropped,
                int bufferedBytes) {
            this.packetsEmitted = packetsEmitted;
            this.packetsSent = packetsSent;
            this.datagramsSent = datagramsSent;
            this.packetsDropped = packetsDropped;
            this.bufferedBytes = bufferedBytes;
        }
    }

    private void onMediaFailure(
            long generation,
            AtomicBoolean mediaFailed,
            String problemCode) {
        synchronized (this) {
            if (activeGeneration != generation || worker == null) {
                return;
            }
            mediaFailed.set(true);
            running.set(false);
        }
        AudioRecord current = recorder;
        if (current != null) {
            try {
                current.stop();
            } catch (IllegalStateException ignored) {
                // The capture worker still owns release.
            }
        }
        listener.onFailed(generation, problemCode);
    }
}
