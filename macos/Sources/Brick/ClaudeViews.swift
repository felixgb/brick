import SwiftUI

/// Shows what Claude found in an email. Nothing has been saved yet: the user
/// edits, unticks, and confirms.
struct ExtractionSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let result: ExtractionResult
    @State private var proposals: [Proposal]

    init(result: ExtractionResult) {
        self.result = result
        _proposals = State(initialValue: result.proposals)
    }

    private var chosen: [Item] {
        proposals
            .filter { $0.include && !$0.item.title.trimmingCharacters(in: .whitespaces).isEmpty }
            .map(\.item)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(proposals.isEmpty ? "Nothing to add" : "Add to your to-do list and calendar")
                    .font(.headline)
                Text("From “\(result.subject)”")
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            if !result.summary.isEmpty {
                Text(result.summary)
                    .font(.callout)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if result.truncated {
                Label("This email was very long, so Claude only read the first part.", systemImage: "scissors")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            if !proposals.isEmpty {
                ScrollView {
                    VStack(spacing: 10) {
                        ForEach($proposals) { $proposal in
                            ProposalCard(proposal: $proposal)
                        }
                    }
                    .padding(.vertical, 2)
                }
                .frame(maxHeight: 420)
            }

            HStack {
                Spacer()
                if proposals.isEmpty {
                    Button("Close") { dismiss() }
                        .keyboardShortcut(.defaultAction)
                } else {
                    Button("Cancel", role: .cancel) { dismiss() }
                        .keyboardShortcut(.cancelAction)
                    Button(chosen.count == 1 ? "Add 1 item" : "Add \(chosen.count) items") {
                        model.add(chosen)
                        dismiss()
                    }
                    .keyboardShortcut(.defaultAction)
                    .disabled(chosen.isEmpty)
                }
            }
        }
        .padding(20)
        .frame(width: 520)
    }
}

private struct ProposalCard: View {
    @Binding var proposal: Proposal

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Toggle("Include", isOn: $proposal.include)
                .toggleStyle(.checkbox)
                .labelsHidden()
                .padding(.top, 3)

            Form {
                TextField("Title", text: $proposal.item.title)
                ItemWhenFields(item: $proposal.item)
                TextField("Location", text: $proposal.item.location)
                TextField("Notes", text: $proposal.item.notes, axis: .vertical)
                    .lineLimit(1...5)
            }
            .formStyle(.columns)
            .disabled(!proposal.include)
            .opacity(proposal.include ? 1 : 0.5)
        }
        .padding(12)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 8))
    }
}

/// Settings for the Claude integration: the API key and the model.
struct ClaudeSettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var key = ""
    @State private var modelName = ""

    private var configured: Bool { model.claude?.configured == true }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Form {
                SecureField(
                    "Anthropic API key", text: $key,
                    prompt: Text(configured ? "Saved in config.toml" : "sk-ant-…"))
                TextField("Model", text: $modelName, prompt: Text(model.claude?.defaultModel ?? ""))
            }
            .formStyle(.columns)

            HStack {
                if configured {
                    Button("Remove key", role: .destructive) {
                        model.setClaudeKey("")
                        key = ""
                    }
                }
                Spacer()
                Button("Save") {
                    if !key.trimmingCharacters(in: .whitespaces).isEmpty {
                        model.setClaudeKey(key)
                        key = ""
                    }
                    model.setClaudeModel(modelName.trimmingCharacters(in: .whitespaces))
                }
                .keyboardShortcut(.defaultAction)
            }

            Divider()

            Text(
                """
                When you choose Add to To-do on an email, its sender, recipients, subject, date and text \
                are sent to Anthropic's API using this key. Attachments are not sent. Nothing is sent at \
                any other time.
                """
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)

            Spacer(minLength: 0)
        }
        .padding(20)
        .frame(width: 460, height: 320)
        .onAppear {
            model.refreshClaudeStatus()
            // Show the model only when it has been changed from the default.
            let current = model.claude?.model ?? ""
            modelName = current == model.claude?.defaultModel ? "" : current
        }
    }
}
