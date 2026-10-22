package dev.capyio.android;

/** Worker-confirmed actual write acceptance, never setup or queued-ACK evidence. */
final class AndroidSecureSpeakerProgress {
    private long bytes;
    private long records;
    void observe(int nativeState, long[] counters, long writtenBytes, long queuedRecords) {
        if (nativeState != 1 && nativeState != 4) return;
        if (counters == null || counters.length != 3 || counters[0] < 0 || counters[1] < 0
                || counters[2] < 0 || counters[2] % 4 != 0 || counters[1] > counters[0]
                || counters[1] > queuedRecords || counters[2] > writtenBytes
                || counters[1] < records || counters[2] < bytes) return;
        bytes = counters[2]; records = counters[1];
    }
    long acceptedBytes() { return bytes; }
    long acceptedRecords() { return records; }
}
