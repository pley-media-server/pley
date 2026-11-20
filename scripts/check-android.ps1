# Checks the Android app's release build against LICENSING.md and its budgets:
#  * every library in the store release (:app's storeReleaseRuntimeClasspath) comes
#    from an allowed group, so no FFmpeg, mpv or GPL artifact slips in (the apps
#    contain no FFmpeg and no GPL code);
#  * the release APK has no native code (lib/**.so) but the two small libraries
#    AndroidX brings, built for every ABI: pley's own code has none, so it runs
#    on Fire TV's 32-bit userspace;
#  * the release APK is at most 12 MB.
# Run after `gradlew assembleStoreRelease` in apps/android (CI does both). The size
# goes to the job summary on GitHub Actions. Exit code 1 on violations.
#   pwsh scripts/check-android.ps1 [-Apk <path>] [-MaxMegabytes 12]
param(
    [string]$Apk,
    [double]$MaxMegabytes = 12
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$android = Join-Path $root "apps/android"
if (-not $Apk) {
    $Apk = Join-Path $android "app/build/outputs/apk/store/release/app-store-release-unsigned.apk"
}

# Groups whose artifacts may ship: AndroidX and Google's annotations and Guava
# pieces it brings, Kotlin and JetBrains', OkHttp and Okio, Coil (and the
# Accompanist drawable painter its Compose part uses, Apache 2.0) and the QR code
# library (MIT). A new group is a decision, made here.
$allowed = @(
    "androidx.*",
    "com.google.accompanist",
    "com.google.guava",
    "com.google.errorprone",
    "com.google.j2objc",
    "com.google.code.findbugs",
    "org.checkerframework",
    "org.jspecify",
    "org.jetbrains*",
    "com.squareup.okhttp3",
    "com.squareup.okio",
    "io.coil-kt.coil3",
    "io.nayuki"
)

# Native libraries that may ship, each for every ABI: Compose's path iterator
# (androidx.graphics:graphics-path) and DataStore's multi-process counter. Any
# other .so fails, pley's own above all.
$allowedNative = @(
    "libandroidx.graphics.path.so",
    "libdatastore_shared_counter.so"
)

$failed = $false

# ---------------------------------------------------------------- licenses

$windows = $env:OS -eq "Windows_NT"
$gradlew = if ($windows) { Join-Path $android "gradlew.bat" } else { Join-Path $android "gradlew" }
Push-Location $android
try {
    $tree = & $gradlew -q --no-daemon :app:dependencies --configuration storeReleaseRuntimeClasspath
    if ($LASTEXITCODE) { throw "gradlew :app:dependencies failed" }
} finally {
    Pop-Location
}

# Lines such as "+--- androidx.core:core-ktx:1.19.1 -> 1.19.1 (*)"; the app's own
# modules are "project :core:common".
$artifacts = @{}
foreach ($line in $tree) {
    if ($line -match '[+\\]--- ([^:\s]+):([^:\s]+)') {
        $artifacts["$($Matches[1]):$($Matches[2])"] = $Matches[1]
    }
}
if ($artifacts.Count -eq 0) {
    Write-Host "  ERROR: no libraries found in :app's storeReleaseRuntimeClasspath" -ForegroundColor Red
    $failed = $true
}
$refused = @($artifacts.Keys | Where-Object {
    $group = $artifacts[$_]
    -not ($allowed | Where-Object { $group -like $_ })
} | Sort-Object)
Write-Host "storeRelease: $($artifacts.Count) libraries"
foreach ($artifact in $refused) {
    Write-Host "  ERROR: $artifact is from a group outside the allowlist (scripts/check-android.ps1, LICENSING.md)" -ForegroundColor Red
    $failed = $true
}

# ---------------------------------------------------------------- the APK

if (-not (Test-Path $Apk)) {
    Write-Host "  ERROR: no release APK at $Apk (run gradlew assembleStoreRelease first)" -ForegroundColor Red
    exit 1
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path $Apk).Path)
try {
    $native = @($zip.Entries | Where-Object { $_.FullName -like "lib/*" -and $_.FullName -like "*.so" } | ForEach-Object { $_.FullName })
} finally {
    $zip.Dispose()
}
$foreign = @($native | Where-Object { $allowedNative -notcontains ($_ -split "/")[-1] })
foreach ($library in $foreign) {
    Write-Host "  ERROR: the release APK has native code: $library" -ForegroundColor Red
    $failed = $true
}
# What AndroidX brings must run on 32-bit ARM too (Fire TV).
foreach ($name in @($native | ForEach-Object { ($_ -split "/")[-1] } | Sort-Object -Unique)) {
    if ($allowedNative -contains $name -and $native -notcontains "lib/armeabi-v7a/$name") {
        Write-Host "  ERROR: $name has no armeabi-v7a build, which Fire TV needs" -ForegroundColor Red
        $failed = $true
    }
}

$bytes = (Get-Item $Apk).Length
$megabytes = $bytes / 1MB
$size = "{0:N2} MB ({1:N0} bytes)" -f $megabytes, $bytes
Write-Host "release APK: $size, at most $MaxMegabytes MB"
if ($megabytes -gt $MaxMegabytes) {
    Write-Host "  ERROR: the release APK is over $MaxMegabytes MB" -ForegroundColor Red
    $failed = $true
}
if ($env:GITHUB_STEP_SUMMARY) {
    Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value @(
        "### Android release",
        "",
        "| | |",
        "|---|---|",
        "| APK (storeRelease, unsigned) | $size, at most $MaxMegabytes MB |",
        "| Libraries | $($artifacts.Count), $($refused.Count) outside the allowlist |",
        "| Native libraries | $($native.Count - $foreign.Count) from AndroidX, $($foreign.Count) others |"
    )
}

if ($failed) { exit 1 }
Write-Host "Android release checks passed."
