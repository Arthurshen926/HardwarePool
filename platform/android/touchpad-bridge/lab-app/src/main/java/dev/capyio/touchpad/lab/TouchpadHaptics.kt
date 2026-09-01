package dev.capyio.touchpad.lab

internal enum class TouchpadHapticEffect {
    Tap,
    Drag,
}

internal enum class TouchpadHapticStrength(
    val label: String,
    private val tapScale: Float,
    private val dragScale: Float,
    private val tapAmplitude: Int,
    private val dragAmplitude: Int,
) {
    Weak("弱", 0.25f, 0.40f, 64, 96),
    Medium("中", 0.50f, 0.70f, 128, 180),
    Strong("强", 0.75f, 1.00f, 208, 255),
    ;

    fun next(): TouchpadHapticStrength = entries[(ordinal + 1) % entries.size]

    fun scale(effect: TouchpadHapticEffect): Float = when (effect) {
        TouchpadHapticEffect.Tap -> tapScale
        TouchpadHapticEffect.Drag -> dragScale
    }

    fun amplitude(effect: TouchpadHapticEffect): Int = when (effect) {
        TouchpadHapticEffect.Tap -> tapAmplitude
        TouchpadHapticEffect.Drag -> dragAmplitude
    }

    companion object {
        fun fromPreference(value: Int): TouchpadHapticStrength =
            entries.getOrElse(value) { Medium }
    }
}
