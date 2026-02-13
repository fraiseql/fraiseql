// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "fraiseql-swift",
    platforms: [
        .macOS(.v13),
        .iOS(.v16),
        .tvOS(.v16),
        .watchOS(.v9)
    ],
    products: [
        .library(
            name: "FraiseQLSecurity",
            targets: ["FraiseQLSecurity"]
        )
    ],
    targets: [
        .target(
            name: "FraiseQLSecurity",
            path: "Sources",
            sources: ["FraiseQLSecurity"],
            publicHeadersPath: nil
        ),
        .testTarget(
            name: "FraiseQLSecurityTests",
            dependencies: ["FraiseQLSecurity"],
            path: "Tests"
        )
    ],
    swiftLanguageVersions: [.v5]
)
