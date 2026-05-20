import SwiftUI
import UIKit

enum QuartermasterBrand {
  static let ink = Color(red: 0x18 / 255, green: 0x20 / 255, blue: 0x1C / 255)
  static let green900 = Color(red: 0x17 / 255, green: 0x33 / 255, blue: 0x26 / 255)
  static let green800 = Color(red: 0x23 / 255, green: 0x4A / 255, blue: 0x35 / 255)
  static let green600 = Color(red: 0x3F / 255, green: 0x76 / 255, blue: 0x58 / 255)
  static let sage100 = Color(red: 0xE8 / 255, green: 0xEE / 255, blue: 0xE8 / 255)
  static let paper = Color(red: 0xF7 / 255, green: 0xF8 / 255, blue: 0xF4 / 255)
  static let slate700 = Color(red: 0x33 / 255, green: 0x40 / 255, blue: 0x39 / 255)
  static let slate500 = Color(red: 0x66 / 255, green: 0x71 / 255, blue: 0x6B / 255)
  static let steel = Color(red: 0x7C / 255, green: 0x87 / 255, blue: 0x80 / 255)
  static let line = Color(red: 0xD9 / 255, green: 0xDE / 255, blue: 0xD6 / 255)
  static let lineStrong = Color(red: 0xC7 / 255, green: 0xD0 / 255, blue: 0xC7 / 255)
  static let label = Color(red: 0xF0 / 255, green: 0xEE / 255, blue: 0xE7 / 255)
  static let brass = Color(red: 0xA6 / 255, green: 0x6F / 255, blue: 0x2B / 255)
  static let blueprint = Color(red: 0x2F / 255, green: 0x5F / 255, blue: 0x7A / 255)
  static let beet = Color(red: 0x8F / 255, green: 0x2E / 255, blue: 0x3E / 255)
  static let beetStrong = Color(red: 0x9B / 255, green: 0x2F / 255, blue: 0x2F / 255)
  static let carrot = Color(red: 0xC5 / 255, green: 0x6B / 255, blue: 0x22 / 255)
  static let leaf = Color(red: 0x2F / 255, green: 0x7A / 255, blue: 0x4F / 255)

  static let successForeground = Color(red: 0x24 / 255, green: 0x6B / 255, blue: 0x45 / 255)
  static let successBackground = Color(red: 0xE4 / 255, green: 0xF2 / 255, blue: 0xEA / 255)
  static let successBorder = Color(red: 0xB9 / 255, green: 0xDE / 255, blue: 0xC8 / 255)
  static let warningForeground = Color(red: 0x9A / 255, green: 0x4F / 255, blue: 0x12 / 255)
  static let warningBackground = Color(red: 0xFF / 255, green: 0xF1 / 255, blue: 0xDF / 255)
  static let warningBorder = Color(red: 0xE9 / 255, green: 0xC2 / 255, blue: 0x89 / 255)
  static let dangerBackground = Color(red: 0xF8 / 255, green: 0xE2 / 255, blue: 0xE5 / 255)
  static let dangerBorder = Color(red: 0xE3 / 255, green: 0xA8 / 255, blue: 0xB1 / 255)
  static let infoForeground = Color(red: 0x24 / 255, green: 0x5B / 255, blue: 0x73 / 255)
  static let infoBackground = Color(red: 0xE4 / 255, green: 0xF0 / 255, blue: 0xF4 / 255)
  static let infoBorder = Color(red: 0xB5 / 255, green: 0xD3 / 255, blue: 0xDD / 255)
  static let neutralForeground = Color(red: 0x58 / 255, green: 0x64 / 255, blue: 0x5F / 255)
  static let neutralBackground = Color(red: 0xEE / 255, green: 0xF1 / 255, blue: 0xEC / 255)
  static let neutralBorder = Color(red: 0xD5 / 255, green: 0xDC / 255, blue: 0xD4 / 255)
  static let frozenForeground = Color(red: 0x1D / 255, green: 0x5C / 255, blue: 0x73 / 255)
  static let frozenBackground = Color(red: 0xDD / 255, green: 0xEF / 255, blue: 0xF4 / 255)
  static let frozenBorder = Color(red: 0xAC / 255, green: 0xD2 / 255, blue: 0xDE / 255)
}

enum QuartermasterSpacing {
  static let x1: CGFloat = 4
  static let x2: CGFloat = 8
  static let x3: CGFloat = 12
  static let x4: CGFloat = 16
  static let x5: CGFloat = 20
  static let x6: CGFloat = 24
  static let x8: CGFloat = 32
}

enum QuartermasterRadius {
  static let xs: CGFloat = 4
  static let sm: CGFloat = 6
  static let md: CGFloat = 8
  static let lg: CGFloat = 12
}

struct QuartermasterStatusStyle {
  let foreground: Color
  let background: Color
  let border: Color

