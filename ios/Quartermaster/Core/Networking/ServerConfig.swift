import Foundation

enum ServerConfig {
  static var defaultURL: URL {
    if let storedURL {
      return storedURL
    }
    if let configuredURL {
      return configuredURL
    }

    #if targetEnvironment(simulator)
      return URL(string: "http://localhost:8080")!
    #else
      return URL(string: "http://quartermaster.local:8080")!
    #endif
  }

  private static var configuredURL: URL? {
    guard
      let raw = Bundle.main.object(forInfoDictionaryKey: "QuartermasterAPIBaseURL") as? String,
      !raw.isEmpty
    else {
      return nil
    }
    return URL(string: raw)
  }

  static let storedURLKey = "quartermaster.server_url"

  private static var storedURL: URL? {
    guard
      let raw = UserDefaults.standard.string(forKey: storedURLKey),
      !raw.isEmpty
    else {
      return nil
    }
    return URL(string: raw)
  }
}
