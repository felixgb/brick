import SwiftUI

@main
struct BrickApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup("Brick", id: "main") {
            MainView()
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
            AccountsSettingsView()
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
        CommandMenu("Message") {
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
            Button("Archive") { model.moveSelection(to: "archive") }
                .keyboardShortcut("a", modifiers: [.command, .control])
                .disabled(model.selection.isEmpty)
            // Plain Delete is handled by the message list itself, so that it
            // never fires while typing in a text field.
            Button("Delete") { model.moveSelection(to: "trash") }
                .keyboardShortcut(.delete, modifiers: .command)
                .disabled(model.selection.isEmpty)
            Divider()
            Button(model.selectedRow?.seen == false ? "Mark as Read" : "Mark as Unread") { model.toggleRead() }
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
