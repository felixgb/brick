import SwiftUI
import WebKit

struct ReaderView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        if let detail = model.detail {
            VStack(spacing: 0) {
                header(detail)
                Divider()
                if let body = detail.body {
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
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
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
                NSWorkspace.shared.open(url)
            }
            decisionHandler(.cancel)
        }
    }
}
