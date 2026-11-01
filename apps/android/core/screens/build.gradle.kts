// One ViewModel per screen, shared by the phone and TV interfaces, and the
// routes between screens. No interface code; the words both interfaces
// share (requests') are its resources.
plugins {
    alias(libs.plugins.pley.android.library)
    alias(libs.plugins.kotlin.serialization)
}

android {
    namespace = "app.pley.android.screens"
}

dependencies {
    api(project(":core:data"))
    api(project(":core:player"))
    api(project(":core:caps"))
    api(project(":core:auth"))
    api(libs.androidx.lifecycle.viewmodel.ktx)
    // SavedStateHandle: what a screen keeps across the process's death (a sign-in in the browser).
    api(libs.androidx.lifecycle.viewmodel.savedstate)
    api(libs.androidx.navigation3.runtime)
    implementation(libs.kotlinx.serialization.json)

    testImplementation(libs.kotlinx.coroutines.test)
    testImplementation(libs.turbine)
    // Live TV's fakes and fixtures, the live player's engine on a Looper, FakeRequests; Robolectric for the requests' words.
    testImplementation(project(":core:testing"))
    testImplementation(libs.robolectric)
    testImplementation(libs.androidx.test.core)
}
