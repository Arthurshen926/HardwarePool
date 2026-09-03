package dev.capyio.touchpad

import android.view.MotionEvent

/** Primitive-only JNI contract. Private packets are handed back to the owning Android host. */
object NativeTouchpadBridge {
    const val CONTRACT_VERSION: Int = 1

    init {
        System.loadLibrary("capyio_android_jni")
    }

    external fun nativeCreate(
        streamId: String,
        streamEpoch: Long,
        clockDomainId: String,
        widthPx: Int,
        heightPx: Int,
        widthHimetric: Int,
        heightHimetric: Int,
        maxContacts: Int,
        reportsPressure: Boolean,
        firstSequence: Long,
    ): Long

    external fun nativeStart(handle: Long, eventTimeNanos: Long): ByteArray

    external fun nativeMotion(
        handle: Long,
        eventTimeNanos: Long,
        action: Int,
        actionIndex: Int,
        pointerIds: IntArray,
        toolTypes: IntArray,
        xPx: FloatArray,
        yPx: FloatArray,
        pressure: FloatArray,
    ): ByteArray

    external fun nativeStop(handle: Long, eventTimeNanos: Long): ByteArray?

    external fun nativeClose(handle: Long, eventTimeNanos: Long): ByteArray?
}

data class TouchpadPacketSessionConfigV1(
    val streamId: String,
    val streamEpoch: Long,
    val widthPx: Int,
    val heightPx: Int,
    val widthHimetric: Int,
    val heightHimetric: Int,
    val maxContacts: Int = 5,
    val reportsPressure: Boolean = true,
    val firstSequence: Long = 0,
    val clockDomainId: String = "android.elapsed-realtime-nanos",
)

/**
 * UI-thread-owned MotionEvent adapter. It performs no transport, gesture recognition, or I/O.
 */
class TouchpadPacketSessionV1(config: TouchpadPacketSessionConfigV1) {
    private val ownerThread = Thread.currentThread()
    private val widthPx = config.widthPx
    private val heightPx = config.heightPx
    private val reportsPressure = config.reportsPressure
    private var handle = NativeTouchpadBridge.nativeCreate(
        config.streamId,
        config.streamEpoch,
        config.clockDomainId,
        config.widthPx,
        config.heightPx,
        config.widthHimetric,
        config.heightHimetric,
        config.maxContacts,
        config.reportsPressure,
        config.firstSequence,
    )

    fun start(eventTimeNanos: Long): ByteArray {
        checkOwnerThread()
        return NativeTouchpadBridge.nativeStart(requireHandle(), eventTimeNanos)
    }

    fun onMotionEvent(event: MotionEvent): ByteArray {
        checkOwnerThread()
        val isCancel = event.actionMasked == MotionEvent.ACTION_CANCEL
        val count = if (isCancel) 0 else event.pointerCount
        val ids = IntArray(count)
        val tools = IntArray(count)
        val x = FloatArray(count)
        val y = FloatArray(count)
        val pressure = FloatArray(count)
        for (index in 0 until count) {
            ids[index] = event.getPointerId(index)
            tools[index] = event.getToolType(index)
            x[index] = event.getX(index).coerceIn(0f, widthPx.toFloat())
            y[index] = event.getY(index).coerceIn(0f, heightPx.toFloat())
            pressure[index] = if (reportsPressure) event.getPressure(index) else -1f
        }
        return NativeTouchpadBridge.nativeMotion(
            requireHandle(),
            event.eventTimeNanos,
            event.actionMasked,
            if (isCancel) 0 else event.actionIndex,
            ids,
            tools,
            x,
            y,
            pressure,
        )
    }

    fun stop(eventTimeNanos: Long): ByteArray? {
        checkOwnerThread()
        return NativeTouchpadBridge.nativeStop(requireHandle(), eventTimeNanos)
    }

    fun close(eventTimeNanos: Long): ByteArray? {
        checkOwnerThread()
        val activeHandle = requireHandle()
        val packet = NativeTouchpadBridge.nativeClose(activeHandle, eventTimeNanos)
        handle = 0
        return packet
    }

    private fun requireHandle(): Long = checkNotNull(handle.takeIf { it != 0L }) {
        "touchpad packet session is closed"
    }

    private fun checkOwnerThread() {
        check(Thread.currentThread() === ownerThread) {
            "touchpad packet session must remain on its creating thread"
        }
    }
}