  static let available = QuartermasterStatusStyle(
    foreground: QuartermasterBrand.successForeground,
    background: QuartermasterBrand.successBackground,
    border: QuartermasterBrand.successBorder
  )
  static let soon = QuartermasterStatusStyle(
    foreground: QuartermasterBrand.warningForeground,
    background: QuartermasterBrand.warningBackground,
    border: QuartermasterBrand.warningBorder
  )
  static let expired = QuartermasterStatusStyle(
    foreground: QuartermasterBrand.beet,
    background: QuartermasterBrand.dangerBackground,
    border: QuartermasterBrand.dangerBorder
  )
  static let expiredStrong = QuartermasterStatusStyle(
    foreground: .white,
    background: QuartermasterBrand.beetStrong,
    border: QuartermasterBrand.beetStrong
  )
  static let info = QuartermasterStatusStyle(
    foreground: QuartermasterBrand.infoForeground,
    background: QuartermasterBrand.infoBackground,
    border: QuartermasterBrand.infoBorder
  )
  static let neutral = QuartermasterStatusStyle(
    foreground: QuartermasterBrand.neutralForeground,
    background: QuartermasterBrand.neutralBackground,
    border: QuartermasterBrand.neutralBorder
  )
  static let frozen = QuartermasterStatusStyle(
    foreground: QuartermasterBrand.frozenForeground,
    background: QuartermasterBrand.frozenBackground,
    border: QuartermasterBrand.frozenBorder
  )
}

extension Color {
  static var quartermasterTextPrimary: Color {
    Color(
      UIColor { traits in
        traits.userInterfaceStyle == .dark
          ? UIColor(red: 0xF2 / 255, green: 0xF5 / 255, blue: 0xEF / 255, alpha: 1)
          : UIColor(red: 0x18 / 255, green: 0x20 / 255, blue: 0x1C / 255, alpha: 1)
      })
  }

  static var quartermasterTextSecondary: Color {
    Color(
      UIColor { traits in
        traits.userInterfaceStyle == .dark
          ? UIColor(red: 0xC7 / 255, green: 0xD0 / 255, blue: 0xC7 / 255, alpha: 1)
          : UIColor(red: 0x33 / 255, green: 0x40 / 255, blue: 0x39 / 255, alpha: 1)
      })
  }

  static var quartermasterTextMuted: Color {
    Color(
      UIColor { traits in
        traits.userInterfaceStyle == .dark
          ? UIColor(red: 0x9E / 255, green: 0xA9 / 255, blue: 0xA2 / 255, alpha: 1)
          : UIColor(red: 0x66 / 255, green: 0x71 / 255, blue: 0x6B / 255, alpha: 1)
      })
  }

  static var quartermasterAppSurface: Color {
    Color(
      UIColor { traits in
        traits.userInterfaceStyle == .dark
          ? UIColor(red: 0x10 / 255, green: 0x17 / 255, blue: 0x13 / 255, alpha: 1)
          : UIColor(red: 0xF7 / 255, green: 0xF8 / 255, blue: 0xF4 / 255, alpha: 1)
      })
  }

  static var quartermasterPanelSurface: Color {
    Color(
      UIColor { traits in
        traits.userInterfaceStyle == .dark
          ? UIColor(red: 0x17 / 255, green: 0x21 / 255, blue: 0x1B / 255, alpha: 1)
          : UIColor.white
      })
  }

  static var quartermasterSubtleSurface: Color {
    Color(
      UIColor { traits in
        traits.userInterfaceStyle == .dark
          ? UIColor(red: 0x1F / 255, green: 0x2D / 255, blue: 0x25 / 255, alpha: 1)
          : UIColor(red: 0xE8 / 255, green: 0xEE / 255, blue: 0xE8 / 255, alpha: 1)
      })
  }

  static var quartermasterBorder: Color {
    Color(
      UIColor { traits in
        traits.userInterfaceStyle == .dark
          ? UIColor(red: 0x2D / 255, green: 0x3A / 255, blue: 0x32 / 255, alpha: 1)
          : UIColor(red: 0xD9 / 255, green: 0xDE / 255, blue: 0xD6 / 255, alpha: 1)
      })
  }

  static var quartermasterTint: Color {
    Color(
      UIColor { traits in
        traits.userInterfaceStyle == .dark
          ? UIColor(red: 0x82 / 255, green: 0xB9 / 255, blue: 0x9A / 255, alpha: 1)
          : UIColor(red: 0x23 / 255, green: 0x4A / 255, blue: 0x35 / 255, alpha: 1)
      })
  }

  static var quartermasterError: Color {
    Color(
      UIColor { traits in
        traits.userInterfaceStyle == .dark
          ? UIColor(red: 0xFF / 255, green: 0xB1 / 255, blue: 0xB8 / 255, alpha: 1)
          : UIColor(red: 0x9B / 255, green: 0x2F / 255, blue: 0x2F / 255, alpha: 1)
      })
  }
}

extension View {
  func quartermasterScreenBackground() -> some View {
    self
      .scrollContentBackground(.hidden)
      .background(Color.quartermasterAppSurface)
  }

  func quartermasterPanelRow() -> some View {
    self
      .listRowBackground(Color.quartermasterPanelSurface)
      .listRowSeparatorTint(Color.quartermasterBorder)
  }

  func quartermasterMetadata() -> some View {
    self.foregroundStyle(Color.quartermasterTextMuted)
  }
}
