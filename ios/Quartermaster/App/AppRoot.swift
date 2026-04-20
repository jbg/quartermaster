import SwiftUI

struct AppRoot: View {
    @Environment(AppState.self) private var appState

    var body: some View {
        switch appState.phase {
        case .launching:
            LaunchView()
        case .unauthenticated:
            OnboardingView()
        case .authenticated:
            MainTabView()
        }
    }
}

private struct LaunchView: View {
    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "fork.knife")
                .font(.system(size: 56, weight: .regular))
            Text("Quartermaster")
                .font(.title.weight(.semibold))
            ProgressView()
                .padding(.top, 24)
        }
        .foregroundStyle(.primary)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.background)
    }
}
