import Foundation
import Observation
#if canImport(AppKit)
import AppKit
#elseif canImport(UIKit)
import UIKit
#endif

/// Opens a link outside the app: the browser, or the mail composer for mailto.
@MainActor
func openExternally(_ url: URL) {
    #if canImport(AppKit)
    NSWorkspace.shared.open(url)
    #elseif canImport(UIKit)
    UIApplication.shared.open(url)
    #endif
}

enum SendState: Equatable {
    case sending
    case sent
    case failed(String)
}

enum AddAccountState: Equatable {
    case idle
    case waiting(String)
    case failed(String)
    case added
}

/// All window state. Views read it; the engine's events update it.
@MainActor
@Observable
final class AppModel {
    let core: Core?
    let startupError: String?

    var accounts: [Account] = []
    var folders: [Int64: [Folder]] = [:]
    var providers: [Provider] = []

    var selectedFolder: Int64? {
        didSet { if selectedFolder != oldValue { folderChanged() } }
    }
    var rows: [MessageRow] = []
    var total = 0
    var selection: Set<Int64> = [] {
        didSet { if selection != oldValue { selectionChanged() } }
    }
    var searchText = "" {
        didSet { if searchText != oldValue { scheduleSearch() } }
    }

    /// Group the message list into conversations. Kept in config.toml ([ui] threaded);
    /// read from there in init.
    var threaded = true {
        didSet {
            guard threaded != oldValue else { return }
            setSetting("ui.threaded", threaded ? "true" : "false")
            selection = []
            loadMessages()
        }
    }

    var detail: MessageDetail?
    /// Every message in the open message's conversation, oldest first.
    var thread: [MessageRow] = []
    var allowRemote = false
    private var pendingBody: Int64?

    // To-do and calendar
    var tab: AppTab = .mail
    var items: [Item] = []
    var editingItem: Item?
    /// Items already made from the email on screen.
    var linkedItems: [Item] = []
    var claude: ClaudeStatus?
    /// The message Claude is reading right now, if any.
    var extractingMessageId: Int64?
    var extractionResult: ExtractionResult?
    private var extractionToken = ""

    var syncing = 0
    var banner: String?
    /// A passing, non-error message ("Added 3 items.").
    var notice: String?
    private var noticeTask: Task<Void, Never>?
    var sendStates: [String: SendState] = [:]
    var addAccountState: AddAccountState = .idle

    /// Stands in for a folder id: every account's inbox together.
    static let allInboxes: Int64 = -1

    private static let pageSize = 200
    private var searchTask: Task<Void, Never>?
    private var refreshTask: Task<Void, Never>?

