package dev.capyio.touchpad.lab

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TapDragFeedbackTrackerTest {
    private fun tracker() = TapDragFeedbackTracker(
        tapMaxDurationMillis = 300,
        doubleTapMaxGapMillis = 500,
        tapSlopPx = 8f,
        doubleTapSlopPx = 80f,
    )

    @Test
    fun fastSecondContactStartsDragAfterCrossingSlop() {
        val tracker = tracker()
        tracker.onDown(0, 100f, 100f)
        assertTrue(tracker.onUp(60, 100f, 100f))
        tracker.onDown(61, 102f, 100f)

        assertFalse(tracker.onMove(108f, 100f))
        assertTrue(tracker.onMove(111f, 100f))
        assertFalse(tracker.onMove(140f, 100f))
    }

    @Test
    fun lateOrDistantSecondContactDoesNotStartDrag() {
        val late = tracker()
        late.onDown(0, 100f, 100f)
        assertTrue(late.onUp(60, 100f, 100f))
        late.onDown(561, 100f, 100f)
        assertFalse(late.onMove(140f, 100f))

        val distant = tracker()
        distant.onDown(0, 100f, 100f)
        assertTrue(distant.onUp(60, 100f, 100f))
        distant.onDown(80, 200f, 100f)
        assertFalse(distant.onMove(240f, 100f))
    }

    @Test
    fun movedFirstContactAndMultitouchCancellationRejectCandidate() {
        val moved = tracker()
        moved.onDown(0, 100f, 100f)
        moved.onMove(120f, 100f)
        assertFalse(moved.onUp(60, 120f, 100f))
        moved.onDown(80, 120f, 100f)
        assertFalse(moved.onMove(140f, 100f))

        val cancelled = tracker()
        cancelled.onDown(0, 100f, 100f)
        assertTrue(cancelled.onUp(60, 100f, 100f))
        cancelled.cancel()
        cancelled.onDown(80, 100f, 100f)
        assertFalse(cancelled.onMove(140f, 100f))
    }

    @Test
    fun ordinaryDoubleTapReportsTwoTapFeedbackEvents() {
        val tracker = tracker()
        tracker.onDown(0, 100f, 100f)
        assertTrue(tracker.onUp(60, 100f, 100f))
        tracker.onDown(100, 102f, 100f)
        assertTrue(tracker.onUp(160, 102f, 100f))
    }
}
