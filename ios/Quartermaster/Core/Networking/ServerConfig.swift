import Foundation

enum ServerConfig {
  static var defaultURL: URL {
    #if targetEnvironment(simulator)
      return URL(string: "http://localhost:8080")!
    #else
      return URL(string: "http://quartermaster.local:8080")!
    #endif
  }
}