    init() {
        do {
            let support = try FileManager.default.url(
                for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            #if os(iOS)
            // config.toml goes in Documents, which the Files app shows, so it can be
            // edited on the phone; the mail store stays out of sight in Application Support.
            if getenv("BRICK_CONFIG") == nil,
                let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first
            {
                setenv("BRICK_CONFIG", documents.appendingPathComponent("config.toml").path, 1)
            }
            #endif
            let core = try Core(dataDirectory: support.appendingPathComponent("Brick", isDirectory: true))
            self.core = core
            self.startupError = nil
            core.onEvent = { [weak self] event in
                MainActor.assumeIsolated { self?.handle(event) }
            }
            providers = (try? core.call("providers.list")) ?? []
            // Earlier versions kept this in UserDefaults; carry it over once.
            if let old = UserDefaults.standard.object(forKey: "threaded") as? Bool {
                if (try? core.send("settings.get", ["key": "ui.threaded"])) as? String == nil {
                    _ = try? core.send("settings.set", ["key": "ui.threaded", "value": old ? "true" : "false"])
                }
                UserDefaults.standard.removeObject(forKey: "threaded")
            }
            threaded = ((try? core.send("settings.get", ["key": "ui.threaded"])) as? String) != "false"
            reloadAccounts()
            reloadItems()
            refreshClaudeStatus()
        } catch {
            self.core = nil
            self.startupError = error.localizedDescription
        }
    }

    var isSearching: Bool { !searchText.trimmingCharacters(in: .whitespaces).isEmpty }

    var selectedRow: MessageRow? {
        guard selection.count == 1, let id = selection.first else { return nil }
        return rows.first { $0.id == id }
    }

    func account(_ id: Int64) -> Account? { accounts.first { $0.id == id } }

    func folder(_ id: Int64?) -> Folder? {
        guard let id else { return nil }
        return folders.values.joined().first { $0.id == id }
    }

    var isUnified: Bool { selectedFolder == Self.allInboxes }

    var currentAccountId: Int64 {
        selectedRow?.accountId ?? folder(selectedFolder)?.accountId ?? accounts.first?.id ?? 0
    }

    // MARK: Loading

    func reloadAccounts() {
        guard let core else { return }
        accounts = (try? core.call("accounts.list")) ?? []
        var all: [Int64: [Folder]] = [:]
        for account in accounts {
            let list: [Folder] = (try? core.call("folders.list", ["account_id": account.id])) ?? []
            all[account.id] = list.filter(\.selectable)
        }
        folders = all
        let unifiedStillMakesSense = isUnified && accounts.count > 1
        if !unifiedStillMakesSense && folder(selectedFolder) == nil {
            // Nothing open yet (first run, or the open folder went away): show an inbox.
            var inbox: Int64?
            for account in accounts where inbox == nil {
                inbox = all[account.id]?.first(where: { $0.role == "inbox" })?.id
            }
            selectedFolder = inbox
        }
    }

    private func folderChanged() {
        if isSearching { searchText = "" }
        selection = []
        loadMessages()
    }

    /// Re-reads the visible list from the local store.
    func loadMessages() {
        guard let core else { return }
        if isSearching {
            let query = searchText.trimmingCharacters(in: .whitespaces)
            let found: SearchResults? = try? core.call("messages.search", ["query": query, "limit": 500])
            rows = found?.rows ?? []
            total = rows.count
            return
        }
        // Keep as many rows loaded as were already on screen so the list doesn't jump.
        let want = max(Self.pageSize, rows.count)
        guard let params = listParams(offset: 0, limit: min(want, 1000)) else {
            rows = []
            total = 0
            return
        }
        let page: MessagePage? = try? core.call("messages.list", params)
        rows = page?.rows ?? []
        total = page?.total ?? 0
    }

    private func listParams(offset: Int, limit: Int) -> [String: Any]? {
        var params: [String: Any] = ["offset": offset, "limit": limit, "threaded": threaded]
        if isUnified {
            params["role"] = "inbox"
        } else if let folderId = selectedFolder {
            params["folder_id"] = folderId
        } else {
            return nil
        }
        return params
    }

    /// Called when the last loaded row scrolls into view.
    func loadMore() {
        guard let core, !isSearching, rows.count < total,
            let params = listParams(offset: rows.count, limit: Self.pageSize)
        else { return }
        let page: MessagePage? = try? core.call("messages.list", params)
        let known = Set(rows.map(\.id))
        rows.append(contentsOf: (page?.rows ?? []).filter { !known.contains($0.id) })
    }

    private func scheduleSearch() {
        searchTask?.cancel()
        searchTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled, let self else { return }
            self.selection = []
            self.loadMessages()
        }
    }

