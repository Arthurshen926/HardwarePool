plugins {
    id("com.android.application")
}

android {
    namespace = "dev.capyio.touchpad.lab"
    compileSdk = 36

    defaultConfig {
        applicationId = "dev.capyio.touchpad.lab"
        minSdk = 29
        targetSdk = 36
        versionCode = 20
        versionName = "1.10"
    }
}

dependencies {
    implementation(project(":bridge"))
    testImplementation("junit:junit:4.13.2")
}
