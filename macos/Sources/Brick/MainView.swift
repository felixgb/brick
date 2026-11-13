import SwiftUI

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
        .navigationTitle(model.isSearching ? "Search" : (model.folder(model.selectedFolder)?.name ?? "Brick"))
        .navigationSubtitle(subtitle)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let banner = model.banner {
                BannerView(text: banner) { model.banner = nil }
            }
        }
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

struct BannerView: View {
    let text: String
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.yellow)
            Text(text).lineLimit(2)
            Spacer()
            Button("Dismiss", systemImage: "xmark", action: dismiss)
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
        }
        .font(.callout)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
    }
}

struct SidebarView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model

        List(selection: $model.selectedFolder) {
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
                Button("Mark as Read") { model.setFlags(ids, seen: true) }
                Button("Mark as Unread") { model.setFlags(ids, seen: false) }
                Button("Flag") { model.setFlags(ids, flagged: true) }
                Button("Remove Flag") { model.setFlags(ids, flagged: false) }
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

struct MessageRowView: View {
    let row: MessageRow

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            Circle()
                .fill(row.seen ? Color.clear : Color.accentColor)
                .frame(width: 8, height: 8)
                .padding(.top, 5)
                .accessibilityLabel(row.seen ? "" : "Unread")

            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline) {
                    Text(row.sender)
                        .font(.headline)
                        .fontWeight(row.seen ? .regular : .semibold)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    if row.flagged {
                        Image(systemName: "flag.fill").foregroundStyle(.orange).font(.caption)
                    }
                    if row.hasAttachments {
                        Image(systemName: "paperclip").foregroundStyle(.secondary).font(.caption)
                    }
                    Text(Self.short(row.when))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                Text(row.displaySubject)
                    .font(.subheadline)
                    .lineLimit(1)
                if !row.snippet.isEmpty {
                    Text(row.snippet)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
        }
        .padding(.vertical, 3)
    }

    static func short(_ date: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(date) {
            return date.formatted(date: .omitted, time: .shortened)
        }
        if calendar.isDateInYesterday(date) {
            return "Yesterday"
        }
        if calendar.isDate(date, equalTo: .now, toGranularity: .year) {
            return date.formatted(.dateTime.day().month(.abbreviated))
        }
        return date.formatted(date: .numeric, time: .omitted)
    }
}
