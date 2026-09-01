package dev.capyio.touchpad.lab

import kotlin.math.hypot

internal class TapDragFeedbackTracker(
    private val tapMaxDurationMillis: Long,
    private val doubleTapMaxGapMillis: Long,
    private val tapSlopPx: Float,
    private val doubleTapSlopPx: Float,
) {
    private data class Tap(val releasedAtMillis: Long, val x: Float, val y: Float)

    private var priorTap: Tap? = null
    private var downAtMillis = 0L
    private var downX = 0f
    private var downY = 0f
    private var active = false
    private var secondTapCandidate = false
    private var movedBeyondTapSlop = false
    private var dragStarted = false

    fun onDown(eventTimeMillis: Long, x: Float, y: Float) {
        val prior = priorTap
        secondTapCandidate = prior != null &&
            eventTimeMillis >= prior.releasedAtMillis &&
            eventTimeMillis - prior.releasedAtMillis <= doubleTapMaxGapMillis &&
            distance(prior.x, prior.y, x, y) <= doubleTapSlopPx
        downAtMillis = eventTimeMillis
        downX = x
        downY = y
        active = true
        movedBeyondTapSlop = false
        dragStarted = false
    }

    fun onMove(x: Float, y: Float): Boolean {
        if (!active) return false
        movedBeyondTapSlop = movedBeyondTapSlop || distance(downX, downY, x, y) > tapSlopPx
        if (!secondTapCandidate || !movedBeyondTapSlop || dragStarted) return false
        dragStarted = true
        priorTap = null
        return true
    }

    fun onUp(eventTimeMillis: Long, x: Float, y: Float): Boolean {
        if (!active) return false
        movedBeyondTapSlop = movedBeyondTapSlop || distance(downX, downY, x, y) > tapSlopPx
        val duration = eventTimeMillis - downAtMillis
        val isTap =
            !dragStarted &&
            duration in 0..tapMaxDurationMillis &&
            !movedBeyondTapSlop
        priorTap = if (isTap) {
            Tap(eventTimeMillis, x, y)
        } else {
            null
        }
        resetContact()
        return isTap
    }

    fun cancel() {
        priorTap = null
        resetContact()
    }

    private fun resetContact() {
        active = false
        secondTapCandidate = false
        movedBeyondTapSlop = false
        dragStarted = false
    }

    private fun distance(ax: Float, ay: Float, bx: Float, by: Float): Float =
        hypot(bx - ax, by - ay)
}
