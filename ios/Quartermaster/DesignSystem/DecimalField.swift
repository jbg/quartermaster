import SwiftUI

/// Decimal-pad `TextField` with a keyboard toolbar that adds the "Done"
/// button iOS's decimal pad doesn't ship with. Use everywhere the user
/// types a number (AddStock, EditBatch, Consume), so dismissing the
/// keyboard stays consistent across the app.
struct DecimalField: View {
  let title: String
  @Binding var text: String

  @FocusState private var focused: Bool

  var body: some View {
    TextField(title, text: $text)
      .keyboardType(.decimalPad)
      .focused($focused)
      .toolbar {
        if focused {
          ToolbarItemGroup(placement: .keyboard) {
            Spacer()
            Button("Done") { focused = false }
              .fontWeight(.semibold)
          }
        }
      }
  }
}
