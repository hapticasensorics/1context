// swift-tools-version: 6.0

import PackageDescription

let package = Package(
  name: "OneContextMac",
  platforms: [
    .macOS(.v13)
  ],
  products: [
    .executable(name: "1context", targets: ["OneContextCLI"]),
    .executable(name: "1contextd", targets: ["OneContextDaemon"]),
    .executable(name: "1context-local-web-proxy", targets: ["OneContextLocalWebProxy"]),
    .executable(name: "OneContextMenuBar", targets: ["OneContextMenuBar"])
  ],
  dependencies: [
    .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.9.1")
  ],
  targets: [
    .target(name: "OneContextCore"),
    .target(
      name: "OneContextPlatform",
      dependencies: ["OneContextCore"]
    ),
    .target(
      name: "OneContextProtocol",
      dependencies: ["OneContextCore", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextUpdate",
      dependencies: ["OneContextCore", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextSparkleUpdate",
      dependencies: [
        "OneContextUpdate",
        .product(name: "Sparkle", package: "Sparkle")
      ]
    ),
    .target(
      name: "OneContextInstall",
      dependencies: ["OneContextCore"]
    ),
    .target(
      name: "OneContextPermissions",
      dependencies: ["OneContextCore", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextSetup",
      dependencies: ["OneContextLocalWeb", "OneContextPermissions", "OneContextUpdate"]
    ),
    .target(
      name: "OneContextAgent",
      dependencies: ["OneContextCore", "OneContextPlatform", "OneContextProtocol"]
    ),
    .target(
      name: "OneContextMemoryCore",
      dependencies: ["OneContextCore", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextLocalWeb",
      dependencies: ["OneContextCore", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextSupervisor",
      dependencies: ["OneContextCore", "OneContextPlatform", "OneContextProtocol"]
    ),
    .target(
      name: "OneContextRuntimeSupport",
      dependencies: [
        "OneContextCore",
        "OneContextPlatform",
        "OneContextProtocol",
        "OneContextUpdate",
        "OneContextPermissions",
        "OneContextSupervisor"
      ]
    ),
    .executableTarget(
      name: "OneContextCLI",
      dependencies: ["OneContextRuntimeSupport", "OneContextAgent", "OneContextInstall", "OneContextLocalWeb", "OneContextMemoryCore", "OneContextSetup"]
    ),
    .executableTarget(
      name: "OneContextDaemon",
      dependencies: ["OneContextRuntimeSupport", "OneContextAgent", "OneContextLocalWeb", "OneContextMemoryCore", "OneContextSetup"]
    ),
    .executableTarget(
      name: "OneContextLocalWebProxy"
    ),
    .executableTarget(
      name: "OneContextMenuBar",
      dependencies: ["OneContextRuntimeSupport", "OneContextAgent", "OneContextInstall", "OneContextLocalWeb", "OneContextPermissions", "OneContextSetup", "OneContextSparkleUpdate"],
      exclude: ["Resources"],
      linkerSettings: [
        .unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks"])
      ]
    ),
    .testTarget(
      name: "OneContextCoreTests",
      dependencies: ["OneContextCore"]
    ),
    .testTarget(
      name: "OneContextPlatformTests",
      dependencies: ["OneContextPlatform"]
    ),
    .testTarget(
      name: "OneContextProtocolTests",
      dependencies: ["OneContextProtocol"]
    ),
    .testTarget(
      name: "OneContextUpdateTests",
      dependencies: ["OneContextUpdate"]
    ),
    .testTarget(
      name: "OneContextInstallTests",
      dependencies: ["OneContextInstall"]
    ),
    .testTarget(
      name: "OneContextPermissionsTests",
      dependencies: ["OneContextPermissions"]
    ),
    .testTarget(
      name: "OneContextSetupTests",
      dependencies: ["OneContextSetup"]
    ),
    .testTarget(
      name: "OneContextAgentTests",
      dependencies: ["OneContextAgent"]
    ),
    .testTarget(
      name: "OneContextMemoryCoreTests",
      dependencies: ["OneContextMemoryCore"]
    ),
    .testTarget(
      name: "OneContextLocalWebTests",
      dependencies: ["OneContextLocalWeb"]
    )
  ]
)
