import SwiftUI

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var adding = false

    var body: some View {
        @Bindable var model = model

        NavigationStack {
            Form {
                Section("Accounts") {
                    ForEach(model.accounts) { account in
                        NavigationLink {
                            AccountOptionsView(account: account)
                        } label: {
                            VStack(alignment: .leading) {
                                Text(account.email)
                                Text(model.providers.first { $0.id == account.provider }?.label ?? account.provider)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    Button("Add Account", systemImage: "plus") { adding = true }
                }
                Section("Mail") {
                    Toggle("Group into Conversations", isOn: $model.threaded)
                }
                Section {
                    LabeledContent("File", value: "config.toml")
                } header: {
                    Text("Settings file")
                } footer: {
                    Text("Everything Brick keeps, including passwords, is in config.toml. Open it in Files under On My iPhone › Brick to change settings that have no screen here yet, such as the Claude API key.")
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .sheet(isPresented: $adding) { AddAccountView() }
        }
    }
}

/// Extra addresses an account may send as, its signature, and removing it.
struct AccountOptionsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let account: Account
    @State private var aliases = ""
    @State private var signature = ""
    @State private var confirmingRemoval = false

    var body: some View {
        Form {
            Section {
                TextEditor(text: $aliases)
                    .font(.body.monospaced())
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .frame(minHeight: 80)
            } header: {
                Text("Other addresses you can send from")
            } footer: {
                Text("One per line. Your mail provider must let this account send as them.")
            }
            Section("Signature") {
                TextEditor(text: $signature)
                    .frame(minHeight: 100)
            }
            Section {
                Button("Remove Account", role: .destructive) { confirmingRemoval = true }
            } footer: {
                Text("Mail stays on the server. Only the copy on this iPhone and the saved sign-in are deleted.")
            }
        }
        .navigationTitle(account.email)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") {
                    let list = aliases.split(whereSeparator: { ",; \n\t".contains($0) }).map(String.init)
                    model.setSetting("aliases_\(account.id)", list.joined(separator: "\n"))
                    model.setSetting("signature_\(account.id)", signature.trimmingCharacters(in: .whitespacesAndNewlines))
                    dismiss()
                }
            }
        }
        .confirmationDialog("Remove \(account.email) from Brick?", isPresented: $confirmingRemoval) {
            Button("Remove Account", role: .destructive) {
                model.removeAccount(account.id)
                dismiss()
            }
        }
        .onAppear {
            aliases = model.aliases(for: account.id).joined(separator: "\n")
            signature = model.signature(for: account.id)
        }
    }
}

struct AddAccountView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    @State private var providerId = "fastmail"
    @State private var name = ""
    @State private var email = ""
    @State private var password = ""
    @State private var username = ""
    @State private var imapHost = ""
    @State private var imapPort = 993
    @State private var smtpHost = ""
    @State private var smtpPort = 465
    @State private var smtpSecurity = "tls"

    private var provider: Provider? { model.providers.first { $0.id == providerId } }
    private var isCustom: Bool { providerId == "custom" }
    /// Browser sign-in needs a redirect the iPhone app can't receive yet.
    private var needsBrowserSignIn: Bool { provider?.oauth != nil }

    private var waiting: Bool {
        if case .waiting = model.addAccountState { return true }
        return false
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker("Provider", selection: $providerId) {
                        ForEach(model.providers) { provider in
                            Text(provider.label).tag(provider.id)
                        }
                    }
                }
                if needsBrowserSignIn {
                    Section {
                        Text("Signing in to \(provider?.label ?? "this provider") through the browser doesn't work on iPhone yet. If your account allows app passwords, choose Other (IMAP) and use one.")
                            .foregroundStyle(.secondary)
                    }
                } else {
                    Section {
                        TextField("Your name", text: $name, prompt: Text("Shown to people you write to"))
                            .textContentType(.name)
                        TextField("Email", text: $email, prompt: Text("name@example.com"))
                            .keyboardType(.emailAddress)
                            .textContentType(.emailAddress)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        SecureField("Password", text: $password)
                            .textContentType(.password)
                    } footer: {
                        if let hint = provider?.hint, !hint.isEmpty { Text(hint) }
                    }
                    if isCustom {
                        Section("Servers") {
                            TextField("Username", text: $username, prompt: Text("Defaults to the email address"))
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                            TextField("IMAP server", text: $imapHost)
                                .keyboardType(.URL)
                                .textInputAutocapitalization(.never)
                            LabeledContent("IMAP port") {
                                TextField("993", value: $imapPort, format: .number.grouping(.never))
                                    .keyboardType(.numberPad)
                                    .multilineTextAlignment(.trailing)
                            }
                            TextField("SMTP server", text: $smtpHost)
                                .keyboardType(.URL)
                                .textInputAutocapitalization(.never)
                            LabeledContent("SMTP port") {
                                TextField("465", value: $smtpPort, format: .number.grouping(.never))
                                    .keyboardType(.numberPad)
                                    .multilineTextAlignment(.trailing)
                            }
                            Picker("SMTP security", selection: $smtpSecurity) {
                                Text("TLS").tag("tls")
                                Text("STARTTLS").tag("starttls")
                            }
                        }
                    }
                }

                switch model.addAccountState {
                case .waiting(let message):
                    Section {
                        HStack {
                            ProgressView()
                            Text(message)
                        }
                    }
                case .failed(let message):
                    Section {
                        Label(message, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.red)
                    }
                default:
                    EmptyView()
                }
            }
            .disabled(waiting)
            .navigationTitle("Add Account")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) {
                        model.addAccountState = .idle
                        dismiss()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add", action: submit)
                        .disabled(waiting || needsBrowserSignIn || email.isEmpty || password.isEmpty)
                }
            }
        }
        .interactiveDismissDisabled(waiting)
        .onAppear { model.addAccountState = .idle }
        .onChange(of: providerId) {
            model.addAccountState = .idle
            imapPort = provider?.imapPort ?? 993
            smtpPort = provider?.smtpPort ?? 465
        }
        .onChange(of: model.addAccountState) { _, state in
            if state == .added {
                model.addAccountState = .idle
                dismiss()
            }
        }
    }

    private func submit() {
        guard let provider else { return }
        var fields: [String: Any] = [
            "provider": provider.id,
            "email": email.trimmingCharacters(in: .whitespaces),
            "name": name.trimmingCharacters(in: .whitespaces),
            "password": password,
        ]
        if isCustom {
            fields["username"] = username.trimmingCharacters(in: .whitespaces)
            fields["imap_host"] = imapHost.trimmingCharacters(in: .whitespaces)
            fields["imap_port"] = imapPort
            fields["smtp_host"] = smtpHost.trimmingCharacters(in: .whitespaces)
            fields["smtp_port"] = smtpPort
            fields["smtp_security"] = smtpSecurity
        }
        model.addAccount(provider: provider, fields: fields, clientId: "", clientSecret: "")
    }
}
