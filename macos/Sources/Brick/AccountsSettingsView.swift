import SwiftUI

struct AccountsSettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var selected: Int64?
    @State private var adding = false
    @State private var confirmingRemoval = false
    @State private var optionsFor: Account?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            List(model.accounts, selection: $selected) { account in
                VStack(alignment: .leading) {
                    Text(account.email)
                    Text(model.providers.first { $0.id == account.provider }?.label ?? account.provider)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .tag(Optional(account.id))
            }
            .overlay {
                if model.accounts.isEmpty {
                    ContentUnavailableView("No accounts yet", systemImage: "envelope")
                }
            }

            Divider()
            HStack(spacing: 0) {
                Button("Add account", systemImage: "plus") { adding = true }
                Button("Remove account", systemImage: "minus") { confirmingRemoval = true }
                    .disabled(selected == nil)
                Button("Addresses and signature", systemImage: "signature") {
                    optionsFor = model.accounts.first { $0.id == selected }
                }
                .disabled(selected == nil)
                .help("Extra addresses to send from, and a signature")
                Spacer()
            }
            .labelStyle(.iconOnly)
            .buttonStyle(.borderless)
            .padding(8)
        }
        .frame(width: 460, height: 320)
        .sheet(isPresented: $adding) {
            AddAccountSheet()
        }
        .sheet(item: $optionsFor) { account in
            AccountOptionsSheet(account: account)
        }
        .confirmationDialog(
            "Remove \(model.accounts.first { $0.id == selected }?.email ?? "this account") from Brick?",
            isPresented: $confirmingRemoval
        ) {
            Button("Remove account", role: .destructive) {
                if let selected { model.removeAccount(selected) }
                selected = nil
            }
        } message: {
            Text("Mail stays on the server. Only the local copy and saved sign-in are deleted.")
        }
    }
}

/// Extra addresses an account may send as, and its signature.
struct AccountOptionsSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let account: Account
    @State private var aliases = ""
    @State private var signature = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(account.email).font(.headline)

            VStack(alignment: .leading, spacing: 4) {
                Text("Other addresses you can send from").font(.subheadline)
                TextEditor(text: $aliases)
                    .font(.body.monospaced())
                    .frame(height: 64)
                    .border(Color.secondary.opacity(0.3))
                Text("One per line. Your mail provider must let this account send as them.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 4) {
                Text("Signature").font(.subheadline)
                TextEditor(text: $signature)
                    .frame(height: 96)
                    .border(Color.secondary.opacity(0.3))
            }

            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    let list = aliases.split(whereSeparator: { ",; \n\t".contains($0) }).map(String.init)
                    model.setSetting("aliases_\(account.id)", list.joined(separator: "\n"))
                    model.setSetting("signature_\(account.id)", signature.trimmingCharacters(in: .whitespacesAndNewlines))
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 420)
        .onAppear {
            aliases = model.aliases(for: account.id).joined(separator: "\n")
            signature = model.signature(for: account.id)
        }
    }
}

struct AddAccountSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    @State private var providerId = "fastmail"
    @State private var name = ""
    @State private var email = ""
    @State private var password = ""
    @State private var clientId = ""
    @State private var clientSecret = ""
    @State private var username = ""
    @State private var imapHost = ""
    @State private var imapPort = 993
    @State private var smtpHost = ""
    @State private var smtpPort = 465
    @State private var smtpSecurity = "tls"

    private var provider: Provider? { model.providers.first { $0.id == providerId } }
    private var isCustom: Bool { providerId == "custom" }

    private var waiting: Bool {
        if case .waiting = model.addAccountState { return true }
        return false
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Form {
                Picker("Provider", selection: $providerId) {
                    ForEach(model.providers) { provider in
                        Text(provider.label).tag(provider.id)
                    }
                }
                TextField("Your name", text: $name, prompt: Text("Shown to people you write to"))
                TextField("Email", text: $email, prompt: Text("name@example.com"))

                if provider?.oauth != nil {
                    TextField("OAuth client ID", text: $clientId)
                    if providerId == "gmail" {
                        SecureField("OAuth client secret", text: $clientSecret)
                    }
                } else {
                    SecureField("Password", text: $password)
                }

                if isCustom {
                    TextField("Username", text: $username, prompt: Text("Defaults to the email address"))
                    TextField("IMAP server", text: $imapHost)
                    TextField("IMAP port", value: $imapPort, format: .number.grouping(.never))
                    TextField("SMTP server", text: $smtpHost)
                    TextField("SMTP port", value: $smtpPort, format: .number.grouping(.never))
                    Picker("SMTP security", selection: $smtpSecurity) {
                        Text("TLS").tag("tls")
                        Text("STARTTLS").tag("starttls")
                    }
                }
            }
            .formStyle(.columns)
            .disabled(waiting)

            if let hint = provider?.hint, !hint.isEmpty {
                Text(hint).font(.callout).foregroundStyle(.secondary)
            }

            switch model.addAccountState {
            case .waiting(let message):
                HStack {
                    ProgressView().controlSize(.small)
                    Text(message)
                }
                .font(.callout)
            case .failed(let message):
                Label(message, systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .foregroundStyle(.red)
            default:
                EmptyView()
            }

            HStack {
                Spacer()
                Button("Cancel", role: .cancel) {
                    model.addAccountState = .idle
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
                Button(provider?.oauth != nil ? "Sign in with browser" : "Add account", action: submit)
                    .keyboardShortcut(.defaultAction)
                    .disabled(waiting || email.isEmpty)
            }
        }
        .padding(20)
        .frame(width: 460)
        .onAppear {
            model.addAccountState = .idle
            loadClientSettings()
        }
        .onChange(of: providerId) {
            model.addAccountState = .idle
            imapPort = provider?.imapPort ?? 993
            smtpPort = provider?.smtpPort ?? 465
            loadClientSettings()
        }
        .onChange(of: model.addAccountState) { _, state in
            if state == .added {
                model.addAccountState = .idle
                dismiss()
            }
        }
    }

    private func loadClientSettings() {
        guard let oauth = provider?.oauth else { return }
        clientId = model.setting(oauth.clientIdKey)
        clientSecret = model.setting(oauth.clientSecretKey)
    }

    private func submit() {
        guard let provider else { return }
        var fields: [String: Any] = [
            "provider": provider.id,
            "email": email.trimmingCharacters(in: .whitespaces),
            "name": name.trimmingCharacters(in: .whitespaces),
        ]
        if provider.oauth == nil {
            fields["password"] = password
        }
        if isCustom {
            fields["username"] = username.trimmingCharacters(in: .whitespaces)
            fields["imap_host"] = imapHost.trimmingCharacters(in: .whitespaces)
            fields["imap_port"] = imapPort
            fields["smtp_host"] = smtpHost.trimmingCharacters(in: .whitespaces)
            fields["smtp_port"] = smtpPort
            fields["smtp_security"] = smtpSecurity
        }
        model.addAccount(
            provider: provider, fields: fields,
            clientId: clientId.trimmingCharacters(in: .whitespaces),
            clientSecret: clientSecret.trimmingCharacters(in: .whitespaces))
    }
}
