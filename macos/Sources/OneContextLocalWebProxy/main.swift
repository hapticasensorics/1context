import Darwin
import Foundation

private struct ProxyArguments {
  var listenHosts = ["127.0.0.1", "::1"]
  var listenPort = 443
  var targetHost = "127.0.0.1"
  var targetPort = 39191

  init(_ arguments: [String]) throws {
    var index = 0
    while index < arguments.count {
      let option = arguments[index]
      guard index + 1 < arguments.count else {
        throw ProxyError.invalidArguments("Missing value for \(option)")
      }
      let value = arguments[index + 1]
      switch option {
      case "--listen-host":
        listenHosts = value
          .split(separator: ",")
          .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
          .filter { !$0.isEmpty }
        guard !listenHosts.isEmpty else {
          throw ProxyError.invalidArguments("Invalid --listen-host: \(value)")
        }
      case "--listen-port":
        guard let port = Int(value), (1...65535).contains(port) else {
          throw ProxyError.invalidArguments("Invalid --listen-port: \(value)")
        }
        listenPort = port
      case "--target-host":
        targetHost = value
      case "--target-port":
        guard let port = Int(value), (1...65535).contains(port) else {
          throw ProxyError.invalidArguments("Invalid --target-port: \(value)")
        }
        targetPort = port
      default:
        throw ProxyError.invalidArguments("Unknown option: \(option)")
      }
      index += 2
    }
  }
}

private enum ProxyError: Error, LocalizedError {
  case invalidArguments(String)
  case socket(String)

  var errorDescription: String? {
    switch self {
    case .invalidArguments(let message), .socket(let message):
      return message
    }
  }
}

private final class TCPProxy {
  private let arguments: ProxyArguments

  init(arguments: ProxyArguments) {
    self.arguments = arguments
  }

  func run() throws -> Never {
    signal(SIGPIPE, SIG_IGN)
    let listenFDs = try arguments.listenHosts.map { host in
      try makeListenSocket(host: host, port: arguments.listenPort)
    }
    defer {
      for fd in listenFDs {
        close(fd)
      }
    }

    fputs(
      "1context-local-web-proxy listening on \(arguments.listenHosts.joined(separator: ",")):\(arguments.listenPort) -> \(arguments.targetHost):\(arguments.targetPort)\n",
      stderr
    )

    while true {
      var descriptors = listenFDs.map { pollfd(fd: $0, events: Int16(POLLIN), revents: 0) }
      let pollStatus = poll(&descriptors, nfds_t(descriptors.count), -1)
      if pollStatus < 0 {
        if errno == EINTR { continue }
        throw ProxyError.socket("poll failed: \(errnoDescription())")
      }

      for descriptor in descriptors where descriptor.revents & Int16(POLLIN) != 0 {
        let clientFD = accept(descriptor.fd, nil, nil)
        if clientFD < 0 {
          if errno == EINTR { continue }
          throw ProxyError.socket("accept failed: \(errnoDescription())")
        }

        DispatchQueue.global(qos: .userInitiated).async { [arguments] in
          TCPProxy.handle(clientFD: clientFD, arguments: arguments)
        }
      }
    }
  }

  private static func handle(clientFD: Int32, arguments: ProxyArguments) {
    let targetFD: Int32
    do {
      targetFD = try makeConnectedSocket(host: arguments.targetHost, port: arguments.targetPort)
    } catch {
      close(clientFD)
      fputs("1context-local-web-proxy connect failed: \(error.localizedDescription)\n", stderr)
      return
    }

    let group = DispatchGroup()
    group.enter()
    DispatchQueue.global(qos: .userInitiated).async {
      pump(from: clientFD, to: targetFD)
      shutdown(targetFD, SHUT_WR)
      group.leave()
    }

    group.enter()
    DispatchQueue.global(qos: .userInitiated).async {
      pump(from: targetFD, to: clientFD)
      shutdown(clientFD, SHUT_WR)
      group.leave()
    }

    group.wait()
    close(clientFD)
    close(targetFD)
  }

