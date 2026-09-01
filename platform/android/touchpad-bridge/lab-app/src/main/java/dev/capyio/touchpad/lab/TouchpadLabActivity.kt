package dev.capyio.touchpad.lab

import android.app.Activity
import android.app.AlertDialog
import android.content.Intent
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.provider.Settings
import android.util.Log
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.Switch
import dev.capyio.touchpad.TouchpadPacketSessionConfigV1
import dev.capyio.touchpad.TouchpadRecordSessionV1
import dev.capyio.touchpad.TouchpadRouteConfigV1
import java.io.DataInputStream
import java.io.DataOutputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.net.ConnectException
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.Locale
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.atomic.AtomicBoolean

private const val LAB_PORT = 61000
private const val ROUTE_EPOCH = 1L
private const val MAX_QUEUE_RECORDS = 64
private const val MAX_PENDING_MOTION_RECORDS = 4
private const val MIN_MOVE_INTERVAL_MILLIS = 16L
private const val ADDED_CONTACT_SETTLE_MILLIS = 72L
private const val MULTI_FINGER_MOTION_SCALE_PERCENT = 70
private const val TAP_DRAG_MAX_TAP_DURATION_MILLIS = 300L
private const val HAPTICS_PREFERENCES = "touchpad-haptics"
private const val HAPTICS_ENABLED_KEY = "enabled"
private const val HAPTICS_STRENGTH_KEY = "strength"
private const val VIVO_SMARTSHOT_SETTINGS_ACTION =
    "com.vivo.smartshot.ui.SettingMenuActivity"
private const val VIVO_SMARTSHOT_PACKAGE = "com.vivo.smartshot"

