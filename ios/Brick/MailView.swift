import SwiftUI

extension ComposeDraft: Identifiable {
    var id: String { token }
}

/// Mailboxes, then messages, then one message. On the iPhone the three
/// columns collapse into one stack that starts at the message list.
struct MailView: View {
    @Environment(AppModel.self) private var model
    @State private var column: NavigationSplitViewColumn = .content
    @State private var composing: ComposeDraft?

    var body: some View {
        NavigationSplitView(preferredCompactColumn: $column) {
            FolderListView().banners()
        } content: {
            MessageListView(composing: $composing).banners()
        } detail: {
            ReaderView(composing: $composing).banners()
        }
        .sheet(item: $composing) { draft in
            ComposeView(draft: draft)
        }
    }
}

struct FolderListView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model

        List(selection: $model.selectedFolder) {
            if model.accounts.count > 1 {
                Label("All Inboxes", systemImage: "tray.2")
                    .badge(model.folders.values.joined().filter { $0.role == "inbox" }.reduce(0) { $0 + $1.unread })
                    .tag(Optional(AppModel.allInboxes))
            }
            ForEach(model.accounts) { account in
                Section(account.email) {
                    ForEach(model.folders[account.id] ?? []) { folder in
                        Label(folder.name, systemImage: folder.symbol)
                            .badge(folder.unread)
                            .tag(Optional(folder.id))
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("Mailboxes")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { SettingsButton() }
        }
        .overlay {
            if model.accounts.isEmpty {
                NoAccountsView()
            }
        }
    }
}

struct NoAccountsView: View {
    @State private var adding = false

    var body: some View {
        ContentUnavailableView {
            Label("Add your first account", systemImage: "envelope")
        } description: {
            Text("Brick works with Fastmail and other IMAP accounts.")
        } actions: {
            Button("Add Account") { adding = true }
                .buttonStyle(.borderedProminent)
        }
        .sheet(isPresented: $adding) { AddAccountView() }
    }
}

struct MessageListView: View {
    @Environment(AppModel.self) private var model
    @Binding var composing: ComposeDraft?

    /// The list shows one message at a time; the model allows several.
    private var openMessage: Binding<Int64?> {
        Binding {
            model.selection.count == 1 ? model.selection.first : nil
        } set: { id in
            model.selection = id.map { [$0] } ?? []
        }
    }

    var body: some View {
        @Bindable var model = model

        List(model.rows, selection: openMessage) { row in
            MessageRowView(row: row)
                .onAppear {
                    // Rows are loaded a page at a time as the list is scrolled.
                    if row.id == model.rows.last?.id { model.loadMore() }
                }
                .swipeActions(edge: .trailing) {
                    Button("Delete", systemImage: "trash", role: .destructive) {
                        model.move([row.id], to: "trash")
                    }
                    Button("Archive", systemImage: "archivebox") {
                        model.move([row.id], to: "archive")
                    }
                    .tint(.indigo)
                }
                .swipeActions(edge: .leading) {
                    Button(
                        row.hasUnread ? "Read" : "Unread",
                        systemImage: row.hasUnread ? "envelope.open" : "envelope.badge"
                    ) {
                        model.setFlags(model.expanded([row.id]), seen: row.hasUnread)
                    }
                    .tint(.blue)
                    Button(row.flagged ? "Unflag" : "Flag", systemImage: row.flagged ? "flag.slash" : "flag") {
                        model.setFlags(model.expanded([row.id]), flagged: !row.flagged)
                    }
                    .tint(.orange)
                }
        }
        .listStyle(.plain)
        .searchable(text: $model.searchText, prompt: "Search all mail")
        .refreshable { await model.syncAndWait() }
        .navigationTitle(title)
        .navigationSubtitle(subtitle)
        .toolbar {
            ToolbarItemGroup(placement: .topBarTrailing) {
                Menu("View", systemImage: "line.3.horizontal.decrease") {
                    Toggle("Group into Conversations", isOn: $model.threaded)
                }
                Button("New Message", systemImage: "square.and.pencil") {
                    composing = model.newDraft()
                }
                .disabled(model.accounts.isEmpty)
            }
        }
        .overlay {
            if model.rows.isEmpty && !model.accounts.isEmpty {
                if model.isSearching {
                    ContentUnavailableView.search(text: model.searchText)
                } else {
                    ContentUnavailableView("No messages", systemImage: "tray")
                }
            } else if model.accounts.isEmpty {
                NoAccountsView()
            }
        }
    }

    private var title: String {
        if model.isSearching { return "Search" }
        if model.isUnified { return "All Inboxes" }
        return model.folder(model.selectedFolder)?.name ?? "Brick"
    }

    private var subtitle: String {
        if model.syncing > 0 { return "Checking mail…" }
        if model.isSearching { return "\(model.total) results" }
        if model.isUnified {
            let unread = model.folders.values.joined().filter { $0.role == "inbox" }.reduce(0) { $0 + $1.unread }
            return unread > 0 ? "\(unread) unread" : ""
        }
        if let folder = model.folder(model.selectedFolder), folder.unread > 0 {
            return "\(folder.unread) unread"
        }
        return ""
    }
}
