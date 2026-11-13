import Foundation

struct Account: Decodable, Identifiable, Hashable {
    let id: Int64
    let email: String
    let name: String
    let provider: String
}

struct Folder: Decodable, Identifiable, Hashable {
    let id: Int64
    let accountId: Int64
    let path: String
    let name: String
    let role: String
    let selectable: Bool
    let unread: Int
    let total: Int

    var symbol: String {
        switch role {
        case "inbox": "tray"
        case "sent": "paperplane"
        case "drafts": "doc"
        case "archive", "all": "archivebox"
        case "trash": "trash"
        case "junk": "xmark.bin"
        default: "folder"
        }
    }
}

struct MessageRow: Decodable, Identifiable, Hashable {
    let id: Int64
    let accountId: Int64
    let folderId: Int64
    let subject: String
    let fromName: String
    let fromAddr: String
    let to: String
    let date: Int64
    let seen: Bool
    let flagged: Bool
    let answered: Bool
    let hasAttachments: Bool
    let snippet: String

    var sender: String { fromName.isEmpty ? fromAddr : fromName }
    var fullSender: String { fromName.isEmpty ? fromAddr : "\(fromName) <\(fromAddr)>" }
    var displaySubject: String { subject.isEmpty ? "(no subject)" : subject }
    var when: Date { Date(timeIntervalSince1970: TimeInterval(date)) }
}

struct MessagePage: Decodable {
    let total: Int
    let rows: [MessageRow]
}

struct SearchResults: Decodable {
    let rows: [MessageRow]
}

struct Attachment: Decodable, Identifiable, Hashable {
    let index: Int
    let name: String
    let mime: String
    let size: Int
    var id: Int { index }
}

struct MessageBody: Decodable {
    let html: String
    let text: String
    let attachments: [Attachment]
    let blockedRemote: Bool
}

struct MessageDetail: Decodable {
    let message: MessageRow
    let cc: String
    let body: MessageBody?
}

struct OAuthInfo: Decodable, Hashable {
    let clientIdKey: String
    let clientSecretKey: String
}

struct Provider: Decodable, Identifiable, Hashable {
    let id: String
    let label: String
    let imapPort: Int
    let smtpPort: Int
    let oauth: OAuthInfo?
    let hint: String
}

struct OAuthStart: Decodable {
    let authUrl: String
}

/// What a compose window opens with. Codable so SwiftUI can restore windows.
struct ComposeDraft: Codable, Hashable {
    var token = UUID().uuidString
    var accountId: Int64 = 0
    var to = ""
    var cc = ""
    var subject = ""
    var text = ""
    var replyToId: Int64?
}
