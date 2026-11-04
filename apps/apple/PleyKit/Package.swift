// swift-tools-version: 6.0
import PackageDescription

// Everything in Pley's Apple apps but their views. Each module is a
// target, and a target lists only the modules below it, so packages depend
// only downwards (docs/APPLE.md, "Code layout"). Apple's frameworks only:
// no third-party package.
let package = Package(
    name: "PleyKit",
    platforms: [.iOS(.v16), .tvOS(.v18), .macOS(.v15)],
    products: [
        .library(name: "PleyModel", targets: ["PleyModel"]),
        .library(name: "PleyCommon", targets: ["PleyCommon"]),
        .library(name: "PleyNetwork", targets: ["PleyNetwork"]),
        .library(name: "PleyAuth", targets: ["PleyAuth"]),
        .library(name: "PleyData", targets: ["PleyData"]),
        .library(name: "PleyCaps", targets: ["PleyCaps"]),
        .library(name: "PleySubtitles", targets: ["PleySubtitles"]),
        .library(name: "PleyPlayer", targets: ["PleyPlayer"]),
        .library(name: "PleyScreens", targets: ["PleyScreens"]),
        .library(name: "PleyPlayerViews", targets: ["PleyPlayerViews"]),
        .library(name: "PleyDownloads", targets: ["PleyDownloads"]),
        .library(name: "PleyShelf", targets: ["PleyShelf"]),
        .library(name: "PleyTesting", targets: ["PleyTesting"]),
        .executable(name: "pley-apple-check", targets: ["pley-apple-check"]),
        .executable(name: "pley-apple-play", targets: ["pley-apple-play"]),
        .executable(name: "pley-apple-fixtures", targets: ["pley-apple-fixtures"]),
    ],
    targets: [
        .target(name: "PleyModel"),
        .target(name: "PleyCommon"),
        // Apple TV's Top Shelf: the row the app saves and the extension shows, and its signed links.
        // Foundation and CryptoKit only, for the extension.
        .target(name: "PleyShelf"),
        .target(name: "PleyNetwork", dependencies: ["PleyModel", "PleyCommon"]),
        .target(name: "PleyAuth", dependencies: ["PleyModel", "PleyCommon", "PleyNetwork"]),
        .target(name: "PleyData", dependencies: ["PleyModel", "PleyCommon", "PleyNetwork", "PleyAuth"]),
        // What this device plays: the caps report sent with playback requests.
        .target(name: "PleyCaps", dependencies: ["PleyModel", "PleyCommon"]),
        // Subtitles the app draws: WebVTT, picture tracks (PGS, VobSub, DVB) and their sync.
        .target(name: "PleySubtitles", dependencies: ["PleyNetwork", "PleyModel"]),
        // Playing: sessions on the server, their reports, recovery, and the AVPlayer engine.
        .target(name: "PleyPlayer", dependencies: ["PleySubtitles", "PleyCaps", "PleyData", "PleyNetwork", "PleyModel",
                                                   "PleyCommon"]),
        // The screens' state, shared by the iPhone, iPad and Mac app and the Apple TV app; no views.
        .target(name: "PleyScreens", dependencies: ["PleyPlayer", "PleySubtitles", "PleyCaps", "PleyData", "PleyAuth",
                                                    "PleyNetwork", "PleyModel", "PleyCommon", "PleyShelf"]),
        // The player's surface on each platform (AVKit's own controls) and what the apps lay over it.
        .target(name: "PleyPlayerViews", dependencies: ["PleyPlayer", "PleySubtitles", "PleyCaps", "PleyData",
                                                        "PleyModel", "PleyCommon"]),
        // Downloads to watch without the server: background transfers in pieces, offline files (iPhone, iPad, Mac).
        .target(name: "PleyDownloads", dependencies: ["PleyData", "PleyCaps", "PleyAuth", "PleyNetwork", "PleyModel",
                                                      "PleyCommon"]),
        .target(name: "PleyTesting", dependencies: ["PleyModel", "PleyCommon"]),
        // Signs in to a real, disposable server (scripts/check-server.sh). macOS only.
        .executableTarget(name: "pley-apple-check", dependencies: ["PleyCaps", "PleyData", "PleyAuth", "PleyNetwork",
                                                                   "PleyModel", "PleyCommon", "PleyTesting"]),
        // Plays a disposable server's test media through the real player (scripts/check-playback.sh). macOS only.
        .executableTarget(name: "pley-apple-play", dependencies: ["PleyPlayer", "PleyDownloads", "PleySubtitles",
                                                                  "PleyCaps", "PleyAuth",
                                                                  "PleyData", "PleyNetwork", "PleyModel", "PleyCommon",
                                                                  "PleyTesting"]),
        // Serves the fixture server for the UI tests of requests (scripts/check-screens.sh). macOS only.
        .executableTarget(name: "pley-apple-fixtures", dependencies: ["PleyTesting"]),
        .testTarget(name: "PleyModelTests", dependencies: ["PleyModel", "PleyTesting"]),
        .testTarget(name: "PleyCommonTests", dependencies: ["PleyCommon", "PleyTesting"]),
        .testTarget(name: "PleyNetworkTests", dependencies: ["PleyNetwork", "PleyModel", "PleyCommon", "PleyTesting"]),
        .testTarget(name: "PleyAuthTests",
                    dependencies: ["PleyAuth", "PleyNetwork", "PleyModel", "PleyCommon", "PleyTesting"]),
        .testTarget(name: "PleyDataTests",
                    dependencies: ["PleyData", "PleyAuth", "PleyNetwork", "PleyModel", "PleyCommon", "PleyTesting"]),
        .testTarget(name: "PleyCapsTests", dependencies: ["PleyCaps", "PleyModel", "PleyCommon", "PleyTesting"],
                    exclude: ["Probes"]),
        .testTarget(name: "PleySubtitlesTests",
                    dependencies: ["PleySubtitles", "PleyNetwork", "PleyModel", "PleyCommon", "PleyTesting"]),
        .testTarget(name: "PleyPlayerTests",
                    dependencies: ["PleyPlayer", "PleySubtitles", "PleyCaps", "PleyData", "PleyNetwork", "PleyModel",
                                   "PleyCommon", "PleyTesting"]),
        // The player menus: UIKit's on the iOS and tvOS simulators, the action menu's delegate on the Mac.
        .testTarget(name: "PleyPlayerViewsTests", dependencies: ["PleyPlayerViews", "PleyPlayer", "PleyData"]),
        .testTarget(name: "PleyDownloadsTests",
                    dependencies: ["PleyDownloads", "PleyData", "PleyCaps", "PleyAuth", "PleyNetwork", "PleyModel",
                                   "PleyCommon", "PleyTesting"]),
        .testTarget(name: "PleyScreensTests",
                    dependencies: ["PleyScreens", "PleyPlayer", "PleyCaps", "PleyData", "PleyAuth", "PleyNetwork",
                                   "PleyModel", "PleyCommon", "PleyShelf", "PleyTesting"]),
        .testTarget(name: "PleyShelfTests", dependencies: ["PleyShelf"]),
    ],
    swiftLanguageModes: [.v6]
)
