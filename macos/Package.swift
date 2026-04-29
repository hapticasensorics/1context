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
    .executable(name: "OneContextMenuBar", targets: ["OneContextMenuBar"])
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
        "OneContextSupervisor"
      ]
    ),
    .executableTarget(
      name: "OneContextCLI",
      dependencies: ["OneContextRuntimeSupport"]
    ),
    .executableTarget(
      name: "OneContextDaemon",
      dependencies: ["OneContextRuntimeSupport"]
    ),
    .executableTarget(
      name: "OneContextMenuBar",
      dependencies: ["OneContextRuntimeSupport"],
      exclude: ["Resources"]
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
    )
  ]
)
