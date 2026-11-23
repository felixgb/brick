import QuickLook
import SwiftUI
import WebKit

struct ReaderView: View {
    @Environment(AppModel.self) private var model
    @Binding var composing: ComposeDraft?
    @State private var showThread = false
    @State private var showAllHeaders = false
    @State private var preview: URL?

    var body: some View {
        if let detail = model.detail {
            VStack(spacing: 0) {
                header(detail)
                Divider()
                if model.thread.count > 1 {
                    threadStrip(detail)
                    Divider()
                }
                if let body = detail.body {
                    if body.hasInvitation {
                        bar("This email has a calendar invitation.", action: "Add") { model.addInvitation() }
                        Divider()
                    }
                    if body.blockedRemote {
                        bar("Remote images are hidden.", action: "Show") { model.showRemoteImages() }
                        Divider()
                    }
                    MessageWebView(html: body.html)
                } else {
                    ProgressView("Loading…")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                Divider()
                actionBar(detail.message)
            }
            .navigationBarTitleDisplayMode(.inline)
            // The message's own actions take the tab bar's place. (A .bottomBar
            // toolbar doesn't show in the collapsed split view under a TabView.)
            .toolbar(.hidden, for: .tabBar)
            .toolbar { toolbar(detail) }
            .quickLookPreview($preview)
        } else {
            ContentUnavailableView("No message selected", systemImage: "envelope")
        }
    }

    @ToolbarContentBuilder
    private func toolbar(_ detail: MessageDetail) -> some ToolbarContent {
        let message = detail.message
        ToolbarItem(placement: .topBarTrailing) {
            Button(message.seen ? "Mark as Unread" : "Mark as Read",
                   systemImage: message.seen ? "envelope.badge" : "envelope.open") {
                model.setFlags([message.id], seen: !message.seen)
                // The open message keeps showing; reflect the change in its header.
                model.reloadOpenMessage()
            }
        }
    }

    private func actionBar(_ message: MessageRow) -> some View {
        HStack {
            Button("Flag", systemImage: message.flagged ? "flag.fill" : "flag") {
                model.setFlags([message.id], flagged: !message.flagged)
                model.reloadOpenMessage()
            }
            Spacer()
            Button("Archive", systemImage: "archivebox") { model.moveSelection(to: "archive") }
            Spacer()
            Button("Delete", systemImage: "trash") { model.moveSelection(to: "trash") }
            Spacer()
            Menu("Reply", systemImage: "arrowshape.turn.up.left") {
                Button("Reply", systemImage: "arrowshape.turn.up.left") { composing = model.draft(.reply) }
                Button("Reply All", systemImage: "arrowshape.turn.up.left.2") { composing = model.draft(.replyAll) }
                Button("Forward", systemImage: "arrowshape.turn.up.right") { composing = model.draft(.forward) }
            }
        }
        .labelStyle(.iconOnly)
        .font(.title3)
        .padding(.horizontal, 28)
        .padding(.vertical, 10)
        .background(.bar)
    }

    private func header(_ detail: MessageDetail) -> some View {
        let message = detail.message
        return VStack(alignment: .leading, spacing: 4) {
            Text(message.displaySubject)
                .font(.title3.weight(.semibold))
                .textSelection(.enabled)
            HStack(alignment: .firstTextBaseline) {
                Text(message.sender).fontWeight(.medium).lineLimit(1)
                Spacer()
                Text(message.when.formatted(date: .abbreviated, time: .shortened))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            Group {
                if showAllHeaders {
                    Text(message.fullSender)
                    Text("To: \(message.to)")
                    if !detail.cc.isEmpty { Text("Cc: \(detail.cc)") }
                } else {
                    Text("To: \(message.to)").lineLimit(1)
                }
            }
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .textSelection(.enabled)
            .onTapGesture { showAllHeaders.toggle() }

            if let attachments = detail.body?.attachments, !attachments.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack {
                        ForEach(attachments) { attachment in
                            Button {
                                open(attachment, of: message.id)
                            } label: {
                                Label(
                                    "\(attachment.name) (\(Int64(attachment.size).formatted(.byteCount(style: .file))))",
                                    systemImage: "paperclip")
                                .lineLimit(1)
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                        }
                    }
                }
                .padding(.top, 2)
            }
            if !model.linkedItems.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack {
                        ForEach(model.linkedItems) { item in
                            Button {
                                model.editingItem = item
                            } label: {
                                Label(item.title, systemImage: item.done ? "checkmark.circle.fill" : "circle")
                                    .lineLimit(1)
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    /// Saves the attachment to a temporary file and shows it in Quick Look,
    /// which also offers Share and Save to Files.
    private func open(_ attachment: Attachment, of messageId: Int64) {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("attachments/\(messageId)", isDirectory: true)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        var name = attachment.name.replacingOccurrences(of: "/", with: "-")
        if name.isEmpty { name = "attachment" }
        let url = folder.appendingPathComponent(name)
        if model.saveAttachment(attachment, to: url) {
            preview = url
        }
    }

    /// The other messages of this conversation.
    private func threadStrip(_ detail: MessageDetail) -> some View {
        VStack(spacing: 0) {
            Button {
                withAnimation { showThread.toggle() }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: showThread ? "chevron.down" : "chevron.right")
                        .font(.caption)
                        .frame(width: 10)
                    Text("Conversation, \(model.thread.count) messages")
                    Spacer()
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .font(.subheadline)
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            if showThread {
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(model.thread) { message in
                            let current = message.id == detail.message.id
                            Button {
                                model.openInThread(message.id)
                            } label: {
                                HStack(alignment: .firstTextBaseline, spacing: 8) {
                                    Circle()
                                        .fill(message.seen ? Color.clear : Color.accentColor)
                                        .frame(width: 7, height: 7)
                                    Text(message.sender)
                                        .fontWeight(current ? .semibold : .regular)
                                        .lineLimit(1)
                                    Spacer(minLength: 8)
                                    Text(MessageRowView.short(message.when))
                                        .foregroundStyle(.secondary)
                                }
                                .font(.subheadline)
                                .padding(.horizontal, 16)
                                .padding(.vertical, 6)
                                .background(current ? Color.accentColor.opacity(0.15) : Color.clear)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .frame(maxHeight: 180)
            }
        }
        .background(.bar)
    }

    private func bar(_ text: String, action: String, perform: @escaping () -> Void) -> some View {
        HStack {
            Text(text).foregroundStyle(.secondary)
            Spacer()
            Button(action, action: perform)
        }
        .font(.subheadline)
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
        .background(.bar)
    }
}

extension AppModel {
    /// Re-reads the open message so its header shows flag changes made from the reader.
    func reloadOpenMessage() {
        guard let id = detail?.message.id else { return }
        if let loaded: MessageDetail = try? core?.call("messages.get", ["id": id, "allow_remote": allowRemote]) {
            detail = loaded
        }
    }
}

/// Shows one message body. JavaScript is off, nothing is persisted, and links
/// open outside the app. The engine's Content-Security-Policy blocks every
/// network load except, once the user opts in, images.
struct MessageWebView: UIViewRepresentable {
    let html: String

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        configuration.dataDetectorTypes = [.link, .phoneNumber]
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = context.coordinator
        view.allowsBackForwardNavigationGestures = false
        view.allowsLinkPreview = false
        return view
    }

    func updateUIView(_ view: WKWebView, context: Context) {
        guard context.coordinator.shown != html else { return }
        context.coordinator.shown = html
        view.loadHTMLString(Self.fitToScreen(html), baseURL: nil)
    }

    /// Mail is written for wide windows; scale it to the phone's width.
    private static func fitToScreen(_ html: String) -> String {
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">" + html
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var shown: String?

        func webView(
            _ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
            decisionHandler: @escaping @MainActor (WKNavigationActionPolicy) -> Void
        ) {
            guard navigationAction.navigationType == .linkActivated else {
                decisionHandler(.allow)
                return
            }
            if let url = navigationAction.request.url, ["https", "http", "mailto", "tel"].contains(url.scheme ?? "") {
                openExternally(url)
            }
            decisionHandler(.cancel)
        }
    }
}