  private static func pump(from sourceFD: Int32, to destinationFD: Int32) {
    var buffer = [UInt8](repeating: 0, count: 64 * 1024)
    while true {
      let readCount = buffer.withUnsafeMutableBytes { bytes in
        Darwin.read(sourceFD, bytes.baseAddress, bytes.count)
      }
      if readCount == 0 {
        return
      }
      if readCount < 0 {
        if errno == EINTR { continue }
        return
      }

      var written = 0
      while written < readCount {
        let writeCount = buffer.withUnsafeBytes { bytes in
          Darwin.write(destinationFD, bytes.baseAddress!.advanced(by: written), readCount - written)
        }
        if writeCount < 0 {
          if errno == EINTR { continue }
          return
        }
        written += writeCount
      }
    }
  }
}

private func makeListenSocket(host: String, port: Int) throws -> Int32 {
  let address = try socketAddress(host: host, port: port)
  let fd = socket(address.family, SOCK_STREAM, 0)
  guard fd >= 0 else {
    throw ProxyError.socket("socket failed: \(errnoDescription())")
  }

  var reuse: Int32 = 1
  setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))
  if address.family == AF_INET6 {
    var v6Only: Int32 = 1
    setsockopt(fd, IPPROTO_IPV6, IPV6_V6ONLY, &v6Only, socklen_t(MemoryLayout<Int32>.size))
  }

  let bindStatus = address.withSockaddr {
    Darwin.bind(fd, $0, address.length)
  }
  guard bindStatus == 0 else {
    let message = errnoDescription()
    close(fd)
    throw ProxyError.socket("bind \(host):\(port) failed: \(message)")
  }

  guard listen(fd, SOMAXCONN) == 0 else {
    let message = errnoDescription()
    close(fd)
    throw ProxyError.socket("listen failed: \(message)")
  }
  return fd
}

private func makeConnectedSocket(host: String, port: Int) throws -> Int32 {
  let address = try socketAddress(host: host, port: port)
  let fd = socket(address.family, SOCK_STREAM, 0)
  guard fd >= 0 else {
    throw ProxyError.socket("socket failed: \(errnoDescription())")
  }

  let status = address.withSockaddr {
    connect(fd, $0, address.length)
  }
  guard status == 0 else {
    let message = errnoDescription()
    close(fd)
    throw ProxyError.socket("connect \(host):\(port) failed: \(message)")
  }
  return fd
}

private enum SocketAddress {
  case ipv4(sockaddr_in)
  case ipv6(sockaddr_in6)

  var family: Int32 {
    switch self {
    case .ipv4:
      return AF_INET
    case .ipv6:
      return AF_INET6
    }
  }

  var length: socklen_t {
    switch self {
    case .ipv4:
      return socklen_t(MemoryLayout<sockaddr_in>.size)
    case .ipv6:
      return socklen_t(MemoryLayout<sockaddr_in6>.size)
    }
  }

  func withSockaddr<Result>(_ body: (UnsafePointer<sockaddr>) -> Result) -> Result {
    switch self {
    case .ipv4(var address):
      return withUnsafePointer(to: &address) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1, body)
      }
    case .ipv6(var address):
      return withUnsafePointer(to: &address) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1, body)
      }
    }
  }
}

private func socketAddress(host: String, port: Int) throws -> SocketAddress {
  if host.contains(":") {
    var address = sockaddr_in6()
    address.sin6_len = UInt8(MemoryLayout<sockaddr_in6>.size)
    address.sin6_family = sa_family_t(AF_INET6)
    address.sin6_port = in_port_t(port).bigEndian
    guard inet_pton(AF_INET6, host, &address.sin6_addr) == 1 else {
      throw ProxyError.socket("Invalid IPv6 host: \(host)")
    }
    return .ipv6(address)
  }

  var address = sockaddr_in()
  address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
  address.sin_family = sa_family_t(AF_INET)
  address.sin_port = in_port_t(port).bigEndian
  guard inet_pton(AF_INET, host, &address.sin_addr) == 1 else {
    throw ProxyError.socket("Invalid IPv4 host: \(host)")
  }
  return .ipv4(address)
}

private func errnoDescription() -> String {
  String(cString: strerror(errno))
}

do {
  let arguments = try ProxyArguments(Array(CommandLine.arguments.dropFirst()))
  try TCPProxy(arguments: arguments).run()
} catch {
  fputs("1context-local-web-proxy: \(error.localizedDescription)\n", stderr)
  fputs(
    "Usage: 1context-local-web-proxy --listen-host 127.0.0.1,::1 --listen-port 443 --target-host 127.0.0.1 --target-port 39191\n",
    stderr
  )
  Foundation.exit(1)
}
