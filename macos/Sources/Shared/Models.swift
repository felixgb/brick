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
    /// Set when the list is grouped into conversations: the conversation this
    /// row is the newest message of, and how many of its messages are in the list.
    var threadId: Int64? = nil
    var threadCount: Int? = nil
    var threadUnread: Int? = nil

    var messagesInThread: Int { threadCount ?? 1 }
    /// Whether anything in the conversation is unread (just this message when not grouped).
    var hasUnread: Bool { threadUnread.map { $0 > 0 } ?? !seen }

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
    /// Carries a calendar invitation that `items.import_invitation` can add.
    let invitation: Bool?

    var hasInvitation: Bool { invitation ?? false }
}

/// What importing a calendar file or an invitation did.
struct ImportResult: Decodable {
    let added: Int
    let skipped: Int
    let repeatsDropped: Int

    var summary: String {
        var text = added == 1 ? "Added 1 item" : "Added \(added) items"
        if skipped > 0 { text += ", \(skipped) already there" }
        if repeatsDropped > 0 { text += ", \(repeatsDropped) repeat\(repeatsDropped == 1 ? "" : "s") too complex to keep" }
        return text + "."
    }
}

struct ThreadMessages: Decodable {
    let rows: [MessageRow]
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
    /// An alias to send as; nil for the account's own address.
    var from: String?
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
    /// "", "daily", "weekly", "monthly" or "yearly". `date` is the next occurrence still to do.
    var `repeat` = ""
    /// The last day an occurrence may start.
    var repeatUntil: String?

    var isFromEmail: Bool { !sourceMessageId.isEmpty }
    var isOverdue: Bool { !done && (endDate ?? date).map { $0 < Day.today } == true }

    var isRepeating: Bool { !self.repeat.isEmpty }

    /// Whether an occurrence starts on `day`. Monthly and yearly repeats that
    /// start on the 29th to 31st fall on the last day of a shorter month.
    func startsOn(_ day: String) -> Bool {
        guard let date, day >= date else { return false }
        if !isRepeating { return day == date }
        if let repeatUntil, day > repeatUntil { return false }
        guard let start = Day.date(date), let on = Day.date(day) else { return false }
        let calendar = Calendar.current
        switch self.repeat {
        case "daily":
            return true
        case "weekly":
            return (calendar.dateComponents([.day], from: start, to: on).day ?? 1) % 7 == 0
        case "monthly", "yearly":
            let startParts = calendar.dateComponents([.month, .day], from: start)
            let onParts = calendar.dateComponents([.month, .day], from: on)
            let length = calendar.range(of: .day, in: .month, for: on)?.count ?? 31
            if self.repeat == "yearly", startParts.month != onParts.month { return false }
            return onParts.day == min(startParts.day ?? 1, length)
        default:
            return false
        }
    }

    /// Whether the item falls on the given day, counting every day of a span
    /// and every occurrence of a repeating item.
    func occurs(on day: String) -> Bool {
        guard let date else { return false }
        if !isRepeating { return date <= day && day <= (endDate ?? date) }
        guard day >= date, let on = Day.date(day), let start = Day.date(date) else { return false }
        let calendar = Calendar.current
        let span = endDate.flatMap(Day.date).flatMap { calendar.dateComponents([.day], from: start, to: $0).day } ?? 0
        // An occurrence covering `day` started on it, or up to `span` days before.
        for back in 0...max(span, 0) {
            guard let from = calendar.date(byAdding: .day, value: -back, to: on) else { continue }
            if startsOn(Day.string(from)) { return true }
        }
        return false
    }

    /// "Repeats weekly until 3 Mar 2027", or nil.
    var repeatLabel: String? {
        guard isRepeating else { return nil }
        let unit = ["daily": "daily", "weekly": "weekly", "monthly": "monthly", "yearly": "yearly"][self.repeat] ?? self.repeat
        var text = "Repeats \(unit)"
        if let repeatUntil, let end = Day.date(repeatUntil) {
            text += " until " + end.formatted(.dateTime.day().month(.abbreviated).year())
        }
        return text
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
            "repeat": self.repeat, "repeat_until": orNull(repeatUntil),
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

enum CalendarMode: String, CaseIterable, Identifiable {
    case month, week, day
    var id: String { rawValue }
    var title: String { rawValue.capitalized }
}

/// One entry of the From picker: an account's own address or one of its aliases.
struct FromChoice: Identifiable, Hashable {
    let accountId: Int64
    /// nil for the account's own address.
    let alias: String?
    let label: String
    var id: String { "\(accountId)|\(alias ?? "")" }
}
