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
      dependencies: ["OneContextCore", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextSetup",
      dependencies: ["OneContextLocalWeb", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextLocalWeb",
      dependencies: ["OneContextCore", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextCapture",
      dependencies: ["OneContextCore", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextWikiRuntime",
      dependencies: ["OneContextPlatform"]
    ),
    .target(
      name: "OneContextMemoryRuntime",
      dependencies: ["OneContextCore", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextAgentRuntime",
      dependencies: ["OneContextCore", "OneContextPlatform"]
    ),
    .target(
      name: "OneContextSupervisor",
      dependencies: ["OneContextCore", "OneContextPlatform", "OneContextProtocol"]
    ),
    .executableTarget(
      name: "OneContextCLI",
      dependencies: ["OneContextCore", "OneContextPlatform", "OneContextProtocol", "OneContextSupervisor", "OneContextInstall", "OneContextLocalWeb", "OneContextSetup", "OneContextUpdate", "OneContextCapture"]
    ),
    .executableTarget(
      name: "OneContextDaemon",
      dependencies: ["OneContextCore", "OneContextPlatform", "OneContextProtocol", "OneContextLocalWeb", "OneContextSetup", "OneContextSupervisor", "OneContextCapture", "OneContextWikiRuntime", "OneContextMemoryRuntime", "OneContextAgentRuntime"]
    ),
    .executableTarget(
      name: "OneContextLocalWebProxy"
    ),
    .executableTarget(
      name: "OneContextMenuBar",
      dependencies: ["OneContextCore", "OneContextPlatform", "OneContextProtocol", "OneContextSupervisor", "OneContextInstall", "OneContextLocalWeb", "OneContextSetup", "OneContextUpdate", "OneContextSparkleUpdate"],
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
      name: "OneContextSparkleUpdateTests",
      dependencies: ["OneContextSparkleUpdate"]
    ),
    .testTarget(
      name: "OneContextInstallTests",
      dependencies: ["OneContextInstall"]
    ),
    .testTarget(
      name: "OneContextSetupTests",
      dependencies: ["OneContextSetup"]
    ),
    .testTarget(
      name: "OneContextLocalWebTests",
      dependencies: ["OneContextLocalWeb"]
    ),
    .testTarget(
      name: "OneContextCaptureTests",
      dependencies: ["OneContextCapture"]
    ),
    .testTarget(
      name: "OneContextWikiRuntimeTests",
      dependencies: ["OneContextWikiRuntime"]
    ),
    .testTarget(
      name: "OneContextMemoryRuntimeTests",
      dependencies: ["OneContextMemoryRuntime"]
    ),
    .testTarget(
      name: "OneContextAgentRuntimeTests",
      dependencies: ["OneContextAgentRuntime"]
    ),
    .testTarget(
      name: "OneContextSupervisorTests",
      dependencies: ["OneContextSupervisor"]
    )
  ]
)
