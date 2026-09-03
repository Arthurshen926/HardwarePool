# CapyIO Controller Lab for Android

This foreground-only lab publishes a complete touch-controller snapshot and the
latest accelerometer/gyroscope values over bounded UDP. It is intentionally
separate from the unified Android Node host until that host owns a stable
Gradle/JNI/runtime boundary.

## Build

Requirements: JDK 17, Android SDK platform 36, build-tools 36.1.0 and Android
Gradle Plugin 9.3.1.

```powershell
$env:JAVA_HOME = 'C:\Program Files\Android\Android Studio\jbr'
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
gradle --no-daemon :app:assembleDebug
```

The APK is written to `app/build/outputs/apk/debug/app-debug.apk`. The package
name is `io.capyio.controllerlab`; rollback is:

```powershell
adb uninstall io.capyio.controllerlab
```

Android sensor vectors retain the normative fixed device coordinate frame.
The Activity uses one fixed landscape orientation so the Windows DS4
Projection can apply one explicit and tested phone-mount permutation without
silently changing `capyio.motion.imu-samples/1` semantics.

## Physical lab flow

1. Open the desktop Controller view and start **Android controller input**.
2. Copy the displayed LAN IPv4, UDP port and hexadecimal token to the phone.
3. Press **开始** on the phone. The desktop source changes to
   `android_touch`, live controls and IMU values move, and packet age remains
   below the 350 ms neutral timeout.
4. Start DSU on the desktop and connect Cemu/Dolphin to `127.0.0.1:26760`.
5. Stop DSU before stopping Android input. Pausing the Activity, stopping the
   sender or losing the peer produces a neutral controller state.

For an operator-assisted debug gate that does not require navigating the desktop
UI, run the debug executable with:

```powershell
target/debug/capyio-desktop.exe --gamepad-physical-gate 31581 26761
```

Configure the printed token and Android port, start the phone sender, then press
a control and move the device. The command replaces its DSU subscriber once and
fails unless the replacement observes both non-neutral controls and finite IMU
data. This entry point is absent from release builds.

The token filters unrelated trusted-LAN traffic but does not provide encrypted
or production-grade pairing. Do not use this lab on an untrusted network.
