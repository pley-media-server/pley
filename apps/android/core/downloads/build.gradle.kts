// Downloads, for watching away from the server: the original file of a
// movie or an episode, with its details, picture and subtitles, fetched by
// a foreground service and played from the device; progress made offline
// waits until the server can be told.
plugins {
    alias(libs.plugins.pley.android.library)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "app.pley.android.downloads"
}

dependencies {
    api(project(":core:data"))
    implementation(libs.androidx.core.ktx)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.okhttp)

    // FakePleyApi, for the items' details.
    testImplementation(project(":core:testing"))
    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.okhttp.mockwebserver3)
    // Why a download failed, in each language's words.
    testImplementation(libs.robolectric)
    testImplementation(libs.androidx.test.core)
}
