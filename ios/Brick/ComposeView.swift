import SwiftUI

struct ComposeView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var draft: ComposeDraft
    @State private var problem: String?
    @State private var choices: [FromChoice] = []
    @State private var choice = ""
    /// The signature block currently in the body, if any.
    @State private var signature = ""
    @State private var confirmingDiscard = false
    @FocusState private var focus: Field?

    private enum Field { case to, body }

    init(draft: ComposeDraft) {
        _draft = State(initialValue: draft)
    }

    private var state: SendState? { model.sendStates[draft.token] }
    private var sending: Bool { state == .sending }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    if choices.count > 1 {
                        Picker("From", selection: $choice) {
                            ForEach(choices) { option in
                                Text(option.label).tag(option.id)
                            }
                        }
                    }
                    address("To", $draft.to)
                        .focused($focus, equals: .to)
                    address("Cc", $draft.cc)
                    TextField("Subject", text: $draft.subject)
                }
                Section {
                    TextEditor(text: $draft.text)
                        .focused($focus, equals: .body)
                        .frame(minHeight: 320)
                }
                if let problem {
                    Section {
                        Label(problem, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.red)
                    }
                }
            }
            .disabled(sending)
            .navigationTitle(draft.subject.isEmpty ? "New Message" : draft.subject)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) { confirmingDiscard = true }
                        .confirmationDialog("Discard this message?", isPresented: $confirmingDiscard) {
                            Button("Discard", role: .destructive) { dismiss() }
                        }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if sending {
                        ProgressView()
                    } else {
                        Button("Send", systemImage: "paperplane.fill") { problem = model.send(draft) }
                            .disabled(draft.to.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
        }
        .interactiveDismissDisabled()
        .onAppear {
            if draft.accountId == 0 { draft.accountId = model.currentAccountId }
            choices = model.fromChoices()
            choice = FromChoice(accountId: draft.accountId, alias: draft.from, label: "").id
            applySignature()
            focus = draft.to.isEmpty ? .to : .body
        }
        .onChange(of: choice) { _, new in
            guard let picked = choices.first(where: { $0.id == new }) else { return }
            draft.accountId = picked.accountId
            draft.from = picked.alias
            applySignature()
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

    private func address(_ label: String, _ text: Binding<String>) -> some View {
        LabeledContent(label) {
            TextField(label, text: text, prompt: Text("name@example.com"))
                .keyboardType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
        }
    }

    private func signatureBlock(for accountId: Int64) -> String {
        let text = model.signature(for: accountId)
        return text.isEmpty ? "" : "-- \n" + text
    }

    /// Puts the sending account's signature above any quoted text, replacing
    /// the previous account's when From changes. (As on the Mac.)
    private func applySignature() {
        let next = signatureBlock(for: draft.accountId)
        guard next != signature else { return }
        var text = draft.text
        let block = "\n\n" + signature
        if !signature.isEmpty, text.contains(block) {
            text = text.replacingOccurrences(of: block, with: next.isEmpty ? "" : "\n\n" + next)
        } else if !signature.isEmpty, text.contains(signature) {
            text = text.replacingOccurrences(of: signature, with: next)
        } else if !next.isEmpty {
            if text.contains(next) {
                signature = next
                return
            }
            text = "\n\n" + next + text
        }
        signature = next
        draft.text = text
    }
}
