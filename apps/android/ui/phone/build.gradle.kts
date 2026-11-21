// The phone and tablet interface: Material 3 from the shared tokens, the
// navigation bar or rail, the screens and the player.
plugins {
    alias(libs.plugins.pley.android.library)
    alias(libs.plugins.pley.android.compose)
}

android {
    namespace = "app.pley.android.ui.phone"
}

dependencies {
    api(project(":core:screens"))
    implementation(project(":core:designsystem"))
    implementation(project(":core:player"))
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material3.navigation.suite)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.viewmodel.navigation3)
    implementation(libs.androidx.navigation3.ui)
    implementation(libs.androidx.media3.ui.compose)
    implementation(libs.androidx.car.app)

    testImplementation(project(":core:testing"))
    testImplementation(libs.robolectric)
    testImplementation(libs.androidx.compose.ui.test.junit4)
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.kotlinx.coroutines.test)
    // The fake sign-in module keeps its tokens with org.json, a stub in local tests.
    testImplementation(libs.json)
    debugImplementation(libs.androidx.compose.ui.test.manifest)
}
