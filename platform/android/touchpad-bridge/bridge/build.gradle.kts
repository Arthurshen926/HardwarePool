plugins {
    id("com.android.library")
}

android {
    namespace = "dev.capyio.touchpad"
    compileSdk = 36

    defaultConfig {
        minSdk = 29
    }

    sourceSets {
        named("main") {
            jniLibs.directories.add("../../../../target/android-jni")
        }
    }
}
