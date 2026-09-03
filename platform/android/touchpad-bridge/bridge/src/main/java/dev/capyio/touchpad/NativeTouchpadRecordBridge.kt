package dev.capyio.touchpad

import android.view.MotionEvent

object NativeTouchpadRecordBridge {
    init {
        System.loadLibrary("capyio_android_jni")
    }

    external fun nativeRecordCreate(
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
        routeId: String,
        sessionId: String,
        sourceNodeId: String,
        sourceCapabilityId: String,
        sourcePortId: String,
        sinkNodeId: String,
        sinkCapabilityId: String,
        sinkPortId: String,
        authorizationExpiresAtMs: Long,
    ): Long

    external fun nativeRecordHello(handle: Long): ByteArray
    external fun nativeRecordStart(handle: Long, eventTimeNanos: Long): ByteArray
    external fun nativeRecordMotion(
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
    external fun nativeRecordStop(handle: Long, eventTimeNanos: Long): ByteArray?
    external fun nativeRecordClose(handle: Long, eventTimeNanos: Long): ByteArray?
    external fun nativeRecordTakeClose(handle: Long): ByteArray
}

data class TouchpadRouteConfigV1(
    val routeId: String,
    val sessionId: String,
    val sourceNodeId: String,
    val sourceCapabilityId: String,
    val sourcePortId: String,
    val sinkNodeId: String,
    val sinkCapabilityId: String,
    val sinkPortId: String,
    val authorizationExpiresAtMs: Long = -1,
)

class TouchpadRecordSessionV1(
    packet: TouchpadPacketSessionConfigV1,
    route: TouchpadRouteConfigV1,
) {
    private val ownerThread = Thread.currentThread()
    private val widthPx = packet.widthPx
    private val heightPx = packet.heightPx
    private val reportsPressure = packet.reportsPressure
    private var handle = NativeTouchpadRecordBridge.nativeRecordCreate(
        packet.streamId,
        packet.streamEpoch,
        packet.clockDomainId,
        packet.widthPx,
        packet.heightPx,
        packet.widthHimetric,
        packet.heightHimetric,
        packet.maxContacts,
        packet.reportsPressure,
        packet.firstSequence,
        route.routeId,
        route.sessionId,
        route.sourceNodeId,
        route.sourceCapabilityId,
        route.sourcePortId,
        route.sinkNodeId,
        route.sinkCapabilityId,
        route.sinkPortId,
        route.authorizationExpiresAtMs,
    )

    fun hello(): ByteArray = NativeTouchpadRecordBridge.nativeRecordHello(requireHandle())

    fun start(eventTimeNanos: Long): ByteArray {
        checkOwnerThread()
        return NativeTouchpadRecordBridge.nativeRecordStart(requireHandle(), eventTimeNanos)
    }

    fun onMotionEvent(event: MotionEvent): ByteArray {
        checkOwnerThread()
        val cancel = event.actionMasked == MotionEvent.ACTION_CANCEL
        val count = if (cancel) 0 else event.pointerCount
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
        return NativeTouchpadRecordBridge.nativeRecordMotion(
            requireHandle(), event.eventTimeNanos, event.actionMasked,
            if (cancel) 0 else event.actionIndex, ids, tools, x, y, pressure,
        )
    }

    fun close(eventTimeNanos: Long): List<ByteArray> {
        checkOwnerThread()
        val active = requireHandle()
        val records = buildList {
            NativeTouchpadRecordBridge.nativeRecordClose(active, eventTimeNanos)?.let(::add)
            add(NativeTouchpadRecordBridge.nativeRecordTakeClose(active))
        }
        handle = 0
        return records
    }

    private fun requireHandle(): Long = checkNotNull(handle.takeIf { it != 0L }) {
        "touchpad record session is closed"
    }

    private fun checkOwnerThread() {
        check(Thread.currentThread() === ownerThread) {
            "touchpad record session must remain on its creating thread"
        }
    }
}
