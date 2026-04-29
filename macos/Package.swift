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
    .target(name: "OneContextRuntimeSupport"),
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
    )
  ]
)
