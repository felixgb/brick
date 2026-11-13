import SwiftUI

struct ComposeView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var draft: ComposeDraft
    @State private var problem: String?
    @FocusState private var focus: Field?

    private enum Field { case to, body }

    init(draft: ComposeDraft) {
        _draft = State(initialValue: draft)
    }

    private var state: SendState? { model.sendStates[draft.token] }
    private var sending: Bool { state == .sending }

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Picker("From", selection: $draft.accountId) {
                    ForEach(model.accounts) { account in
                        Text(account.name.isEmpty ? account.email : "\(account.name) <\(account.email)>")
                            .tag(account.id)
                    }
                }
                TextField("To", text: $draft.to, prompt: Text("name@example.com"))
                    .focused($focus, equals: .to)
                TextField("Cc", text: $draft.cc)
                TextField("Subject", text: $draft.subject)
            }
            .formStyle(.columns)
            .padding(12)

            Divider()

            TextEditor(text: $draft.text)
                .font(.body)
                .focused($focus, equals: .body)
                .scrollContentBackground(.hidden)
                .padding(8)

            if let problem {
                Divider()
                Label(problem, systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            }
        }
        .disabled(sending)
        .navigationTitle(draft.subject.isEmpty ? "New Message" : draft.subject)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                if sending {
                    ProgressView().controlSize(.small)
                } else {
                    Button("Send", systemImage: "paperplane.fill", action: send)
                        .keyboardShortcut(.return, modifiers: .command)
                        .help("Send")
                }
            }
        }
        .onAppear {
            if draft.accountId == 0 { draft.accountId = model.currentAccountId }
            focus = draft.to.isEmpty ? .to : .body
        }
        .onChange(of: state) { _, new in
            switch new {
            case .some(.sent):
                model.sendStates[draft.token] = nil
                dismiss()
            case .some(.failed(let message)):
                problem = "Not sent. \(message)"
                model.sendStates[draft.token] = nil
            default:
                break
            }
        }
    }

    private func send() {
        problem = model.send(draft)
    }
}
