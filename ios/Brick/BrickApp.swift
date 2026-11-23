import BackgroundTasks
import SwiftUI

@main
struct BrickApp: App {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .active:
                // The engine's two-minute poll stops while the app is suspended;
                // catch up as soon as it is back on screen.
                model.syncNow()
            case .background:
                BackgroundRefresh.schedule()
            default:
                break
            }
        }
        .backgroundTask(.appRefresh(BackgroundRefresh.identifier)) {
            await BackgroundRefresh.run(model)
        }
    }
}

/// Occasional mail checks while Brick is in the background. iOS decides when
/// (and whether) they run, and allows about 30 seconds each.
enum BackgroundRefresh {
    static let identifier = "st.mm.fgb.brick.refresh"

    static func schedule() {
        let request = BGAppRefreshTaskRequest(identifier: identifier)
        request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
        try? BGTaskScheduler.shared.submit(request)
    }

    @MainActor
    static func run(_ model: AppModel) async {
        schedule()
        await model.syncAndWait(timeout: .seconds(25))
    }
}

extension AppModel {
    /// Starts a sync and returns once every account has finished, or at the timeout.
    func syncAndWait(timeout: Duration = .seconds(30)) async {
        syncNow()
        let clock = ContinuousClock()
        let deadline = clock.now + timeout
        // sync_started arrives from the engine a moment after the request.
        try? await Task.sleep(for: .milliseconds(500))
        while syncing > 0, clock.now < deadline {
            try? await Task.sleep(for: .milliseconds(300))
        }
    }
}

/// The three tabs, with the shared sheets and banners on top.
struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model

        Group {
            if let error = model.startupError {
                ContentUnavailableView(
                    "Brick couldn't open its mail store", systemImage: "exclamationmark.triangle",
                    description: Text(error))
            } else {
                TabView(selection: $model.tab) {
                    Tab("Mail", systemImage: "envelope", value: AppTab.mail) {
                        // Banners go on each column: the split view draws over an inset around it.
                        MailView()
                    }
                    Tab("To-do", systemImage: "checklist", value: AppTab.todo) {
                        TodoView().banners()
                    }
                    Tab("Calendar", systemImage: "calendar", value: AppTab.calendar) {
                        CalendarView().banners()
                    }
                }
            }
        }
        .sheet(item: $model.editingItem) { item in
            ItemEditor(item: item)
        }
    }
}

private struct Banners: ViewModifier {
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        content.safeAreaInset(edge: .bottom, spacing: 0) {
            if let banner = model.banner {
                BannerView(text: banner) { model.banner = nil }
            } else if let notice = model.notice {
                BannerView(text: notice, symbol: "checkmark.circle.fill", tint: .green) { model.notice = nil }
            }
        }
    }
}

extension View {
    /// Errors and passing notices, just above the tab bar.
    func banners() -> some View { modifier(Banners()) }
}

/// Opens Settings from any tab's toolbar.
struct SettingsButton: View {
    @State private var showing = false

    var body: some View {
        Button("Settings", systemImage: "gearshape") { showing = true }
            .sheet(isPresented: $showing) { SettingsView() }
    }
}

/// Wraps the system share sheet, for files made on demand.
struct ShareSheet: UIViewControllerRepresentable {
    let url: URL

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: [url], applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

extension URL: @retroactive Identifiable {
    public var id: String { absoluteString }
}
