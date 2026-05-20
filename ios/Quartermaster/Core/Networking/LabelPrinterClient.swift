import Foundation
import Network

protocol LabelPrinterSending: Sendable {
  func send(_ payload: Data, to host: String, port: Int) async throws
}

struct LabelPrinterClient: LabelPrinterSending {
  func send(_ payload: Data, to host: String, port: Int) async throws {
    guard let rawPort = UInt16(exactly: port),
      let nwPort = NWEndpoint.Port(rawValue: rawPort)
    else {
      throw APIError.server(status: 400, body: nil)
    }
    let connection = NWConnection(host: NWEndpoint.Host(host), port: nwPort, using: .tcp)
    let sender = LabelPrinterConnection(connection: connection)
    try await sender.send(payload)
  }
}

private final class LabelPrinterConnection: @unchecked Sendable {
  private let connection: NWConnection

  init(connection: NWConnection) {
    self.connection = connection
  }

  func send(_ payload: Data) async throws {
    try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation {
        (continuation: CheckedContinuation<Void, Error>) in
        let gate = LabelPrinterContinuationGate(connection: connection, continuation: continuation)

        connection.stateUpdateHandler = { [connection, gate] state in
          switch state {
          case .ready:
            connection.send(
              content: payload,
              completion: .contentProcessed { error in
                if let error {
                  gate.resume(.failure(error))
                } else {
                  gate.resume(.success(()))
                }
              })
          case .failed(let error):
            gate.resume(.failure(error))
          case .cancelled:
            gate.resume(.failure(URLError(.cancelled)))
          default:
            break
          }
        }
        connection.start(queue: .global(qos: .userInitiated))
      }
    } onCancel: {
      connection.cancel()
    }
  }
}

private final class LabelPrinterContinuationGate: @unchecked Sendable {
  private let connection: NWConnection
  private let continuation: CheckedContinuation<Void, Error>
  private let lock = NSLock()
  private var resumed = false

  init(connection: NWConnection, continuation: CheckedContinuation<Void, Error>) {
    self.connection = connection
    self.continuation = continuation
  }

  func resume(_ result: Result<Void, Error>) {
    lock.lock()
    defer { lock.unlock() }
    guard !resumed else { return }
    resumed = true
    connection.cancel()
    continuation.resume(with: result)
  }
}
