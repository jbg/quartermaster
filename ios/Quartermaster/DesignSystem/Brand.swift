import SwiftUI

enum QuartermasterBrand {
  static let ink = Color(red: 0x18 / 255, green: 0x20 / 255, blue: 0x1C / 255)
  static let green900 = Color(red: 0x17 / 255, green: 0x33 / 255, blue: 0x26 / 255)
  static let green800 = Color(red: 0x23 / 255, green: 0x4A / 255, blue: 0x35 / 255)
  static let green600 = Color(red: 0x3F / 255, green: 0x76 / 255, blue: 0x58 / 255)
  static let sage100 = Color(red: 0xE8 / 255, green: 0xEE / 255, blue: 0xE8 / 255)
  static let paper = Color(red: 0xF7 / 255, green: 0xF8 / 255, blue: 0xF4 / 255)
  static let slate700 = Color(red: 0x33 / 255, green: 0x40 / 255, blue: 0x39 / 255)
  static let slate500 = Color(red: 0x66 / 255, green: 0x71 / 255, blue: 0x6B / 255)
  static let label = Color(red: 0xF0 / 255, green: 0xEE / 255, blue: 0xE7 / 255)
  static let brass = Color(red: 0xA6 / 255, green: 0x6F / 255, blue: 0x2B / 255)
  static let blueprint = Color(red: 0x2F / 255, green: 0x5F / 255, blue: 0x7A / 255)
  static let beet = Color(red: 0x8F / 255, green: 0x2E / 255, blue: 0x3E / 255)
  static let beetStrong = Color(red: 0x9B / 255, green: 0x2F / 255, blue: 0x2F / 255)
  static let carrot = Color(red: 0xC5 / 255, green: 0x6B / 255, blue: 0x22 / 255)
  static let leaf = Color(red: 0x2F / 255, green: 0x7A / 255, blue: 0x4F / 255)

  static let successForeground = Color(red: 0x24 / 255, green: 0x6B / 255, blue: 0x45 / 255)
  static let successBackground = Color(red: 0xE4 / 255, green: 0xF2 / 255, blue: 0xEA / 255)
  static let warningForeground = Color(red: 0x9A / 255, green: 0x4F / 255, blue: 0x12 / 255)
  static let warningBackground = Color(red: 0xFF / 255, green: 0xF1 / 255, blue: 0xDF / 255)
  static let dangerBackground = Color(red: 0xF4 / 255, green: 0xE4 / 255, blue: 0xE4 / 255)
  static let infoForeground = Color(red: 0x24 / 255, green: 0x5B / 255, blue: 0x73 / 255)
  static let infoBackground = Color(red: 0xE4 / 255, green: 0xF0 / 255, blue: 0xF4 / 255)
  static let neutralForeground = Color(red: 0x58 / 255, green: 0x64 / 255, blue: 0x5F / 255)
  static let neutralBackground = Color(red: 0xEE / 255, green: 0xF1 / 255, blue: 0xEC / 255)
}

extension Color {
  static var quartermasterTint: Color { QuartermasterBrand.green800 }
  static var quartermasterError: Color { QuartermasterBrand.beetStrong }
}
