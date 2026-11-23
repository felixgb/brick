import SwiftUI

// Views both the Mac and iPhone apps use as they are.

struct BannerView: View {
    let text: String
    var symbol = "exclamationmark.triangle.fill"
    var tint: Color = .yellow
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: symbol).foregroundStyle(tint)
            Text(text).lineLimit(2)
            Spacer()
            Button("Dismiss", systemImage: "xmark", action: dismiss)
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
        }
        .font(.callout)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
    }
}

struct MessageRowView: View {
    let row: MessageRow

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            Circle()
                .fill(row.hasUnread ? Color.accentColor : Color.clear)
                .frame(width: 8, height: 8)
                .padding(.top, 5)
                .accessibilityLabel(row.hasUnread ? "Unread" : "")

            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline) {
                    Text(row.sender)
                        .font(.headline)
                        .fontWeight(row.hasUnread ? .semibold : .regular)
                        .lineLimit(1)
                    if row.messagesInThread > 1 {
                        Text("\(row.messagesInThread)")
                            .font(.caption2.weight(.medium))
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, 5)
                            .background(.quaternary, in: Capsule())
                            .help("\(row.messagesInThread) messages in this conversation")
                    }
                    Spacer(minLength: 8)
                    if row.flagged {
                        Image(systemName: "flag.fill").foregroundStyle(.orange).font(.caption)
                    }
                    if row.hasAttachments {
                        Image(systemName: "paperclip").foregroundStyle(.secondary).font(.caption)
                    }
                    Text(Self.short(row.when))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                Text(row.displaySubject)
                    .font(.subheadline)
                    .lineLimit(1)
                if !row.snippet.isEmpty {
                    Text(row.snippet)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
        }
        .padding(.vertical, 3)
    }

    static func short(_ date: Date) -> String {
        let calendar = Calendar.current
        if calendar.isDateInToday(date) {
            return date.formatted(date: .omitted, time: .shortened)
        }
        if calendar.isDateInYesterday(date) {
            return "Yesterday"
        }
        if calendar.isDate(date, equalTo: .now, toGranularity: .year) {
            return date.formatted(.dateTime.day().month(.abbreviated))
        }
        return date.formatted(date: .numeric, time: .omitted)
    }
}
