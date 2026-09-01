package dev.capyio.touchpad.lab

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TouchpadHapticsTest {
    @Test
    fun strengthCyclesAndInvalidPreferenceFallsBackToMedium() {
        assertEquals(TouchpadHapticStrength.Medium, TouchpadHapticStrength.fromPreference(-1))
        assertEquals(TouchpadHapticStrength.Medium, TouchpadHapticStrength.fromPreference(99))
        assertEquals(TouchpadHapticStrength.Medium, TouchpadHapticStrength.Weak.next())
        assertEquals(TouchpadHapticStrength.Strong, TouchpadHapticStrength.Medium.next())
        assertEquals(TouchpadHapticStrength.Weak, TouchpadHapticStrength.Strong.next())
    }

    @Test
    fun everyStrengthUsesBoundedAmplitudeAndStrongerDragFeedback() {
        TouchpadHapticStrength.entries.forEach { strength ->
            val tap = strength.amplitude(TouchpadHapticEffect.Tap)
            val drag = strength.amplitude(TouchpadHapticEffect.Drag)
            assertTrue(tap in 1..255)
            assertTrue(drag in 1..255)
            assertTrue(drag >= tap)
            assertTrue(strength.scale(TouchpadHapticEffect.Tap) in 0f..1f)
            assertTrue(strength.scale(TouchpadHapticEffect.Drag) in 0f..1f)
        }
    }
}
