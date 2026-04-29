import XCTest
import OneContextCore
@testable import OneContextProtocol

final class RPCEnvelopeTests: XCTestCase {
  func testTypedRequestEncodesRuntimeMethod() throws {
    let request = JSONRPCRequest(id: 7, method: .health, params: EmptyRPCParams())
    let data = try JSONEncoder().encode(request)
    let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]

    XCTAssertEqual(object?["jsonrpc"] as? String, "2.0")
    XCTAssertEqual(object?["id"] as? Int, 7)
    XCTAssertEqual(object?["method"] as? String, "health")
  }

  func testTypedResponseDecodesRuntimeHealth() throws {
    let data = Data("""
    {
      "jsonrpc": "2.0",
      "id": 1,
      "result": {
        "status": "ok",
        "version": "0.1.33",
        "uptimeSeconds": 3,
        "pid": 44
      }
    }
    """.utf8)

    let response = try JSONDecoder().decode(JSONRPCResponse<RuntimeHealth>.self, from: data)

    XCTAssertEqual(response.result?.status, "ok")
    XCTAssertEqual(response.result?.version, "0.1.33")
    XCTAssertNil(response.error)
  }
}
