import SwiftUI

@main
struct BrickApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup("Brick", id: "main") {
            RootView()
                .environment(model)
                .frame(minWidth: 860, minHeight: 480)
        }
        .commands {
            MailCommands(model: model)
        }

        WindowGroup("New Message", id: "compose", for: ComposeDraft.self) { $draft in
            ComposeView(draft: draft ?? model.newDraft())
                .environment(model)
        }
        .defaultSize(width: 640, height: 520)

        Settings {
            TabView {
                AccountsSettingsView()
                    .tabItem { Label("Accounts", systemImage: "person.crop.circle") }
                ClaudeSettingsView()
                    .tabItem { Label("Claude", systemImage: "sparkles") }
            }
            .environment(model)
        }
    }
}

struct MailCommands: Commands {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New Message") { openWindow(id: "compose", value: model.newDraft()) }
                .keyboardShortcut("n")
                .disabled(model.accounts.isEmpty)
        }
        CommandGroup(after: .importExport) {
            Button("Import Calendar…") { model.importCalendar() }
            Button("Export Calendar…") { model.exportCalendar() }
        }
        CommandGroup(before: .toolbar) {
            Button("Mail") { model.tab = .mail }
                .keyboardShortcut("1")
            Button("To-do") { model.tab = .todo }
                .keyboardShortcut("2")
            Button("Calendar") { model.tab = .calendar }
                .keyboardShortcut("3")
            Toggle(
                "Group into Conversations",
                isOn: Binding(get: { model.threaded }, set: { model.threaded = $0 }))
            Divider()
        }
        CommandMenu("Message") {
            Button("Add to To-do with Claude") { model.extractItems() }
                .keyboardShortcut("t", modifiers: [.command, .shift])
                .disabled(model.tab != .mail || model.detail?.body == nil || model.extractingMessageId != nil)
            Divider()
            Button("Reply") { open(.reply) }
                .keyboardShortcut("r")
                .disabled(model.selectedRow == nil)
            Button("Reply All") { open(.replyAll) }
                .keyboardShortcut("r", modifiers: [.command, .shift])
                .disabled(model.selectedRow == nil)
            Button("Forward") { open(.forward) }
                .keyboardShortcut("f", modifiers: [.command, .shift])
                .disabled(model.selectedRow == nil)
            Divider()
            Button("Next Message") { model.step(1) }
                .keyboardShortcut("]")
                .disabled(model.tab != .mail)
            Button("Previous Message") { model.step(-1) }
                .keyboardShortcut("[")
                .disabled(model.tab != .mail)
            Divider()
            Button("Archive") { model.moveSelection(to: "archive") }
                .keyboardShortcut("a", modifiers: [.command, .control])
                .disabled(model.selection.isEmpty)
            // Plain Delete is handled by the message list itself, so that it
            // never fires while typing in a text field.
            Button("Delete") { model.moveSelection(to: "trash") }
                .keyboardShortcut(.delete, modifiers: .command)
                .disabled(model.selection.isEmpty)
            Divider()
            Button(model.selectedRow?.hasUnread == true ? "Mark as Read" : "Mark as Unread") { model.toggleRead() }
                .keyboardShortcut("u", modifiers: [.command, .shift])
                .disabled(model.selection.isEmpty)
            Button(model.selectedRow?.flagged == true ? "Remove Flag" : "Flag") { model.toggleStar() }
                .keyboardShortcut("l", modifiers: [.command, .shift])
                .disabled(model.selection.isEmpty)
        }
        CommandGroup(after: .toolbar) {
            Button("Get New Mail") { model.syncNow() }
                .keyboardShortcut("n", modifiers: [.command, .shift])
        }
    }

    private func open(_ kind: AppModel.ReplyKind) {
        if let draft = model.draft(kind) {
            openWindow(id: "compose", value: draft)
        }
    }
}
