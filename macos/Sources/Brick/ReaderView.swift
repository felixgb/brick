import SwiftUI
import WebKit

struct ReaderView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.openSettings) private var openSettings
    @State private var showThread = false

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
                        invitationBar
                        Divider()
                    }
                    if body.blockedRemote {
                        remoteImagesBar
                        Divider()
                    }
                    MessageWebView(html: body.html)
                } else {
                    ProgressView("Loading…")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .background(.background)
        } else if model.selection.count > 1 {
            ContentUnavailableView("\(model.selection.count) messages selected", systemImage: "envelope.open")
        } else {
            ContentUnavailableView("No message selected", systemImage: "envelope")
        }
    }

    private func header(_ detail: MessageDetail) -> some View {
        let message = detail.message
        return VStack(alignment: .leading, spacing: 6) {
            Text(message.displaySubject)
                .font(.title2.weight(.semibold))
                .textSelection(.enabled)
            HStack(alignment: .firstTextBaseline) {
                Text(message.fullSender).fontWeight(.medium)
                Spacer()
                Text(message.when.formatted(date: .abbreviated, time: .shortened))
                    .foregroundStyle(.secondary)
            }
            .textSelection(.enabled)
            Text("To: \(message.to)")
                .foregroundStyle(.secondary)
                .lineLimit(2)
                .textSelection(.enabled)
            if !detail.cc.isEmpty {
                Text("Cc: \(detail.cc)")
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .textSelection(.enabled)
            }
            if let attachments = detail.body?.attachments, !attachments.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack {
                        ForEach(attachments) { attachment in
                            Button {
                                model.saveAttachment(attachment)
                            } label: {
                                Label(
                                    "\(attachment.name) (\(Int64(attachment.size).formatted(.byteCount(style: .file))))",
                                    systemImage: "paperclip")
                            }
                            .help("Save \(attachment.name)")
                        }
                    }
                }
                .padding(.top, 2)
            }
            itemsBar(detail)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    /// The path from this email to the to-do list and calendar.
    private func itemsBar(_ detail: MessageDetail) -> some View {
        let working = model.extractingMessageId == detail.message.id
        return HStack(spacing: 8) {
            Button {
                if model.claude?.configured == true {
                    model.extractItems()
                } else {
                    // No API key yet: take the user to where it goes.
                    openSettings()
                }
            } label: {
                if working {
                    HStack(spacing: 6) {
                        ProgressView().controlSize(.small)
                        Text("Reading…")
                    }
                } else {
                    Label("Add to To-do", systemImage: "sparkles")
                }
            }
            .disabled(detail.body == nil || model.extractingMessageId != nil)
            .help(
                model.claude?.configured == true
                    ? "Ask Claude to find to-dos and calendar entries in this email"
                    : "Add an Anthropic API key in Settings to use this")

            ForEach(model.linkedItems) { item in
                Button {
                    model.editingItem = item
                } label: {
                    Label(item.title, systemImage: item.done ? "checkmark.circle.fill" : "circle")
                        .lineLimit(1)
                }
                .buttonStyle(.borderless)
                .help(item.whenLabel ?? "No date")
            }
            Spacer(minLength: 0)
        }
        .padding(.top, 4)
    }

    /// The other messages of this conversation, one click away.
    private func threadStrip(_ detail: MessageDetail) -> some View {
        VStack(spacing: 0) {
            Button {
                showThread.toggle()
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
            .font(.callout)
            .padding(.horizontal, 16)
            .padding(.vertical, 6)

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
                                        .frame(width: 140, alignment: .leading)
                                    Text(message.snippet)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                    Spacer(minLength: 8)
                                    Text(MessageRowView.short(message.when))
                                        .foregroundStyle(.secondary)
                                }
                                .font(.callout)
                                .padding(.horizontal, 16)
                                .padding(.vertical, 4)
                                .background(current ? Color.accentColor.opacity(0.15) : Color.clear)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .frame(maxHeight: 150)
            }
        }
        .background(.bar)
    }

    private var invitationBar: some View {
        HStack {
            Image(systemName: "calendar.badge.plus").foregroundStyle(.secondary)
            Text("This email has a calendar invitation.")
                .foregroundStyle(.secondary)
            Spacer()
            Button("Add to calendar") { model.addInvitation() }
        }
        .font(.callout)
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
        .background(.bar)
    }

    private var remoteImagesBar: some View {
        HStack {
            Text("Remote images are hidden to protect your privacy.")
                .foregroundStyle(.secondary)
            Spacer()
            Button("Show images") { model.showRemoteImages() }
        }
        .font(.callout)
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
        .background(.bar)
    }
}

/// Shows one message body. JavaScript is off, nothing is persisted, and links
/// open in the default browser. The document's own Content-Security-Policy
/// (written by the engine) blocks every network load except, once the user
/// opts in, images.
struct MessageWebView: NSViewRepresentable {
    let html: String

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = context.coordinator
        view.allowsBackForwardNavigationGestures = false
        view.allowsLinkPreview = false
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {
        guard context.coordinator.shown != html else { return }
        context.coordinator.shown = html
        view.loadHTMLString(html, baseURL: nil)
    }

    final class Coordinator: NSObject, WKNavigationDelegate {
        var shown: String?

        func webView(
            _ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction,
            decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
        ) {
            guard navigationAction.navigationType == .linkActivated else {
                decisionHandler(.allow)
                return
            }
            if let url = navigationAction.request.url, ["https", "http", "mailto"].contains(url.scheme ?? "") {
                openExternally(url)
            }
            decisionHandler(.cancel)
        }
    }
}
