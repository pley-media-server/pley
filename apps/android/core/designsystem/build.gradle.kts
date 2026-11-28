// What both interfaces draw with: tokens, icons, artwork and its tiles.
// Compose foundation only: never Material, so the TV tree stays free of it.
plugins {
    alias(libs.plugins.pley.android.library)
    alias(libs.plugins.pley.android.compose)
}

android {
    namespace = "app.pley.android.designsystem"
}

dependencies {
    api(project(":core:common"))
    api(project(":core:model"))
    api(libs.androidx.compose.foundation)
    api(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.graphics)
    implementation(libs.androidx.core.ktx)
    api(libs.coil.compose)
    implementation(libs.coil.network.okhttp)
    implementation(libs.androidx.tracing.ktx)

    // The debug gallery (src/debug): every token and tile on one screen, phone or TV.
    debugImplementation(libs.androidx.activity.compose)

    testImplementation(libs.robolectric)
    testImplementation(libs.coil.test)
    testImplementation(libs.androidx.compose.ui.test.junit4)
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.kotlinx.coroutines.test)
    debugImplementation(libs.androidx.compose.ui.test.manifest)
}
