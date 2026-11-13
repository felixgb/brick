// swift-tools-version:5.9
import PackageDescription

// Build the Rust engine first:  cargo build --release -p brick-core
// (build-app.sh does both and wraps the result in Brick.app.)
let package = Package(
    name: "Brick",
    platforms: [.macOS(.v14)],
    targets: [
        .target(name: "CBrick", path: "Sources/CBrick"),
        .executableTarget(
            name: "Brick",
            dependencies: ["CBrick"],
            path: "Sources/Brick",
            linkerSettings: [
                .unsafeFlags(["-L../target/release"]),
                .linkedLibrary("brick_core"),
                .linkedFramework("Security"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("WebKit"),
            ]
        ),
    ]
)
