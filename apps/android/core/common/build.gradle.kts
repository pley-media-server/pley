// Rules and small parts every module shares: server addresses, discovery,
// the device's form and tier, image URLs, trace names, the frame-rate
// counter, the words the app shows. Depends on :core:model alone, whose
// format functions it gives their words.
plugins {
    alias(libs.plugins.pley.android.library)
}

android {
    namespace = "app.pley.android.common"
}

dependencies {
    api(project(":core:model"))
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.tracing.ktx)
    api(libs.kotlinx.coroutines.android)

    // Android's org.json is a stub in local unit tests.
    testImplementation(libs.json)
    // The frame-rate counter's windows.
    testImplementation(libs.robolectric)
    testImplementation(libs.androidx.test.core)
}
