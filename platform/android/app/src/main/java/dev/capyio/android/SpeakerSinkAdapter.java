package dev.capyio.android;

import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioTrack;

import dev.capyio.android.contract.ActualAudioFormat;
import dev.capyio.android.lan.NativeLanPacketQueue;
import dev.capyio.android.lan.NativeLanPacketReassembler;
import dev.capyio.android.lan.NativeLanPcmSinkWorker;
import dev.capyio.android.lan.NativeLanReceiverWorker;
import dev.capyio.android.lan.NativeLanSpeakerSessionConfig;
import dev.capyio.android.lan.NativeLanUdpEndpoint;

import java.io.IOException;

/** Android speaker Sink backed by the trusted-lab native LAN receiver. */
final class SpeakerSinkAdapter implements AudioPlatformAdapter {
    static final String PROBLEM_BUSY = "CAPY.ANDROID.SPEAKER_BUSY";
    static final String PROBLEM_CONFIG = "CAPY.ANDROID.SPEAKER_CONFIG_MISSING";
    static final String PROBLEM_NETWORK = "CAPY.ANDROID.SPEAKER_NETWORK_FAILED";
    static final String PROBLEM_UNSUPPORTED = "CAPY.ANDROID.SPEAKER_FORMAT_UNSUPPORTED";
    static final String PROBLEM_START = "CAPY.ANDROID.SPEAKER_START_FAILED";

    private static final int REQUESTED_SAMPLE_RATE = 48_000;
    private static final int REQUESTED_CHANNEL_MASK = AudioFormat.CHANNEL_OUT_STEREO;
    private static final int REQUESTED_ENCODING = AudioFormat.ENCODING_PCM_16BIT;
    private static final int MAX_TRACK_BUFFER_BYTES = 1024 * 1024;
    private static final int PACKET_QUEUE_CAPACITY = 128;
    private static final int PACKET_QUEUE_BYTES = 512 * 1024;
    private static final int REASSEMBLY_CAPACITY = 8;
    private static final int SOCKET_TIMEOUT_MILLIS = 50;

    private final Listener listener;
    private final NativeLanSpeakerSessionConfig config;

    private volatile AudioTrack track;
    private NativeLanReceiverWorker receiverWorker;
    private NativeLanPcmSinkWorker sinkWorker;
    private long activeGeneration;

    SpeakerSinkAdapter(Listener listener, NativeLanSpeakerSessionConfig config) {
        this.listener = listener;
        this.config = config;
    }

