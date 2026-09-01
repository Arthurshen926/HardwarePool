package dev.capyio.touchpad.lab

import kotlin.math.max
import kotlin.math.min

internal data class TouchpadContactDiagnostic(
    val contactId: Int,
    val pressure: Float,
    val touchMajorPx: Float,
    val touchMinorPx: Float,
)

internal data class TouchpadInputDiagnosticSnapshot(
    val sampleCount: Long,
    val minimumPressure: Float?,
    val maximumPressure: Float?,
    val maximumTouchMajorPx: Float,
    val maximumTouchMinorPx: Float,
    val fiveFingerFrames: Long,
    val fiveFingerGestures: Long,
)

internal class TouchpadInputDiagnostics {
    private var sampleCount = 0L
    private var minimumPressure: Float? = null
    private var maximumPressure: Float? = null
    private var maximumTouchMajorPx = 0f
    private var maximumTouchMinorPx = 0f
    private var fiveFingerFrames = 0L
    private var fiveFingerGestures = 0L
    private var currentGestureReachedFive = false

    fun observe(contacts: List<TouchpadContactDiagnostic>) {
        require(contacts.size <= 5) { "touchpad diagnostics accept at most five contacts" }
        require(contacts.map { it.contactId }.distinct().size == contacts.size) {
            "touchpad diagnostic contact IDs must be unique"
        }
        contacts.forEach { contact ->
            require(contact.pressure.isFinite() && contact.pressure >= 0f) {
                "pressure must be finite and non-negative"
            }
            require(contact.touchMajorPx.isFinite() && contact.touchMajorPx >= 0f) {
                "touch major must be finite and non-negative"
            }
            require(contact.touchMinorPx.isFinite() && contact.touchMinorPx >= 0f) {
                "touch minor must be finite and non-negative"
            }
            sampleCount += 1
            minimumPressure = minimumPressure?.let { min(it, contact.pressure) }
                ?: contact.pressure
            maximumPressure = maximumPressure?.let { max(it, contact.pressure) }
                ?: contact.pressure
            maximumTouchMajorPx = max(maximumTouchMajorPx, contact.touchMajorPx)
            maximumTouchMinorPx = max(maximumTouchMinorPx, contact.touchMinorPx)
        }
        if (contacts.size == 5) {
            fiveFingerFrames += 1
            if (!currentGestureReachedFive) {
                fiveFingerGestures += 1
                currentGestureReachedFive = true
            }
        }
        if (contacts.isEmpty()) currentGestureReachedFive = false
    }

    fun snapshot() = TouchpadInputDiagnosticSnapshot(
        sampleCount = sampleCount,
        minimumPressure = minimumPressure,
        maximumPressure = maximumPressure,
        maximumTouchMajorPx = maximumTouchMajorPx,
        maximumTouchMinorPx = maximumTouchMinorPx,
        fiveFingerFrames = fiveFingerFrames,
        fiveFingerGestures = fiveFingerGestures,
    )
}
