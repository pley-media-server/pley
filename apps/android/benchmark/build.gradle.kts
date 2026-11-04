// Macrobenchmarks and the Baseline and Startup Profile generators, run
// against the app on a device or emulator (docs/ANDROID.md).
plugins {
    alias(libs.plugins.pley.android.test)
    alias(libs.plugins.baselineprofile)
}

// -Ppley.applicationIdSuffix: the app's id and this module's take it, so copies built with
// different suffixes (and different keys) install side by side.
val idSuffix: String = providers.gradleProperty("pley.applicationIdSuffix").orElse("").get()

android {
    namespace = "app.pley.android.benchmark"
    targetProjectPath = ":app"

    defaultConfig {
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        // The app's id, with the suffix it was built with (-Ppley.applicationIdSuffix).
        val target = "com.formandelement.pley$idSuffix"
        buildConfigField("String", "TARGET", "\"$target\"")
        manifestPlaceholders["target"] = target
        // The profiles are made on the store build; the direct one has the same code.
        missingDimensionStrategy("channel", "store")
    }

    buildFeatures {
        buildConfig = true
    }
}

baselineProfile {
    // Profiles are generated on purpose (docs/ANDROID.md), never during a build.
    useConnectedDevices = true
}

dependencies {
    implementation(libs.androidx.test.ext.junit)
    implementation(libs.androidx.test.runner)
    implementation(libs.androidx.test.uiautomator)
    implementation(libs.androidx.benchmark.macro.junit4)
}

androidComponents {
    onVariants { variant ->
        if (idSuffix.isNotEmpty()) variant.applicationId.set("app.pley.android.benchmark$idSuffix")
    }
}
