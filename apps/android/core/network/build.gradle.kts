// One OkHttp stack for the app, the typed API, server events, discovery and
// the network checks.
plugins {
    alias(libs.plugins.pley.android.library)
}

android {
    namespace = "app.pley.android.network"
}

dependencies {
    api(project(":core:common"))
    api(project(":core:model"))
    api(project(":core:auth"))
    api(libs.okhttp)
    implementation(libs.okhttp.sse)
    implementation(libs.kotlinx.serialization.json.okio)
    implementation(libs.androidx.core.ktx)

    // FakePleyApi, the MockWebServer helpers and the JSON fixtures.
    testImplementation(project(":core:testing"))
    testImplementation(libs.okhttp.mockwebserver3)
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.turbine)
    // Android's org.json is a stub in local unit tests (the sign-in module's tokens, discovery's answers).
    testImplementation(libs.json)
    // The words of its errors, read in English, French and Spanish.
    testImplementation(libs.robolectric)
    testImplementation(libs.androidx.test.core)
}
