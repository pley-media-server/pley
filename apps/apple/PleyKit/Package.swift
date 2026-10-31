// swift-tools-version: 6.0
import PackageDescription

// Everything in Pley's Apple apps but their screens. Each module is a
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
        .library(name: "PleyTesting", targets: ["PleyTesting"]),
        .executable(name: "pley-apple-check", targets: ["pley-apple-check"]),
    ],
    targets: [
        .target(name: "PleyModel"),
        .target(name: "PleyCommon"),
        .target(name: "PleyNetwork", dependencies: ["PleyModel", "PleyCommon"]),
        .target(name: "PleyAuth", dependencies: ["PleyModel", "PleyCommon", "PleyNetwork"]),
        .target(name: "PleyData", dependencies: ["PleyModel", "PleyCommon", "PleyNetwork", "PleyAuth"]),
        .target(name: "PleyTesting", dependencies: ["PleyModel", "PleyCommon"]),
        // Signs in to a real, disposable server (scripts/check-server.sh). macOS only.
        .executableTarget(name: "pley-apple-check", dependencies: ["PleyData", "PleyAuth", "PleyNetwork", "PleyModel",
                                                                   "PleyCommon"]),
        .testTarget(name: "PleyModelTests", dependencies: ["PleyModel", "PleyTesting"]),
        .testTarget(name: "PleyCommonTests", dependencies: ["PleyCommon", "PleyTesting"]),
        .testTarget(name: "PleyNetworkTests", dependencies: ["PleyNetwork", "PleyModel", "PleyCommon", "PleyTesting"]),
        .testTarget(name: "PleyAuthTests",
                    dependencies: ["PleyAuth", "PleyNetwork", "PleyModel", "PleyCommon", "PleyTesting"]),
        .testTarget(name: "PleyDataTests",
                    dependencies: ["PleyData", "PleyAuth", "PleyNetwork", "PleyModel", "PleyCommon", "PleyTesting"]),
    ],
    swiftLanguageModes: [.v6]
)
