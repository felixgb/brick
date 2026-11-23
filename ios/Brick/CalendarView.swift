import SwiftUI
import UniformTypeIdentifiers

/// A month at a time, with the chosen day's items listed below it.
struct CalendarView: View {
    @Environment(AppModel.self) private var model
    @State private var month = CalendarView.monthStart(of: .now)
    @State private var selected = Day.today
    @State private var importing = false
    @State private var exported: URL?

    private static let calendarTypes = [UTType(filenameExtension: "ics") ?? .data]

    var body: some View {
        let dayItems = model.itemsOn(selected).sorted { ($0.startTime ?? "") < ($1.startTime ?? "") }

        NavigationStack {
            List {
                Section {
                    MonthGrid(month: month, selected: $selected)
                        .listRowInsets(EdgeInsets(top: 8, leading: 8, bottom: 8, trailing: 8))
                }
                Section(dayTitle) {
                    ForEach(dayItems) { item in
                        ItemRowView(item: item, showsDay: false)
                    }
                    Button("Add on This Day", systemImage: "plus") { model.newItem(on: selected) }
                }
            }
            .navigationTitle(month.formatted(.dateTime.month(.wide).year()))
            .gesture(
                DragGesture(minimumDistance: 40).onEnded { drag in
                    guard abs(drag.translation.width) > abs(drag.translation.height) * 2 else { return }
                    step(drag.translation.width < 0 ? 1 : -1)
                })
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { SettingsButton() }
                ToolbarItemGroup(placement: .topBarTrailing) {
                    Button("Previous Month", systemImage: "chevron.left") { step(-1) }
                    Button("Today") {
                        month = Self.monthStart(of: .now)
                        selected = Day.today
                    }
                    Button("Next Month", systemImage: "chevron.right") { step(1) }
                    Menu("More", systemImage: "ellipsis.circle") {
                        Button("Import Calendar…", systemImage: "square.and.arrow.down") { importing = true }
                        Button("Export Calendar…", systemImage: "square.and.arrow.up", action: export)
                    }
                }
            }
            .fileImporter(
                isPresented: $importing, allowedContentTypes: Self.calendarTypes, allowsMultipleSelection: true
            ) { result in
                switch result {
                case .success(let urls): model.importCalendars(urls)
                case .failure(let error): model.banner = error.localizedDescription
                }
            }
            .sheet(item: $exported) { url in
                ShareSheet(url: url)
            }
        }
    }

    private var dayTitle: String {
        guard let date = Day.date(selected) else { return selected }
        if selected == Day.today { return "Today" }
        return date.formatted(.dateTime.weekday(.wide).day().month(.wide))
    }

    private func step(_ months: Int) {
        guard let next = Calendar.current.date(byAdding: .month, value: months, to: month) else { return }
        month = next
        // Keep the chosen day in view: the 1st of the new month, or today if it's this month.
        let today = Date.now
        selected = Calendar.current.isDate(today, equalTo: next, toGranularity: .month)
            ? Day.today : Day.string(next)
    }

    private func export() {
        do {
            let url = FileManager.default.temporaryDirectory.appendingPathComponent("Brick calendar.ics")
            try model.calendarText().write(to: url, atomically: true, encoding: .utf8)
            exported = url
        } catch {
            model.banner = error.localizedDescription
        }
    }

    static func monthStart(of date: Date) -> Date {
        Calendar.current.dateInterval(of: .month, for: date)?.start ?? date
    }
}

/// Weekday headings and one cell per day, with a dot for each item (up to three).
struct MonthGrid: View {
    @Environment(AppModel.self) private var model
    let month: Date
    @Binding var selected: String

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 0), count: 7)

    var body: some View {
        let calendar = Calendar.current
        let symbols = calendar.veryShortStandaloneWeekdaySymbols
        let first = calendar.firstWeekday - 1
        let headings = Array(symbols[first...] + symbols[..<first])

        LazyVGrid(columns: columns, spacing: 4) {
            ForEach(Array(headings.enumerated()), id: \.offset) { _, symbol in
                Text(symbol)
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            ForEach(Array(days.enumerated()), id: \.offset) { _, day in
                if let day {
                    cell(day)
                } else {
                    Color.clear.frame(height: 44)
                }
            }
        }
    }

    /// The month's days, padded at the front so the 1st falls under its weekday.
    private var days: [String?] {
        let calendar = Calendar.current
        guard let range = calendar.range(of: .day, in: .month, for: month) else { return [] }
        let weekday = calendar.component(.weekday, from: month)
        let lead = (weekday - calendar.firstWeekday + 7) % 7
        let dates: [String?] = range.compactMap { day in
            calendar.date(byAdding: .day, value: day - 1, to: month).map(Day.string)
        }
        return Array(repeating: nil, count: lead) + dates
    }

    private func cell(_ day: String) -> some View {
        let count = model.items.filter { !$0.done && $0.occurs(on: day) }.count
        let isSelected = day == selected
        let isToday = day == Day.today
        return Button {
            selected = day
        } label: {
            VStack(spacing: 3) {
                Text(String(Int(day.suffix(2)) ?? 0))
                    .font(.callout.weight(isToday ? .bold : .regular))
                    .foregroundStyle(isSelected ? Color.white : (isToday ? Color.accentColor : Color.primary))
                    .frame(width: 30, height: 30)
                    .background(isSelected ? Color.accentColor : Color.clear, in: Circle())
                HStack(spacing: 3) {
                    ForEach(0..<min(count, 3), id: \.self) { _ in
                        Circle().fill(Color.secondary).frame(width: 4, height: 4)
                    }
                }
                .frame(height: 4)
            }
            .frame(maxWidth: .infinity, minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(day), \(count) items")
    }
}
