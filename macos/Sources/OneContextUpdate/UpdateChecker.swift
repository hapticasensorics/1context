import Foundation
import OneContextCore
import OneContextPlatform

public struct ReleaseInfo: Sendable {
  public let version: String
  public let notesURL: URL?

  public init(version: String, notesURL: URL?) {
    self.version = version
    self.notesURL = notesURL
  }
}

public struct UpdateCheckResult: Sendable {
  public let latest: ReleaseInfo?
  public let updateAvailable: Bool
  public let checked: Bool
}

public final class UpdateChecker {
  private let environment: [String: String]
  private let session: URLSession

  public init(
    environment: [String: String] = ProcessInfo.processInfo.environment,
    session: URLSession = .shared
  ) {
    self.environment = environment
    self.session = session
  }

  public func check(force: Bool = false, currentVersion: String = oneContextVersion) async throws -> UpdateCheckResult {
    if !force && environment["ONECONTEXT_NO_UPDATE_CHECK"] == "1" {
      return UpdateCheckResult(latest: nil, updateAvailable: false, checked: false)
    }

    let statePaths = UpdateStatePaths.current(environment: environment)
    if !force, let cached = readState(at: statePaths.file) {
      let lastChecked = (cached["last_checked_at"] as? String).flatMap {
        ISO8601DateFormatter().date(from: $0)
      }
      if let lastChecked, Date().timeIntervalSince(lastChecked) < oneContextUpdateCheckInterval {
        let release = releaseInfo(fromState: cached)
        return UpdateCheckResult(
          latest: release,
          updateAvailable: release.map { compareVersions($0.version, currentVersion) > 0 } ?? false,
          checked: false
        )
      }
    }

    let release = try await fetchLatestRelease(currentVersion: currentVersion)
    writeState(release, at: statePaths)
    return UpdateCheckResult(
      latest: release,
      updateAvailable: compareVersions(release.version, currentVersion) > 0,
      checked: true
    )
  }

  public func cached(currentVersion: String = oneContextVersion) -> UpdateCheckResult? {
    let state = readState(at: UpdateStatePaths.current(environment: environment).file)
    guard let release = state.flatMap(releaseInfo(fromState:)) else { return nil }
    return UpdateCheckResult(
      latest: release,
      updateAvailable: compareVersions(release.version, currentVersion) > 0,
      checked: false
    )
  }

  private func fetchLatestRelease(currentVersion: String) async throws -> ReleaseInfo {
    let url = environment["ONECONTEXT_UPDATE_URL"].flatMap(URL.init(string:)) ?? oneContextLatestReleaseURL
    var request = URLRequest(url: url)
    if isGitHubLatestReleaseRedirect(url) {
      request.httpMethod = "HEAD"
    }
    request.setValue("application/json, text/html;q=0.8", forHTTPHeaderField: "accept")
    request.setValue("1context/\(currentVersion)", forHTTPHeaderField: "user-agent")
    request.timeoutInterval = 5

    let (data, response) = try await session.data(for: request)
    guard let httpResponse = response as? HTTPURLResponse,
      (200..<300).contains(httpResponse.statusCode)
    else {
      throw URLError(.badServerResponse)
    }

    if let release = releaseInfo(fromRedirectURL: httpResponse.url ?? url) {
      return release
    }

    guard !data.isEmpty else {
      throw URLError(.cannotParseResponse)
    }

    let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    let release = (object?["stable"] as? [String: Any]) ?? object
    let rawVersion = release?["version"] as? String
      ?? release?["tag_name"] as? String
      ?? release?["name"] as? String
      ?? ""
    let version = rawVersion.replacingOccurrences(of: "^v", with: "", options: .regularExpression)
    let notesURL = (release?["notes_url"] as? String ?? release?["html_url"] as? String).flatMap(URL.init(string:))
    guard !version.isEmpty else {
      throw URLError(.cannotParseResponse)
    }
    return ReleaseInfo(version: version, notesURL: notesURL)
  }

  private func isGitHubLatestReleaseRedirect(_ url: URL) -> Bool {
    url.host == "github.com" && url.path.hasSuffix("/releases/latest")
  }

  private func releaseInfo(fromRedirectURL url: URL) -> ReleaseInfo? {
    let components = url.path.split(separator: "/").map(String.init)
    guard let tagIndex = components.firstIndex(of: "tag"),
      components.indices.contains(tagIndex + 1)
    else {
      return nil
    }

    let version = components[tagIndex + 1].replacingOccurrences(of: "^v", with: "", options: .regularExpression)
    guard !version.isEmpty else { return nil }
    return ReleaseInfo(version: version, notesURL: url)
  }

  private func readState(at url: URL) -> [String: Any]? {
    guard let data = try? Data(contentsOf: url) else { return nil }
    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
  }

  private func releaseInfo(fromState state: [String: Any]) -> ReleaseInfo? {
    guard let version = state["last_seen_latest"] as? String else { return nil }
    return ReleaseInfo(
      version: version,
      notesURL: (state["notes_url"] as? String).flatMap(URL.init(string:))
    )
  }

  private func writeState(_ release: ReleaseInfo, at paths: UpdateStatePaths) {
    try? RuntimePermissions.ensurePrivateDirectory(paths.directory)
    var state: [String: Any] = [
      "last_checked_at": ISO8601DateFormatter().string(from: Date()),
      "last_seen_latest": release.version
    ]
    if let notesURL = release.notesURL {
      state["notes_url"] = notesURL.absoluteString
    }
    if let data = try? JSONSerialization.data(withJSONObject: state, options: [.prettyPrinted, .sortedKeys]) {
      try? RuntimePermissions.writePrivateData(data + Data([UInt8(ascii: "\n")]), to: paths.file)
    }
  }
}

