import SwiftUI

/// The window's content: one of the three tabs, with the switcher in the toolbar.
struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model

        Group {
            switch model.tab {
            case .mail: MainView()
            case .todo: TodoView()
            case .calendar: CalendarView()
            }
        }
        .toolbar {
            ToolbarItem(placement: .principal) {
                Picker("View", selection: $model.tab) {
                    ForEach(AppTab.allCases) { tab in
                        Text(tab.title).tag(tab)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .help("Switch between mail, to-do and calendar")
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let banner = model.banner {
                BannerView(text: banner) { model.banner = nil }
            } else if let notice = model.notice {
                BannerView(text: notice, symbol: "checkmark.circle.fill", tint: .green) { model.notice = nil }
            }
        }
        .sheet(item: $model.editingItem) { item in
            ItemEditor(item: item)
        }
        .sheet(item: $model.extractionResult) { result in
            ExtractionSheet(result: result)
        }
    }
}

struct MainView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        @Bindable var model = model

        NavigationSplitView {
            SidebarView()
                .navigationSplitViewColumnWidth(min: 180, ideal: 220, max: 320)
        } content: {
            MessageListView()
                .navigationSplitViewColumnWidth(min: 280, ideal: 360, max: 560)
        } detail: {
            ReaderView()
        }
        .searchable(text: $model.searchText, placement: .toolbar, prompt: "Search all mail")
        .toolbar {
            ToolbarItemGroup(placement: .navigation) {
                Button("Get Mail", systemImage: "arrow.clockwise") { model.syncNow() }
                    .help("Get new mail")
                Toggle("Conversations", systemImage: "text.bubble", isOn: $model.threaded)
                    .help(model.threaded ? "Show every message separately" : "Group messages into conversations")
            }
            ToolbarItemGroup(placement: .primaryAction) {
                Button("New Message", systemImage: "square.and.pencil") {
                    openWindow(id: "compose", value: model.newDraft())
                }
                .disabled(model.accounts.isEmpty)
                .help("Write a new message")

                Button("Reply", systemImage: "arrowshape.turn.up.left") { compose(.reply) }
                    .disabled(model.selectedRow == nil)
                    .help("Reply to the sender")
                Button("Reply All", systemImage: "arrowshape.turn.up.left.2") { compose(.replyAll) }
                    .disabled(model.selectedRow == nil)
                    .help("Reply to everyone")
                Button("Forward", systemImage: "arrowshape.turn.up.right") { compose(.forward) }
                    .disabled(model.selectedRow == nil)
                    .help("Forward this message")

                Button("Archive", systemImage: "archivebox") { model.moveSelection(to: "archive") }
                    .disabled(model.selection.isEmpty)
                    .help("Archive")
                Button("Delete", systemImage: "trash") { model.moveSelection(to: "trash") }
                    .disabled(model.selection.isEmpty)
                    .help("Move to Trash")
            }
        }
        .navigationTitle(
            model.isSearching ? "Search" : (model.isUnified ? "All Inboxes" : (model.folder(model.selectedFolder)?.name ?? "Brick")))
        .navigationSubtitle(subtitle)
        .overlay {
            if let error = model.startupError {
                ContentUnavailableView(
                    "Brick couldn't open its mail store", systemImage: "exclamationmark.triangle",
                    description: Text(error))
                    .background(.background)
            } else if model.accounts.isEmpty {
                ContentUnavailableView {
                    Label("Add your first account", systemImage: "envelope")
                } description: {
                    Text("Brick works with Fastmail, Gmail, Outlook, and other IMAP accounts.")
                } actions: {
                    Button("Add account") { openSettings() }
                        .buttonStyle(.borderedProminent)
                }
                .background(.background)
            }
        }
    }

    private var subtitle: String {
        if model.syncing > 0 { return "Checking mail…" }
        if model.isSearching { return "\(model.total) results" }
        if model.isUnified {
            let unread = model.folders.values.joined().filter { $0.role == "inbox" }.reduce(0) { $0 + $1.unread }
            let noun = model.threaded ? "conversations" : "messages"
            return unread > 0 ? "\(model.total) \(noun), \(unread) unread" : "\(model.total) \(noun)"
        }
        if let folder = model.folder(model.selectedFolder) {
            return folder.unread > 0 ? "\(folder.total) messages, \(folder.unread) unread" : "\(folder.total) messages"
        }
        return ""
    }

    private func compose(_ kind: AppModel.ReplyKind) {
        if let draft = model.draft(kind) {
            openWindow(id: "compose", value: draft)
        }
    }
}

struct SidebarView: View {
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
    }
}

struct MessageListView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model

        List(model.rows, selection: $model.selection) { row in
            MessageRowView(row: row)
                .onAppear {
                    // Rows are loaded a page at a time as the list is scrolled.
                    if row.id == model.rows.last?.id { model.loadMore() }
                }
        }
        .listStyle(.inset)
        .contextMenu(forSelectionType: Int64.self) { ids in
            if !ids.isEmpty {
                Button("Mark as Read") { model.setFlags(model.expanded(ids), seen: true) }
                Button("Mark as Unread") { model.setFlags(model.expanded(ids), seen: false) }
                Button("Flag") { model.setFlags(model.expanded(ids), flagged: true) }
                Button("Remove Flag") { model.setFlags(model.expanded(ids), flagged: false) }
                Divider()
                Button("Archive") {
                    model.selection = ids
                    model.moveSelection(to: "archive")
                }
                Button("Delete", role: .destructive) {
                    model.selection = ids
                    model.moveSelection(to: "trash")
                }
            }
        }
        .onDeleteCommand { model.moveSelection(to: "trash") }
        // J and K, as in most mail clients. Handled here, not as menu shortcuts,
        // so they never fire while typing in a text field.
        .onKeyPress("j") {
            model.step(1)
            return .handled
        }
        .onKeyPress("k") {
            model.step(-1)
            return .handled
        }
        .overlay {
            if model.rows.isEmpty && !model.accounts.isEmpty {
                if model.isSearching {
                    ContentUnavailableView.search(text: model.searchText)
                } else {
                    ContentUnavailableView("No messages", systemImage: "tray")
                }
            }
        }
    }
}

