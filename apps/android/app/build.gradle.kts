plugins {
    alias(libs.plugins.pley.android.application)
    alias(libs.plugins.pley.android.compose)
    alias(libs.plugins.baselineprofile)
}

// The app's version, its own (scripts/version.mjs sets it; docs/RELEASING.md),
// and the commit the build is made from: PLEY_COMMIT, else git's, else "unknown".
val appVersion = "0.1.6"
val pleyCommit: String = providers.environmentVariable("PLEY_COMMIT").orNull?.trim()?.take(9)?.ifEmpty { null }
    ?: providers.exec {
        commandLine("git", "rev-parse", "--short=9", "HEAD")
        isIgnoreExitValue = true
    }.standardOutput.asText.get().trim().ifEmpty { "unknown" }

/**
 * 1.2.3 is 11020300: every release above the one before, with room for
 * rebuilds. The 10,000,000 keeps the codes above those of the single version
 * every app had before (Google Play has 20003, from 0.2.0), so 0.0.1 is
 * 10000100. Minor and patch stay below 100 (scripts/version.mjs checks).
 */
fun versionCodeOf(version: String): Int {
    val (major, minor, patch) = version.split(".").map(String::toInt)
    return 10_000_000 + major * 1_000_000 + minor * 10_000 + patch * 100
}

android {
    namespace = "app.pley.android"

    defaultConfig {
        // The id Google Play and the Amazon Appstore know the app by. The code
        // keeps its namespace, and sign-in its app.pley.android: scheme.
        applicationId = "com.formandelement.pley"
        // -Ppley.applicationIdSuffix=.bench: a build that installs beside the
        // app already on a device (another key's), for benchmarks and profiles.
        providers.gradleProperty("pley.applicationIdSuffix").orNull?.let { applicationIdSuffix = it }
        // A rebuild of the same version for the stores, which take each code
        // once: PLEY_BUILD, 1 to 99, on top of the version's.
        versionCode = versionCodeOf(appVersion) +
            (providers.environmentVariable("PLEY_BUILD").orNull?.trim()?.toIntOrNull()?.takeIf { it in 0..99 } ?: 0)
        versionName = appVersion
        buildConfigField("String", "COMMIT", "\"$pleyCommit\"")
    }

    // Release builds are signed with the key these name (the release workflow
    // sets them from the repository's secrets), and unsigned without them.
    val keystore = providers.environmentVariable("PLEY_KEYSTORE").orNull
    if (keystore != null) {
        signingConfigs.create("release") {
            storeFile = file(keystore)
            storePassword = providers.environmentVariable("PLEY_KEYSTORE_PASSWORD").get()
            keyAlias = providers.environmentVariable("PLEY_KEY_ALIAS").get()
            keyPassword = providers.environmentVariable("PLEY_KEY_PASSWORD").orNull ?: storePassword
        }
    }

    // Where a build goes: the stores (no self-updater), or the pley server's
    // own download for sideloading (with an update check).
    flavorDimensions += "channel"
    productFlavors {
        create("store") {
            dimension = "channel"
        }
        create("direct") {
            dimension = "channel"
        }
    }

    buildTypes {
        release {
            signingConfigs.findByName("release")?.let { signingConfig = it }
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    buildFeatures {
        buildConfig = true
    }
}

baselineProfile {
    // The profiles :benchmark generates, committed in src/main (docs/ANDROID.md).
    mergeIntoMain = true
    saveInSrc = true
    automaticGenerationDuringBuild = false
}

// Benchmark builds too: the journeys walk the app against the demo server.
// Their build types come from the baselineprofile plugin, so their
// configurations exist only once it has added them.
configurations.matching { it.name == "benchmarkReleaseImplementation" || it.name == "nonMinifiedReleaseImplementation" }
    .configureEach { dependencies.add(project.dependencies.project(":testing:fixtures")) }

dependencies {
    implementation(project(":core:common"))
    implementation(project(":core:model"))
    implementation(project(":core:auth"))
    implementation(project(":core:network"))
    implementation(project(":core:data"))
    implementation(project(":core:caps"))
    implementation(project(":core:player"))
    implementation(project(":core:downloads"))
    implementation(project(":core:designsystem"))
    implementation(project(":core:screens"))
    implementation(project(":ui:phone"))
    implementation(project(":ui:tv"))
    baselineProfile(project(":benchmark"))
    // The fake server, for demos in debug builds; never in a release.
    debugImplementation(project(":testing:fixtures"))

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.core.splashscreen)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.androidx.profileinstaller)
    implementation(libs.androidx.tvprovider)
    implementation(libs.androidx.tracing.ktx)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.media3.exoplayer)
    implementation(libs.androidx.media3.exoplayer.hls)
    implementation(libs.androidx.media3.ui)
    // OkHttp sends the session token to the server only, also across redirects.
    implementation(libs.androidx.media3.datasource.okhttp)
    implementation(libs.okhttp)
    implementation(libs.kotlinx.coroutines.android)
}

// One command for every module (CI and docs/ANDROID.md): the flavors' debug
// unit tests and lint under the names the other modules use.
tasks.register("testDebugUnitTest") {
    group = "verification"
    dependsOn("testStoreDebugUnitTest", "testDirectDebugUnitTest")
}
tasks.register("lintDebug") {
    group = "verification"
    dependsOn("lintStoreDebug", "lintDirectDebug")
}
