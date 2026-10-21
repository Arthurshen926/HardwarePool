package io.capyio.camera.lab;

import io.capyio.camera.contract.AvcCodecConfig;
import io.capyio.camera.contract.EncodedAvcAccessUnit;

/** Transport choice is explicit; secure routes never instantiate the lab socket exporter. */
interface AvcSender extends AutoCloseable {
    void start();
    void setCodecConfig(AvcCodecConfig config);
    void offer(EncodedAvcAccessUnit unit);
    @Override void close();
}
