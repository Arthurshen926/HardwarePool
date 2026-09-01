plugins {
    id("com.android.application")
}

providers.gradleProperty("capyioCameraBuildDir").orNull?.let {
    layout.buildDirectory.set(file(it))
}

android {
    namespace = "io.capyio.camera.lab"
    compileSdk = 36

    defaultConfig {
        applicationId = "io.capyio.camera.lab"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        buildConfig = false
    }

    sourceSets {
        getByName("main").java.directories.add("../camera-contract/src/main/java")
    }

    lint {
        abortOnError = true
        warningsAsErrors = true
        checkReleaseBuilds = true
        // The repository's audited Android toolchain is pinned to API 36.
        // Updating that pin is a separate explicit toolchain task.
        disable += setOf("GradleDependency", "OldTargetApi")
    }
}
