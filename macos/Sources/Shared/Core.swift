import CBrick
import Foundation

struct CoreError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

/// Swift face of the Rust engine. `call` is synchronous and only touches the
/// local store, so it is fine on the main thread; network results come back
/// through `onEvent`, always on the main thread.
final class Core {
    private var engine: OpaquePointer?
    var onEvent: (([String: Any]) -> Void)?

    private static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()

    init(dataDirectory: URL) throws {
        try FileManager.default.createDirectory(at: dataDirectory, withIntermediateDirectories: true)

        let callback: BrickEventCallback = { json, user in
            // Engine thread. Copy the bytes, then hop to the main thread.
            guard let json, let user else { return }
            let data = Data(bytes: json, count: strlen(json))
            let core = Unmanaged<Core>.fromOpaque(user).takeUnretainedValue()
            DispatchQueue.main.async { core.deliver(data) }
        }

        var error: UnsafeMutablePointer<CChar>?
        engine = brick_open(dataDirectory.path, callback, Unmanaged.passUnretained(self).toOpaque(), &error)
        if engine == nil {
            let message = error.map { String(cString: $0) } ?? "unknown error"
            brick_string_free(error)
            throw CoreError(message: message)
        }
    }

    deinit {
        // After this returns the engine makes no further callbacks.
        brick_close(engine)
    }

    private func deliver(_ data: Data) {
        guard let event = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        onEvent?(event)
    }

    /// Sends a request and returns the raw "ok" value.
    @discardableResult
    func send(_ method: String, _ params: [String: Any] = [:]) throws -> Any {
        guard let engine else { throw CoreError(message: "The mail store isn't open.") }
        let request = try JSONSerialization.data(withJSONObject: ["method": method, "params": params])
        guard let text = String(data: request, encoding: .utf8), let raw = brick_call(engine, text) else {
            throw CoreError(message: "The request couldn't be sent.")
        }
        defer { brick_string_free(raw) }
        let data = Data(bytes: raw, count: strlen(raw))
        let response = (try JSONSerialization.jsonObject(with: data) as? [String: Any]) ?? [:]
        if let message = response["error"] as? String {
            throw CoreError(message: message)
        }
        return response["ok"] ?? NSNull()
    }

    /// Decodes a value taken from an event payload.
    static func decode<T: Decodable>(_ value: Any?, as type: T.Type = T.self) -> T? {
        guard let value, !(value is NSNull),
            let data = try? JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed])
        else { return nil }
        return try? decoder.decode(T.self, from: data)
    }

    /// Sends a request and decodes the result.
    func call<T: Decodable>(_ method: String, _ params: [String: Any] = [:]) throws -> T {
        let value = try send(method, params)
        let data = try JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed])
        return try Core.decoder.decode(T.self, from: data)
    }
}
