package dev.capyio.touchpad.lab

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class TouchpadInputDiagnosticsTest {
    private fun contact(
        id: Int,
        pressure: Float = 0.5f,
        major: Float = 20f,
        minor: Float = 15f,
    ) = TouchpadContactDiagnostic(id, pressure, major, minor)

    @Test
    fun accumulatesPressureAndContactSizeWithoutChangingInput() {
        val diagnostics = TouchpadInputDiagnostics()
        diagnostics.observe(listOf(contact(7, pressure = 0.2f, major = 10f, minor = 8f)))
        diagnostics.observe(
            listOf(
                contact(7, pressure = 0.8f, major = 30f, minor = 18f),
                contact(9, pressure = 0.4f, major = 25f, minor = 20f),
            ),
        )

        assertEquals(
            TouchpadInputDiagnosticSnapshot(
                sampleCount = 3,
                minimumPressure = 0.2f,
                maximumPressure = 0.8f,
                maximumTouchMajorPx = 30f,
                maximumTouchMinorPx = 20f,
                fiveFingerFrames = 0,
                fiveFingerGestures = 0,
            ),
            diagnostics.snapshot(),
        )
    }

    @Test
    fun countsOneFiveFingerReachPerCompleteGesture() {
        val diagnostics = TouchpadInputDiagnostics()
        val five = (0 until 5).map(::contact)
        diagnostics.observe(five)
        diagnostics.observe(five)
        diagnostics.observe(five.take(4))
        diagnostics.observe(five)
        diagnostics.observe(emptyList())
        diagnostics.observe(five)

        val snapshot = diagnostics.snapshot()
        assertEquals(4, snapshot.fiveFingerFrames)
        assertEquals(2, snapshot.fiveFingerGestures)
    }

    @Test
    fun rejectsInvalidOrAmbiguousSamplesTransactionally() {
        val diagnostics = TouchpadInputDiagnostics()
        assertThrows(IllegalArgumentException::class.java) {
            diagnostics.observe(listOf(contact(1), contact(1)))
        }
        assertThrows(IllegalArgumentException::class.java) {
            diagnostics.observe(listOf(contact(1, pressure = Float.NaN)))
        }
        assertEquals(0, diagnostics.snapshot().sampleCount)
    }
}
