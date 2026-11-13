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

// MARK: - To-do and calendar items

/// Days are "yyyy-MM-dd" strings and times "HH:mm", both in local time,
/// exactly as the engine stores them.
enum Day {
    private static let dayFormatter: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd"
        return f
    }()

    private static let timeFormatter: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "HH:mm"
        return f
    }()

    static func string(_ date: Date) -> String { dayFormatter.string(from: date) }
    static func date(_ string: String) -> Date? { dayFormatter.date(from: string) }
    static var today: String { string(.now) }

    static func timeString(_ date: Date) -> String { timeFormatter.string(from: date) }
    static func time(_ string: String) -> Date? { timeFormatter.date(from: string) }
}

/// One thing to do or attend. With no date it is only a to-do; with a day it
/// is also on the calendar; with times it is a timed entry.
struct Item: Codable, Identifiable, Hashable {
    var id: Int64 = 0
    var title = ""
    var notes = ""
    var location = ""
    var date: String?
    var endDate: String?
    var startTime: String?
    var endTime: String?
    var done = false
    var sourceAccountId: Int64?
    var sourceMessageId = ""
    var sourceSubject = ""
    var sourceFrom = ""

    var isFromEmail: Bool { !sourceMessageId.isEmpty }
    var isOverdue: Bool { !done && (endDate ?? date).map { $0 < Day.today } == true }

    /// Whether the item falls on the given day, counting every day of a span.
    func occurs(on day: String) -> Bool {
        guard let date else { return false }
        return date <= day && day <= (endDate ?? date)
    }

    /// "Thu 12 Nov, 14:00 to 15:00", or nil for an undated item.
    var whenLabel: String? {
        guard let date, let start = Day.date(date) else { return nil }
        var text = Self.dayLabel(start)
        if let endDate, let end = Day.date(endDate) {
            text += " to " + Self.dayLabel(end)
        }
        if let startTime {
            text += ", " + startTime
            if let endTime { text += " to " + endTime }
        }
        return text
    }

    private static func dayLabel(_ date: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(date) { return "Today" }
        if calendar.isDateInTomorrow(date) { return "Tomorrow" }
        if calendar.isDateInYesterday(date) { return "Yesterday" }
        if calendar.isDate(date, equalTo: .now, toGranularity: .year) {
            return date.formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated))
        }
        return date.formatted(.dateTime.day().month(.abbreviated).year())
    }

    /// The form the engine's `items.save` expects.
    var payload: [String: Any] {
        func orNull(_ value: Any?) -> Any { value ?? NSNull() }
        return [
            "id": id, "title": title, "notes": notes, "location": location,
            "date": orNull(date), "end_date": orNull(endDate),
            "start_time": orNull(startTime), "end_time": orNull(endTime),
            "done": done,
            "source_account_id": orNull(sourceAccountId),
            "source_message_id": sourceMessageId, "source_subject": sourceSubject, "source_from": sourceFrom,
        ]
    }
}

struct ItemSource: Decodable {
    let messageId: Int64
    let folderId: Int64
}

struct ClaudeStatus: Decodable {
    let configured: Bool
    let model: String
    let defaultModel: String
}

/// One item Claude suggested, awaiting the user's decision.
struct Proposal: Identifiable {
    let id = UUID()
    var item: Item
    var include = true
}

/// What Claude found in an email.
struct ExtractionResult: Identifiable {
    let id = UUID()
    let subject: String
    let summary: String
    let truncated: Bool
    let proposals: [Proposal]
}

enum AppTab: String, CaseIterable, Identifiable {
    case mail, todo, calendar
    var id: String { rawValue }
    var title: String {
        switch self {
        case .mail: "Mail"
        case .todo: "To-do"
        case .calendar: "Calendar"
        }
    }
}
