// swift-tools-version: 5.9

import Foundation
import PackageDescription

// `build-xcframework.sh` installs a binary target here.  Keeping the source
// system-library fallback makes the wrapper testable while developing the Rust
// library without committing a platform binary to the repository.
let binaryPath = "Artifacts/CVerseRecall.xcframework"
let forceSystemLibrary = ProcessInfo.processInfo.environment["VERSE_RECALL_USE_SYSTEM_LIBRARY"] == "1"
let cTarget: Target = !forceSystemLibrary && FileManager.default.fileExists(atPath: binaryPath)
    ? .binaryTarget(name: "CVerseRecall", path: binaryPath)
    : .systemLibrary(
        name: "CVerseRecall",
        path: "Sources/CVerseRecall"
    )

let package = Package(
    name: "VerseRecall",
    platforms: [
        .iOS(.v13),
        .macOS(.v11),
    ],
    products: [
        .library(name: "VerseRecall", targets: ["VerseRecall"]),
    ],
    targets: [
        cTarget,
        .target(
            name: "VerseRecall",
            dependencies: ["CVerseRecall"],
            path: "Sources/VerseRecall"
        ),
        .testTarget(
            name: "VerseRecallTests",
            dependencies: ["VerseRecall"],
            path: "Tests/VerseRecallTests"
        ),
    ]
)