    /// Sync reports changes in bursts; redraw once per burst.
    private func scheduleRefresh() {
        refreshTask?.cancel()
        refreshTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(60))
            guard !Task.isCancelled, let self else { return }
            self.loadMessages()
            self.selection = self.selection.intersection(self.rows.map(\.id))
            self.reloadThread()
        }
    }

    // MARK: Reading

    private func selectionChanged() {
        guard let row = selectedRow else {
            detail = nil
            thread = []
            pendingBody = nil
            linkedItems = []
            return
        }
        if detail?.message.id == row.id { return }
        allowRemote = false
        open(row.id)
        if row.hasUnread {
            // Opening a conversation reads all of it.
            setFlags(expanded([row.id]), seen: true)
        }
    }

    private func open(_ id: Int64) {
        guard let core else { return }
        do {
            let loaded: MessageDetail = try core.call("messages.get", ["id": id, "allow_remote": allowRemote])
            detail = loaded
            pendingBody = loaded.body == nil ? id : nil
            reloadThread()
            reloadLinkedItems()
        } catch {
            detail = nil
            pendingBody = nil
        }
    }

    private func reloadThread() {
        guard let core, let id = detail?.message.id else {
            thread = []
            return
        }
        let found: ThreadMessages? = try? core.call("messages.thread", ["id": id])
        thread = found?.rows ?? []
    }

    /// Shows another message of the open conversation without changing the list selection.
    func openInThread(_ id: Int64) {
        guard detail?.message.id != id else { return }
        allowRemote = false
        open(id)
        if let row = thread.first(where: { $0.id == id }), !row.seen {
            setFlags([id], seen: true)
        }
    }

    /// With the list grouped into conversations, a row stands for every message
    /// of its conversation that the list covers; actions apply to all of them.
    func expanded(_ ids: Set<Int64>) -> Set<Int64> {
        guard threaded, !isSearching, let core else { return ids }
        var all = ids
        for id in ids {
            guard let row = rows.first(where: { $0.id == id }), row.messagesInThread > 1 else { continue }
            var params: [String: Any] = ["id": id]
            if isUnified { params["role"] = "inbox" } else { params["folder_id"] = row.folderId }
            let found: ThreadMessages? = try? core.call("messages.thread", params)
            all.formUnion((found?.rows ?? []).map(\.id))
        }
        return all
    }

    func showRemoteImages() {
        guard let id = detail?.message.id else { return }
        allowRemote = true
        open(id)
    }

    // MARK: Actions

    func setFlags(_ ids: Set<Int64>, seen: Bool? = nil, flagged: Bool? = nil) {
        guard let core, !ids.isEmpty else { return }
        var params: [String: Any] = ["ids": Array(ids)]
        if let seen { params["seen"] = seen }
        if let flagged { params["flagged"] = flagged }
        attempt { try core.send("messages.set_flags", params) }
    }

    func toggleRead() {
        guard !selection.isEmpty else { return }
        let hasUnread = rows.contains { selection.contains($0.id) && $0.hasUnread }
        setFlags(expanded(selection), seen: hasUnread)
    }

    func toggleStar() {
        guard let first = rows.first(where: { selection.contains($0.id) }) else { return }
        setFlags(expanded(selection), flagged: !first.flagged)
    }

    /// Selects the message `offset` rows from the selected one (J and K).
    func step(_ offset: Int) {
        guard !rows.isEmpty else { return }
        let current = rows.firstIndex { selection.contains($0.id) }
        let next = current.map { min(max($0 + offset, 0), rows.count - 1) } ?? (offset > 0 ? 0 : rows.count - 1)
        selection = [rows[next].id]
        if next >= rows.count - 1 { loadMore() }
    }

    /// Moves the selection to "archive" or "trash" and selects the next message.
    func moveSelection(to destination: String) {
        guard let core, !selection.isEmpty else { return }
        let ids = expanded(selection)
        let firstIndex = rows.firstIndex { selection.contains($0.id) } ?? 0
        guard attempt({ try core.send("messages.move", ["ids": Array(ids), "to": destination]) }) else { return }
        // The engine removed the rows locally already; show that immediately.
        refreshTask?.cancel()
        loadMessages()
        if rows.isEmpty {
            selection = []
        } else {
            selection = [rows[min(firstIndex, rows.count - 1)].id]
        }
    }

    /// Moves messages (whole conversations when grouped) without picking another
    /// one to show: for swipe actions on rows that aren't open.
    func move(_ ids: Set<Int64>, to destination: String) {
        guard let core, !ids.isEmpty else { return }
        let all = expanded(ids)
        guard attempt({ try core.send("messages.move", ["ids": Array(all), "to": destination]) }) else { return }
        refreshTask?.cancel()
        selection.subtract(all)
        loadMessages()
    }

    func syncNow() {
        attempt { try core?.send("sync.now") }
    }

    /// Writes an attachment of the open message to `url`.
    @discardableResult
    func saveAttachment(_ attachment: Attachment, to url: URL) -> Bool {
        guard let core, let id = detail?.message.id else { return false }
        return attempt { try core.send("attachments.save", ["id": id, "index": attachment.index, "path": url.path]) }
    }

    @discardableResult
    func attempt(_ work: () throws -> Void) -> Bool {
        do {
            try work()
            return true
        } catch {
            banner = error.localizedDescription
            return false
        }
    }

    // MARK: To-do and calendar

    func reloadItems() {
        items = (try? core?.call("items.list")) ?? []
        reloadLinkedItems()
    }

    private func reloadLinkedItems() {
        guard let id = detail?.message.id else {
            linkedItems = []
            return
        }
        linkedItems = (try? core?.call("items.for_message", ["id": id])) ?? []
    }

    func itemsOn(_ day: String) -> [Item] {
        items.filter { $0.occurs(on: day) }
    }

    /// Saves a new or edited item. Returns an error message on failure.
    @discardableResult
    func save(_ item: Item) -> String? {
        guard let core else { return "The mail store isn't open." }
        do {
            try core.send("items.save", ["item": item.payload])
            reloadItems()
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    func setDone(_ item: Item, _ done: Bool) {
        // A repeating item doesn't finish: it moves to its next occurrence, so
        // only the engine knows what to show. Anything else flips locally first
        // so the tick is instant.
        if !item.isRepeating, let index = items.firstIndex(where: { $0.id == item.id }) {
            items[index].done = done
        }
        attempt { try core?.send("items.set_done", ["id": item.id, "done": done]) }
        if item.isRepeating { reloadItems() }
    }

    func delete(_ item: Item) {
        attempt { try core?.send("items.delete", ["id": item.id]) }
        reloadItems()
    }

    func newItem(on day: String? = nil, at time: String? = nil) {
        editingItem = Item(date: day, startTime: time)
    }

    /// Shows a passing message for a few seconds.
    func notify(_ text: String) {
        notice = text
        noticeTask?.cancel()
        noticeTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(6))
            guard !Task.isCancelled else { return }
            self?.notice = nil
        }
    }

    /// Every item as the text of an iCalendar file.
    func calendarText() throws -> String {
        guard let core else { throw CoreError(message: "The mail store isn't open.") }
        return (try core.send("items.export_ics") as? String) ?? ""
    }

    /// Imports calendar files and reports what happened.
    func importCalendars(_ urls: [URL]) {
        guard let core else { return }
        var added = 0, skipped = 0, dropped = 0
        for url in urls {
            // Files picked on iOS are outside the sandbox until opened this way.
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            do {
                let text: String
                if let utf8 = try? String(contentsOf: url, encoding: .utf8) {
                    text = utf8
                } else {
                    text = try String(contentsOf: url, encoding: .isoLatin1)
                }
                let result: ImportResult = try core.call("items.import_ics", ["text": text])
                added += result.added
                skipped += result.skipped
                dropped += result.repeatsDropped
            } catch {
                banner = "\(url.lastPathComponent): \(error.localizedDescription)"
            }
        }
        reloadItems()
        notify(ImportResult(added: added, skipped: skipped, repeatsDropped: dropped).summary)
    }

    /// Adds the calendar invitation the open email carries.
    func addInvitation() {
        guard let core, let id = detail?.message.id else { return }
        do {
            let result: ImportResult = try core.call("items.import_invitation", ["id": id])
            reloadItems()
            notify(result.summary)
        } catch {
            banner = error.localizedDescription
        }
    }

    /// Shows the email an item was made from.
    func openSource(of item: Item) {
        guard let core else { return }
        guard let source: ItemSource = try? core.call("items.source", ["id": item.id]) else {
            banner = "The email this came from is no longer in Brick."
            return
        }
        editingItem = nil
        tab = .mail
        if isSearching { searchText = "" }
        selectedFolder = source.folderId
        loadMessages()
        // The message may be further down than the first page.
        var pages = 0
        while !rows.contains(where: { $0.id == source.messageId }), rows.count < total, pages < 25 {
            loadMore()
            pages += 1
        }
        selection = [source.messageId]
    }

    // MARK: Claude

    func refreshClaudeStatus() {
        claude = try? core?.call("claude.status")
    }

    func setClaudeKey(_ key: String) {
        attempt { try core?.send("claude.set_key", ["key": key]) }
        refreshClaudeStatus()
    }

    func setClaudeModel(_ model: String) {
        attempt { try core?.send("settings.set", ["key": "claude_model", "value": model]) }
        refreshClaudeStatus()
    }

    /// Sends the open email to Claude. The proposals arrive as an event and
    /// are shown for review; nothing is saved until the user confirms.
    func extractItems() {
        guard let core, let id = detail?.message.id, extractingMessageId == nil else { return }
        extractionToken = UUID().uuidString
        if attempt({ try core.send("items.extract", ["id": id, "token": extractionToken]) }) {
            extractingMessageId = id
        }
    }

    /// Saves the proposals the user kept.
    func add(_ proposals: [Item]) {
        let failures = proposals.compactMap { save($0) }
        if let first = failures.first {
            banner = failures.count == 1 ? first : "\(failures.count) items couldn't be added. \(first)"
        }
    }

    // MARK: Composing

    func newDraft() -> ComposeDraft {
        ComposeDraft(accountId: currentAccountId)
    }

    enum ReplyKind { case reply, replyAll, forward }

    func draft(_ kind: ReplyKind) -> ComposeDraft? {
        guard let row = detail?.message ?? selectedRow else { return nil }
        let body = detail?.message.id == row.id ? (detail?.body?.text ?? "") : ""
        let date = row.when.formatted(date: .long, time: .shortened)
        var draft = ComposeDraft(accountId: row.accountId)

        func prefixed(_ prefix: String) -> String {
            row.subject.lowercased().hasPrefix(prefix.lowercased()) ? row.subject : "\(prefix) \(row.subject)"
        }

        if kind == .forward {
            draft.subject = prefixed("Fwd:")
            draft.text = "\n\n-------- Forwarded message --------\nFrom: \(row.fullSender)\nDate: \(date)\n"
                + "Subject: \(row.subject)\nTo: \(row.to)\n\n\(body)"
            return draft
        }

        draft.replyToId = row.id
        draft.to = row.fullSender
        draft.subject = prefixed("Re:")
        let quoted = body.split(separator: "\n", omittingEmptySubsequences: false)
            .map { $0.hasPrefix(">") ? ">\($0)" : "> \($0)" }
            .joined(separator: "\n")
        draft.text = "\n\nOn \(date), \(row.fullSender) wrote:\n\(quoted)"

        if kind == .replyAll {
            let mine = account(row.accountId)?.email.lowercased() ?? ""
            let cc = detail?.message.id == row.id ? (detail?.cc ?? "") : ""
            draft.cc = "\(row.to),\(cc)"
                .split(separator: ",")
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty && (mine.isEmpty || !$0.lowercased().contains(mine)) }
                .joined(separator: ", ")
        }
        return draft
    }

    /// Returns an error message if the draft was rejected before sending.
    func send(_ draft: ComposeDraft) -> String? {
        guard let core else { return "The mail store isn't open." }
        var params: [String: Any] = [
            "account_id": draft.accountId, "to": draft.to, "cc": draft.cc,
            "subject": draft.subject, "text": draft.text, "token": draft.token,
        ]
        if let replyTo = draft.replyToId { params["reply_to_id"] = replyTo }
        if let from = draft.from { params["from"] = from }
        do {
            try core.send("send", params)
            sendStates[draft.token] = .sending
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    // MARK: Accounts

    func setting(_ key: String) -> String {
        ((try? core?.send("settings.get", ["key": key])) as? String) ?? ""
    }

    func setSetting(_ key: String, _ value: String) {
        attempt { try core?.send("settings.set", ["key": key, "value": value]) }
    }

    /// Extra addresses an account may send as.
    func aliases(for accountId: Int64) -> [String] {
        setting("aliases_\(accountId)")
            .split(whereSeparator: { ",; \n\t".contains($0) })
            .map { String($0) }
    }

    func signature(for accountId: Int64) -> String {
        setting("signature_\(accountId)").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Everything the From picker offers.
    func fromChoices() -> [FromChoice] {
        accounts.flatMap { account -> [FromChoice] in
            func label(_ address: String) -> String {
                account.name.isEmpty ? address : "\(account.name) <\(address)>"
            }
            return [FromChoice(accountId: account.id, alias: nil, label: label(account.email))]
                + aliases(for: account.id).map {
                    FromChoice(accountId: account.id, alias: $0, label: label($0))
                }
        }
    }

    func addAccount(provider: Provider, fields: [String: Any], clientId: String, clientSecret: String) {
        guard let core else { return }
        do {
            if let oauth = provider.oauth {
                try core.send("settings.set", ["key": oauth.clientIdKey, "value": clientId])
                try core.send("settings.set", ["key": oauth.clientSecretKey, "value": clientSecret])
                let start: OAuthStart = try core.call("accounts.oauth", fields)
                if let url = URL(string: start.authUrl) {
                    openExternally(url)
                }
                addAccountState = .waiting("Finish signing in in your browser…")
            } else {
                try core.send("accounts.add", fields)
                addAccountState = .waiting("Checking with the server…")
            }
        } catch {
            addAccountState = .failed(error.localizedDescription)
        }
    }

    func removeAccount(_ id: Int64) {
        attempt { try core?.send("accounts.remove", ["id": id]) }
    }

    // MARK: Events

    private func handle(_ event: [String: Any]) {
        let type = event["type"] as? String ?? ""
        let folderId = (event["folder_id"] as? NSNumber)?.int64Value
        let messageId = (event["id"] as? NSNumber)?.int64Value
        let message = event["message"] as? String ?? ""

        switch type {
        case "messages_changed":
            if isSearching || isUnified || folderId == selectedFolder { scheduleRefresh() }
        case "folders_changed", "accounts_changed":
            reloadAccounts()
        case "body_ready":
            if let messageId, messageId == pendingBody { open(messageId) }
            if folderId == selectedFolder { scheduleRefresh() }
        case "body_error":
            if messageId == pendingBody {
                pendingBody = nil
                banner = "This message couldn't be downloaded. \(message)"
            }
        case "sync_started":
            syncing += 1
        case "sync_finished":
            syncing = max(0, syncing - 1)
        case "send_done":
            if let token = event["token"] as? String { sendStates[token] = .sent }
        case "send_error":
            if let token = event["token"] as? String { sendStates[token] = .failed(message) }
        case "account_added":
            if case .waiting = addAccountState { addAccountState = .added }
        case "account_error":
            if case .waiting = addAccountState { addAccountState = .failed(message) }
        case "items_changed":
            reloadItems()
        case "items_extracted":
            guard event["token"] as? String == extractionToken else { break }
            extractingMessageId = nil
            let found: [Item] = Core.decode(event["items"]) ?? []
            extractionResult = ExtractionResult(
                subject: found.first?.sourceSubject ?? detail?.message.displaySubject ?? "",
                summary: event["summary"] as? String ?? "",
                truncated: event["truncated"] as? Bool ?? false,
                proposals: found.map { Proposal(item: $0) })
        case "items_extract_error":
            guard event["token"] as? String == extractionToken else { break }
            extractingMessageId = nil
            banner = message
        case "error":
            banner = message
        default:
            break
        }
    }
}