    @Override
    public synchronized void start(long generation) {
        if (track != null) {
            listener.onFailed(generation, PROBLEM_BUSY);
            return;
        }
        if (config == null) {
            listener.onFailed(generation, PROBLEM_CONFIG);
            return;
        }

        AudioTrack candidate = null;
        try {
            int minimumBytes = AudioTrack.getMinBufferSize(
                    REQUESTED_SAMPLE_RATE,
                    REQUESTED_CHANNEL_MASK,
                    REQUESTED_ENCODING);
            if (minimumBytes <= 0 || minimumBytes > MAX_TRACK_BUFFER_BYTES) {
                listener.onFailed(generation, PROBLEM_UNSUPPORTED);
                return;
            }
            candidate = new AudioTrack.Builder()
                    .setAudioAttributes(new AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_MEDIA)
                            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                            .build())
                    .setAudioFormat(new AudioFormat.Builder()
                            .setSampleRate(REQUESTED_SAMPLE_RATE)
                            .setChannelMask(REQUESTED_CHANNEL_MASK)
                            .setEncoding(REQUESTED_ENCODING)
                            .build())
                    .setTransferMode(AudioTrack.MODE_STREAM)
                    .setBufferSizeInBytes(minimumBytes)
                    .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
                    .build();
            if (candidate.getState() != AudioTrack.STATE_INITIALIZED) {
                listener.onFailed(generation, PROBLEM_START);
                candidate.release();
                return;
            }

            AudioFormat platformFormat = candidate.getFormat();
            int bytesPerFrame = AndroidAudioFormats.bytesPerFrame(
                    platformFormat.getEncoding(),
                    platformFormat.getChannelCount());
            ActualAudioFormat actualFormat = new ActualAudioFormat(
                    platformFormat.getSampleRate(),
                    platformFormat.getChannelCount(),
                    AndroidAudioFormats.canonicalEncoding(platformFormat.getEncoding()),
                    Math.max(1, minimumBytes / bytesPerFrame));

            candidate.play();
            if (candidate.getPlayState() != AudioTrack.PLAYSTATE_PLAYING) {
                listener.onFailed(generation, PROBLEM_START);
                candidate.release();
                return;
            }

            NativeLanPacketQueue queue = new NativeLanPacketQueue(
                    config.binding, PACKET_QUEUE_CAPACITY, PACKET_QUEUE_BYTES);
            NativeLanUdpEndpoint endpoint = NativeLanUdpEndpoint.bind(
                    config.localAddress,
                    config.peerAddress,
                    config.binding,
                    SOCKET_TIMEOUT_MILLIS);
            NativeLanReceiverWorker receiver = new NativeLanReceiverWorker(
                    endpoint,
                    new NativeLanPacketReassembler(config.binding, REASSEMBLY_CAPACITY),
                    queue,
                    problem -> onMediaFailure(generation, problem));
            receiverWorker = receiver;
            AudioTrack renderTrack = candidate;
            NativeLanPcmSinkWorker writer = new NativeLanPcmSinkWorker(
                    queue,
                    bytesPerFrame,
                    new NativeLanPcmSinkWorker.PcmSink() {
                        @Override
                        public int write(byte[] payload, int offset, int length) {
                            return renderTrack.write(
                                    payload,
                                    offset,
                                    length,
                                    AudioTrack.WRITE_NON_BLOCKING);
                        }

                        @Override
                        public void reset() {
                            if (renderTrack.getPlayState() == AudioTrack.PLAYSTATE_PLAYING) {
                                renderTrack.pause();
                            }
                            renderTrack.flush();
                            renderTrack.play();
                        }
                    },
                    () -> android.os.Process.setThreadPriority(
                            android.os.Process.THREAD_PRIORITY_AUDIO),
                    frames -> listener.onFrames(generation, frames),
                    problem -> onMediaFailure(generation, problem));

            track = candidate;
            sinkWorker = writer;
            activeGeneration = generation;
            writer.start();
            receiver.start();
            listener.onStarted(generation, actualFormat);
        } catch (IOException networkFailure) {
            releaseAfterFailedStart(candidate);
            listener.onFailed(generation, PROBLEM_NETWORK);
        } catch (IllegalArgumentException unsupported) {
            releaseAfterFailedStart(candidate);
            listener.onFailed(generation, PROBLEM_UNSUPPORTED);
        } catch (IllegalStateException startFailure) {
            releaseAfterFailedStart(candidate);
            listener.onFailed(generation, PROBLEM_START);
        }
    }

    @Override
    public void stop(long generation) {
        AudioTrack current;
        NativeLanReceiverWorker receiver;
        NativeLanPcmSinkWorker writer;
        synchronized (this) {
            current = track;
            receiver = receiverWorker;
            writer = sinkWorker;
            track = null;
            receiverWorker = null;
            sinkWorker = null;
            activeGeneration = 0;
        }
        if (receiver != null) {
            receiver.stop();
        }
        if (writer != null) {
            writer.stop();
        }
        if (current != null) {
            releaseTrack(current);
        }
        listener.onStopped(generation);
    }

    int underrunCount() {
        AudioTrack current = track;
        return current == null ? 0 : current.getUnderrunCount();
    }

    synchronized TransportMetrics transportMetrics() {
        NativeLanReceiverWorker receiver = receiverWorker;
        if (receiver == null) {
            return TransportMetrics.EMPTY;
        }
        NativeLanReceiverWorker.Stats metrics = receiver.stats();
        return new TransportMetrics(
                metrics.datagramsReceived,
                metrics.wrongPeerDatagrams,
                metrics.malformedDatagrams,
                metrics.completedPackets,
                metrics.partialEvictions,
                metrics.fullPacketDrops,
                metrics.fullByteDrops);
    }

    private void onMediaFailure(long generation, String problemCode) {
        synchronized (this) {
            if (activeGeneration != generation || track == null) {
                return;
            }
        }
        listener.onFailed(generation, problemCode);
    }

    private void releaseAfterFailedStart(AudioTrack candidate) {
        NativeLanReceiverWorker receiver = receiverWorker;
        NativeLanPcmSinkWorker writer = sinkWorker;
        receiverWorker = null;
        sinkWorker = null;
        track = null;
        activeGeneration = 0;
        if (receiver != null) {
            receiver.stop();
        }
        if (writer != null) {
            writer.stop();
        }
        if (candidate != null) {
            releaseTrack(candidate);
        }
    }

    private static void releaseTrack(AudioTrack current) {
        try {
            if (current.getPlayState() == AudioTrack.PLAYSTATE_PLAYING) {
                current.pause();
            }
            current.flush();
            current.stop();
        } catch (IllegalStateException ignored) {
            // Release is still safe and mandatory after a route/device change.
        } finally {
            current.release();
        }
    }

    static final class TransportMetrics {
        static final TransportMetrics EMPTY = new TransportMetrics(0, 0, 0, 0, 0, 0, 0);

        final long datagramsReceived;
        final long wrongPeerDatagrams;
        final long malformedDatagrams;
        final long completedPackets;
        final long partialEvictions;
        final long fullPacketDrops;
        final long fullByteDrops;

        TransportMetrics(
                long datagramsReceived,
                long wrongPeerDatagrams,
                long malformedDatagrams,
                long completedPackets,
                long partialEvictions,
                long fullPacketDrops,
                long fullByteDrops) {
            this.datagramsReceived = datagramsReceived;
            this.wrongPeerDatagrams = wrongPeerDatagrams;
            this.malformedDatagrams = malformedDatagrams;
            this.completedPackets = completedPackets;
            this.partialEvictions = partialEvictions;
            this.fullPacketDrops = fullPacketDrops;
            this.fullByteDrops = fullByteDrops;
        }
    }
}