class TouchpadLabActivity : Activity() {
    private lateinit var touchpad: TouchpadLabView
    private lateinit var vibrator: Vibrator
    private var touchpadHapticsEnabled = true
    private var touchpadHapticStrength = TouchpadHapticStrength.Medium

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        vibrator = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            getSystemService(VibratorManager::class.java).defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            getSystemService(Vibrator::class.java)
        }
        val preferences = getSharedPreferences(HAPTICS_PREFERENCES, MODE_PRIVATE)
        touchpadHapticsEnabled = preferences.getBoolean(HAPTICS_ENABLED_KEY, true)
        touchpadHapticStrength = TouchpadHapticStrength.fromPreference(
            preferences.getInt(HAPTICS_STRENGTH_KEY, TouchpadHapticStrength.Medium.ordinal),
        )
        touchpad = TouchpadLabView()
        val root = FrameLayout(this).apply {
            addView(
                touchpad,
                FrameLayout.LayoutParams(
                    FrameLayout.LayoutParams.MATCH_PARENT,
                    FrameLayout.LayoutParams.MATCH_PARENT,
                ),
            )
            addView(
                LinearLayout(this@TouchpadLabActivity).apply {
                    orientation = LinearLayout.VERTICAL
                    gravity = Gravity.END
                    addView(Switch(this@TouchpadLabActivity).apply {
                        text = "独立触控板振动"
                        textSize = 18f
                        setTextColor(Color.rgb(16, 20, 26))
                        setPadding(20, 10, 20, 10)
                        isChecked = touchpadHapticsEnabled
                        setOnCheckedChangeListener { _, enabled ->
                            touchpadHapticsEnabled = enabled
                            preferences.edit().putBoolean(HAPTICS_ENABLED_KEY, enabled).apply()
                            touchpad.invalidate()
                            if (enabled) vibrateTouchpad(TouchpadHapticEffect.Tap)
                        }
                    })
                    addView(Button(this@TouchpadLabActivity).apply {
                        text = "振动强度：${touchpadHapticStrength.label}"
                        setOnClickListener {
                            touchpadHapticStrength = touchpadHapticStrength.next()
                            preferences.edit()
                                .putInt(HAPTICS_STRENGTH_KEY, touchpadHapticStrength.ordinal)
                                .apply()
                            text = "振动强度：${touchpadHapticStrength.label}"
                            touchpad.invalidate()
                            vibrateTouchpad(TouchpadHapticEffect.Tap)
                        }
                    })
                },
                FrameLayout.LayoutParams(
                    FrameLayout.LayoutParams.WRAP_CONTENT,
                    FrameLayout.LayoutParams.WRAP_CONTENT,
                ).apply {
                    gravity = Gravity.TOP or Gravity.END
                    setMargins(24, 24, 36, 24)
                },
            )
        }
        setContentView(root)
        applyTouchpadWindowMode()
    }

    private fun vibrateTouchpad(effect: TouchpadHapticEffect): Boolean {
        if (!touchpadHapticsEnabled || !vibrator.hasVibrator()) return false
        val vibration = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val primitive = when (effect) {
                TouchpadHapticEffect.Tap -> VibrationEffect.Composition.PRIMITIVE_TICK
                TouchpadHapticEffect.Drag -> VibrationEffect.Composition.PRIMITIVE_CLICK
            }
            if (vibrator.arePrimitivesSupported(primitive).single()) {
                VibrationEffect.startComposition()
                    .addPrimitive(primitive, touchpadHapticStrength.scale(effect))
                    .compose()
            } else {
                VibrationEffect.createOneShot(16L, touchpadHapticStrength.amplitude(effect))
            }
        } else {
            VibrationEffect.createOneShot(16L, touchpadHapticStrength.amplitude(effect))
        }
        vibrator.vibrate(vibration)
        return true
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) applyTouchpadWindowMode()
    }

    override fun onDestroy() {
        touchpad.shutdown()
        super.onDestroy()
    }

    override fun onStop() {
        touchpad.shutdown()
        super.onStop()
        if (!isFinishing) finish()
    }

    @Suppress("DEPRECATION")
    private fun applyTouchpadWindowMode() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.setDecorFitsSystemWindows(false)
            window.insetsController?.let { controller ->
                controller.hide(WindowInsets.Type.systemBars())
                controller.systemBarsBehavior =
                    WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            }
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility =
                View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY or
                    View.SYSTEM_UI_FLAG_FULLSCREEN or
                    View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
                    View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                    View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or
                    View.SYSTEM_UI_FLAG_LAYOUT_STABLE
        }
    }

    private inner class TouchpadLabView : View(this@TouchpadLabActivity) {
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        private val records = ArrayBlockingQueue<ByteArray>(MAX_QUEUE_RECORDS)
        private val running = AtomicBoolean(true)
        private val connected = AtomicBoolean(false)
        private var session: TouchpadRecordSessionV1? = null
        private var status = "等待触控区域初始化…"
        private var acknowledged = 0L
        private var contacts: List<Pair<Float, Float>> = emptyList()
        private var contactDiagnostics: List<TouchpadContactDiagnostic> = emptyList()
        private val inputDiagnostics = TouchpadInputDiagnostics()
        private var currentContactCount = 0
        private var maxContactCount = 0
        private var probableSystemGestureCancellationCount = 0
        private var tapHapticCount = 0
        private var tapDragHapticCount = 0
        private var gestureConflictDialogShown = false
        private var lastMoveEnqueuedMillis = -MIN_MOVE_INTERVAL_MILLIS
        private var suppressMovesUntilMillis = 0L
        private var sender: Thread? = null
        private val tapDragFeedbackTracker = ViewConfiguration.get(context).let { configuration ->
            TapDragFeedbackTracker(
                tapMaxDurationMillis = TAP_DRAG_MAX_TAP_DURATION_MILLIS,
                doubleTapMaxGapMillis = ViewConfiguration.getDoubleTapTimeout().toLong(),
                tapSlopPx = configuration.scaledTouchSlop.toFloat(),
                doubleTapSlopPx = configuration.scaledDoubleTapSlop.toFloat(),
            )
        }

        override fun onSizeChanged(width: Int, height: Int, oldWidth: Int, oldHeight: Int) {
            super.onSizeChanged(width, height, oldWidth, oldHeight)
            if (width > 0 && height > 0) {
                systemGestureExclusionRects = listOf(Rect(0, 0, width, height))
            }
            if (width <= 0 || height <= 0 || session != null) return
            try {
                val created = TouchpadRecordSessionV1(
                    TouchpadPacketSessionConfigV1(
                        streamId = "00000000-0000-4000-8000-00000000f109",
                        streamEpoch = ROUTE_EPOCH,
                        widthPx = width,
                        heightPx = height,
                        widthHimetric = 12_000,
                        heightHimetric = 7_000,
                        maxContacts = 5,
                        reportsPressure = true,
                        firstSequence = 0,
                        clockDomainId = "android.uptime-nanos",
                    ),
                    TouchpadRouteConfigV1(
                        routeId = "00000000-0000-4000-8000-00000000f101",
                        sessionId = "00000000-0000-4000-8000-00000000f102",
                        sourceNodeId = "00000000-0000-4000-8000-00000000f103",
                        sourceCapabilityId = "00000000-0000-4000-8000-00000000f104",
                        sourcePortId = "00000000-0000-4000-8000-00000000f105",
                        sinkNodeId = "00000000-0000-4000-8000-00000000f106",
                        sinkCapabilityId = "00000000-0000-4000-8000-00000000f107",
                        sinkPortId = "00000000-0000-4000-8000-00000000f108",
                    ),
                )
                session = created
                val hello = created.hello()
                enqueue(hello)
                Log.i("CapyIO-PTP", "hello queued bytes=${hello.size}")
                val start = created.start(motionClockNanos())
                enqueue(start)
                Log.i("CapyIO-PTP", "initial cancellation queued bytes=${start.size}")
                status = "已初始化，等待 Windows 回环接收器…"
                startSender()
            } catch (error: Throwable) {
                fault("初始化失败: ${error.message}")
            }
            invalidate()
        }

        override fun onTouchEvent(event: MotionEvent): Boolean {
            parent?.requestDisallowInterceptTouchEvent(
                event.actionMasked != MotionEvent.ACTION_UP &&
                    event.actionMasked != MotionEvent.ACTION_CANCEL,
            )
            contacts = visibleContacts(event)
            contactDiagnostics = visibleContactDiagnostics(event)
            runCatching { inputDiagnostics.observe(contactDiagnostics) }
                .onFailure { error ->
                    Log.w("CapyIO-PTP", "diagnostic sample ignored: ${error.message}")
                }
            val previousContactCount = currentContactCount
            currentContactCount = contacts.size
            maxContactCount = maxOf(maxContactCount, currentContactCount)
            if (contactDiagnostics.size == 5 && previousContactCount < 5) {
                val snapshot = inputDiagnostics.snapshot()
                Log.i(
                    "CapyIO-PTP",
                    "five contacts reached: gestures=${snapshot.fiveFingerGestures} " +
                        "frames=${snapshot.fiveFingerFrames}",
                )
            }
            if (
                currentContactCount != previousContactCount ||
                event.actionMasked != MotionEvent.ACTION_MOVE
            ) {
                Log.i(
                    "CapyIO-PTP",
                    "touch action=${event.actionMasked} raw=${event.pointerCount} " +
                        "effective=$currentContactCount max=$maxContactCount",
                )
            }
            invalidate()
            if (event.actionMasked == MotionEvent.ACTION_UP) performClick()

            updateTapDragFeedback(event)

            if (
                event.actionMasked == MotionEvent.ACTION_CANCEL &&
                maxOf(previousContactCount, event.pointerCount) >= 3
            ) {
                probableSystemGestureCancellationCount += 1
                Log.w(
                    "CapyIO-PTP",
                    "probable system gesture interception: contacts=" +
                        "${maxOf(previousContactCount, event.pointerCount)} " +
                        "count=$probableSystemGestureCancellationCount",
                )
                postStatus(
                    "检测到系统抢占三/四指触摸；请关闭手机的三指系统手势",
                )
                if (!gestureConflictDialogShown) {
                    gestureConflictDialogShown = true
                    showGestureConflictDialog()
                }
            }

            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    suppressMovesUntilMillis = 0L
                    lastMoveEnqueuedMillis = event.eventTime - MIN_MOVE_INTERVAL_MILLIS
                }
                MotionEvent.ACTION_POINTER_DOWN -> {
                    suppressMovesUntilMillis = maxOf(
                        suppressMovesUntilMillis,
                        event.eventTime + ADDED_CONTACT_SETTLE_MILLIS,
                    )
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    suppressMovesUntilMillis = 0L
                }
            }

            val active = session
            if (active == null || !running.get()) return true
            if (!connected.get()) {
                if (event.actionMasked == MotionEvent.ACTION_DOWN) {
                    postStatus("触摸已捕获；等待 Windows 接收器连接")
                }
                return true
            }
            if (event.actionMasked == MotionEvent.ACTION_MOVE) {
                if (event.eventTime < suppressMovesUntilMillis) return true
                val elapsed = event.eventTime - lastMoveEnqueuedMillis
                if (
                    records.size >= MAX_PENDING_MOTION_RECORDS ||
                    elapsed in 0 until MIN_MOVE_INTERVAL_MILLIS
                ) {
                    return true
                }
            }
            return try {
                enqueue(active.onMotionEvent(event))
                if (event.actionMasked == MotionEvent.ACTION_MOVE) {
                    lastMoveEnqueuedMillis = event.eventTime
                }
                true
            } catch (error: Throwable) {
                fault("触控采集失败: ${error.message}")
                true
            }
        }

        override fun performClick(): Boolean {
            super.performClick()
            return true
        }

        private fun visibleContacts(event: MotionEvent): List<Pair<Float, Float>> = when (
            event.actionMasked
        ) {
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> emptyList()
            MotionEvent.ACTION_POINTER_UP -> (0 until event.pointerCount)
                .filter { it != event.actionIndex }
                .map { event.getX(it) to event.getY(it) }
            else -> (0 until event.pointerCount).map { event.getX(it) to event.getY(it) }
        }

        private fun visibleContactDiagnostics(event: MotionEvent): List<TouchpadContactDiagnostic> =
            when (event.actionMasked) {
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> emptyList()
                MotionEvent.ACTION_POINTER_UP -> (0 until event.pointerCount)
                    .filter { it != event.actionIndex }
                    .map { event.contactDiagnostic(it) }
                else -> (0 until event.pointerCount).map { event.contactDiagnostic(it) }
            }

        private fun MotionEvent.contactDiagnostic(index: Int) = TouchpadContactDiagnostic(
            contactId = getPointerId(index),
            pressure = getPressure(index),
            touchMajorPx = getTouchMajor(index),
            touchMinorPx = getTouchMinor(index),
        )

        override fun onDraw(canvas: Canvas) {
            canvas.drawColor(Color.rgb(242, 246, 251))
            paint.color = Color.rgb(27, 110, 243)
            contacts.forEach { (x, y) -> canvas.drawCircle(x, y, 34f, paint) }
            paint.color = Color.rgb(16, 20, 26)
            paint.textSize = 42f
            canvas.drawText("CapyIO 完整触控板实验", 48f, 70f, paint)
            paint.textSize = 26f
            canvas.drawText("可使用 1–5 指；手势由 Windows Precision Touchpad 解释", 48f, 112f, paint)
            paint.textSize = 22f
            canvas.drawText(
                "请保持本页在前台；切换到其他应用后，本页将停止接收触摸",
                48f,
                148f,
                paint,
            )
            paint.textSize = 23f
            val diagnosticSnapshot = inputDiagnostics.snapshot()
            val currentPressure = contactDiagnostics.joinToString(separator = " ") { contact ->
                "#${contact.contactId} ${formatDiagnostic(contact.pressure)}"
            }.ifEmpty { "无" }
            val currentSize = contactDiagnostics.joinToString(separator = " ") { contact ->
                "#${contact.contactId} ${formatDiagnostic(contact.touchMajorPx)}×" +
                    formatDiagnostic(contact.touchMinorPx)
            }.ifEmpty { "无" }
            canvas.drawText(
                "压力诊断: 当前 $currentPressure；累计 " +
                    "${formatDiagnostic(diagnosticSnapshot.minimumPressure)}–" +
                    "${formatDiagnostic(diagnosticSnapshot.maximumPressure)}；" +
                    "样本 ${diagnosticSnapshot.sampleCount}",
                48f,
                height - 184f,
                paint,
            )
            canvas.drawText(
                "接触尺寸(px): 当前 $currentSize；累计最大 " +
                    "${formatDiagnostic(diagnosticSnapshot.maximumTouchMajorPx)}×" +
                    "${formatDiagnostic(diagnosticSnapshot.maximumTouchMinorPx)}；" +
                    "五指帧 ${diagnosticSnapshot.fiveFingerFrames} / 手势 " +
                    "${diagnosticSnapshot.fiveFingerGestures}",
                48f,
                height - 148f,
                paint,
            )
            canvas.drawText(
                "策略: 一指即时移动；新增触点稳定 ${ADDED_CONTACT_SETTLE_MILLIS}ms；3 指以上位移 " +
                    "$MULTI_FINGER_MOTION_SCALE_PERCENT%",
                48f,
                height - 112f,
                paint,
            )
            canvas.drawText(status, 48f, height - 76f, paint)
            canvas.drawText(
                "触点: $currentContactCount（最高 $maxContactCount）  " +
                    "系统抢占: $probableSystemGestureCancellationCount  " +
                    "独立振动: ${if (touchpadHapticsEnabled) "开" else "关"}  " +
                    "强度: ${touchpadHapticStrength.label}  " +
                    "点击触感: $tapHapticCount  拖动触感: $tapDragHapticCount  " +
                    "已确认数据帧: $acknowledged",
                48f,
                height - 40f,
                paint,
            )
        }

        private fun formatDiagnostic(value: Float?): String = value?.let {
            String.format(Locale.US, "%.2f", it)
        } ?: "--"

        private fun updateTapDragFeedback(event: MotionEvent) {
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> tapDragFeedbackTracker.onDown(
                    event.eventTime,
                    event.getX(0),
                    event.getY(0),
                )
                MotionEvent.ACTION_MOVE -> if (
                    event.pointerCount == 1 &&
                    tapDragFeedbackTracker.onMove(event.getX(0), event.getY(0))
                ) {
                    if (vibrateTouchpad(TouchpadHapticEffect.Drag)) {
                        tapDragHapticCount += 1
                        Log.i("CapyIO-PTP", "independent drag-start haptic requested")
                        invalidate()
                    }
                }
                MotionEvent.ACTION_UP -> if (
                    tapDragFeedbackTracker.onUp(
                        event.eventTime,
                        event.getX(0),
                        event.getY(0),
                    ) && vibrateTouchpad(TouchpadHapticEffect.Tap)
                ) {
                    tapHapticCount += 1
                    Log.i("CapyIO-PTP", "independent tap haptic requested")
                    invalidate()
                }
                MotionEvent.ACTION_POINTER_DOWN,
                MotionEvent.ACTION_CANCEL,
                -> tapDragFeedbackTracker.cancel()
            }
        }

        private fun showGestureConflictDialog() {
            AlertDialog.Builder(this@TouchpadLabActivity)
                .setTitle("手机系统正在抢占多指触摸")
                .setMessage(
                    "第三或第四根手指落下后，OriginOS 向 CapyIO 发送了整组取消。" +
                        "请在“超级截屏”中关闭“三指下滑截屏”，并关闭其他三指系统手势后重试。",
                )
                .setPositiveButton("打开超级截屏设置") { _, _ ->
                    openSystemGestureSettings()
                }
                .setNegativeButton("继续测试", null)
                .show()
        }

        private fun openSystemGestureSettings() {
            val vivoSettings = Intent(VIVO_SMARTSHOT_SETTINGS_ACTION).apply {
                setPackage(VIVO_SMARTSHOT_PACKAGE)
            }
            val destination = if (vivoSettings.resolveActivity(packageManager) != null) {
                vivoSettings
            } else {
                Intent(Settings.ACTION_SETTINGS)
            }
            runCatching { startActivity(destination) }
                .onFailure { startActivity(Intent(Settings.ACTION_SETTINGS)) }
        }

        fun shutdown() {
            if (!running.get()) return
            val wasConnected = connected.get()
            session?.let { active ->
                runCatching {
                    active.close(motionClockNanos()).forEach(::enqueueFinal)
                }
            }
            running.set(false)
            if (!wasConnected) sender?.interrupt()
            session = null
        }

        private fun startSender() {
            check(sender == null) { "sender is already started" }
            sender = Thread(::sendLoop, "capyio-touchpad-lab-sender").apply { start() }
        }

        private fun enqueue(record: ByteArray) {
            check(running.get()) { "sender is stopped" }
            check(records.offer(record)) { "bounded record queue is full" }
        }

        private fun enqueueFinal(record: ByteArray) {
            records.offer(record)
        }

        private fun sendLoop() {
            var connectedOnce = false
            var waitingReported = false
            while (running.get()) {
                try {
                    Socket().use { socket ->
                        socket.tcpNoDelay = true
                        socket.soTimeout = 3_000
                        socket.connect(InetSocketAddress("127.0.0.1", LAB_PORT), 3_000)
                        val input = DataInputStream(socket.getInputStream())
                        val output = DataOutputStream(socket.getOutputStream())
                        connected.set(true)
                        connectedOnce = true
                        postStatus("ADB 隧道已连接，可以开始触控")
                        while (running.get() || records.isNotEmpty()) {
                            val record = try {
                                records.take()
                            } catch (_: InterruptedException) {
                                if (!running.get()) break else continue
                            }
                            output.write(record)
                            output.flush()
                            when (record[5].toInt()) {
                                2 -> {
                                    val sequence = littleEndianLong(record, 16)
                                    val ack = ByteArray(24)
                                    input.readFully(ack)
                                    validateAck(ack, sequence)
                                    acknowledged += 1
                                    post { invalidate() }
                                }
                                4 -> return
                            }
                        }
                    }
                    return
                } catch (error: Exception) {
                    connected.set(false)
                    if (!running.get()) return
                    if (connectedOnce) {
                        fault(
                            "Windows 接收器已结束（诊断完成、超时或主机退出）；" +
                                "请先确认 Windows 接收器正在运行，再重新打开本页",
                        )
                        return
                    }
                    if (error !is ConnectException) {
                        fault("发送链路失败: ${error.message}")
                        return
                    }
                    if (!waitingReported) {
                        postStatus("等待 Windows 接收器；本机触摸仍会显示")
                        waitingReported = true
                    }
                    try {
                        Thread.sleep(500)
                    } catch (_: InterruptedException) {
                        if (!running.get()) return
                    }
                } finally {
                    if (connectedOnce) connected.set(false)
                }
            }
        }

        private fun validateAck(ack: ByteArray, expectedSequence: Long) {
            check(ack.size == 24)
            check(ack[0] == 'C'.code.toByte() && ack[1] == 'P'.code.toByte())
            check(ack[2] == 'T'.code.toByte() && ack[3] == 'R'.code.toByte())
            check(ack[4].toInt() == 1 && ack[5].toInt() == 3)
            check(ack[6].toInt() == 0 && ack[7].toInt() == 0)
            check(littleEndianLong(ack, 8) == ROUTE_EPOCH)
            check(littleEndianLong(ack, 16) == expectedSequence)
        }

        private fun littleEndianLong(bytes: ByteArray, offset: Int): Long =
            ByteBuffer.wrap(bytes, offset, 8).order(ByteOrder.LITTLE_ENDIAN).long

        private fun motionClockNanos(): Long = SystemClock.uptimeMillis() * 1_000_000L

        private fun postStatus(message: String) {
            Log.i("CapyIO-PTP", message)
            post {
                status = message
                invalidate()
            }
        }

        private fun fault(message: String) {
            Log.e("CapyIO-PTP", message)
            running.set(false)
            postStatus(message)
        }
    }
}
