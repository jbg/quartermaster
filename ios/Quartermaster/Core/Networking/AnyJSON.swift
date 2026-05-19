import Foundation

struct AnyJSON: Codable, Equatable {
  let value: Any
  let data: Data

  init(data: Data) throws {
    self.data = data
    self.value = try JSONSerialization.jsonObject(with: data)
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    let object = try container.decode(JSONValue.self)
    value = object.jsonObject
    data = try JSONSerialization.data(withJSONObject: object.jsonObject)
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    try container.encode(JSONValue(jsonObject: value))
  }

  static func == (lhs: AnyJSON, rhs: AnyJSON) -> Bool {
    lhs.data == rhs.data
  }
}

private enum JSONValue: Codable {
  case null
  case bool(Bool)
  case number(Double)
  case string(String)
  case array([JSONValue])
  case object([String: JSONValue])

  init(jsonObject: Any) throws {
    switch jsonObject {
    case is NSNull:
      self = .null
    case let value as Bool:
      self = .bool(value)
    case let value as NSNumber:
      self = .number(value.doubleValue)
    case let value as String:
      self = .string(value)
    case let value as [Any]:
      self = .array(try value.map(JSONValue.init(jsonObject:)))
    case let value as [String: Any]:
      self = .object(try value.mapValues(JSONValue.init(jsonObject:)))
    default:
      throw EncodingError.invalidValue(
        jsonObject,
        .init(codingPath: [], debugDescription: "Unsupported JSON value")
      )
    }
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    if container.decodeNil() {
      self = .null
    } else if let value = try? container.decode(Bool.self) {
      self = .bool(value)
    } else if let value = try? container.decode(Double.self) {
      self = .number(value)
    } else if let value = try? container.decode(String.self) {
      self = .string(value)
    } else if let value = try? container.decode([JSONValue].self) {
      self = .array(value)
    } else {
      self = .object(try container.decode([String: JSONValue].self))
    }
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    switch self {
    case .null:
      try container.encodeNil()
    case .bool(let value):
      try container.encode(value)
    case .number(let value):
      try container.encode(value)
    case .string(let value):
      try container.encode(value)
    case .array(let value):
      try container.encode(value)
    case .object(let value):
      try container.encode(value)
    }
  }

  var jsonObject: Any {
    switch self {
    case .null:
      NSNull()
    case .bool(let value):
      value
    case .number(let value):
      value
    case .string(let value):
      value
    case .array(let value):
      value.map(\.jsonObject)
    case .object(let value):
      value.mapValues(\.jsonObject)
    }
  }
}
