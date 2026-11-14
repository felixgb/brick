import SwiftUI

/// The Calendar tab: a month grid of the same items the To-do tab lists,
/// with the selected day's items alongside.
struct CalendarView: View {
    @Environment(AppModel.self) private var model
    /// Any moment inside the month on screen.
    @State private var month = Date.now
    @State private var selectedDay = Day.today

    private let calendar = Calendar.current

    var body: some View {
        HStack(spacing: 0) {
            VStack(spacing: 0) {
                weekdayHeader
                Divider()
                monthGrid
            }
            Divider()
            dayPanel
                .frame(width: 280)
        }
        .background(.background)
        .navigationTitle(month.formatted(.dateTime.month(.wide).year()))
        .navigationSubtitle("")
        .toolbar {
            ToolbarItemGroup(placement: .navigation) {
                Button("Previous month", systemImage: "chevron.left") { shift(-1) }
                    .help("Previous month")
                Button("Today") {
                    month = .now
                    selectedDay = Day.today
                }
                .help("Go to today")
                Button("Next month", systemImage: "chevron.right") { shift(1) }
                    .help("Next month")
            }
            ToolbarItem(placement: .primaryAction) {
                Button("New item", systemImage: "plus") { model.newItem(on: selectedDay) }
                    .help("Add an item on the selected day")
            }
        }
    }

    // MARK: Grid

    /// The 42 days shown: whole weeks covering the month.
    private var days: [Date] {
        guard let interval = calendar.dateInterval(of: .month, for: month) else { return [] }
        let first = interval.start
        let lead = (calendar.component(.weekday, from: first) - calendar.firstWeekday + 7) % 7
        guard let start = calendar.date(byAdding: .day, value: -lead, to: first) else { return [] }
        return (0..<42).compactMap { calendar.date(byAdding: .day, value: $0, to: start) }
    }

    private var weekdaySymbols: [String] {
        let symbols = calendar.shortStandaloneWeekdaySymbols
        let shift = calendar.firstWeekday - 1
        return Array(symbols[shift...] + symbols[..<shift])
    }

    private var weekdayHeader: some View {
        HStack(spacing: 0) {
            ForEach(weekdaySymbols, id: \.self) { symbol in
                Text(symbol)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 6)
            }
        }
    }

    private var monthGrid: some View {
        let days = days
        let columns = Array(repeating: GridItem(.flexible(), spacing: 0), count: 7)
        // Group once per redraw rather than filtering the list for all 42 cells.
        var byDay: [String: [Item]] = [:]
        let visible = Set(days.map(Day.string))
        for item in model.items {
            for day in visible where item.occurs(on: day) {
                byDay[day, default: []].append(item)
            }
        }

        return GeometryReader { geometry in
            LazyVGrid(columns: columns, spacing: 0) {
                ForEach(days, id: \.self) { date in
                    let key = Day.string(date)
                    DayCell(
                        date: date,
                        items: byDay[key] ?? [],
                        inMonth: calendar.isDate(date, equalTo: month, toGranularity: .month),
                        isToday: calendar.isDateInToday(date),
                        isSelected: key == selectedDay
                    )
                    .frame(height: geometry.size.height / 6)
                    .contentShape(Rectangle())
                    .equatable()
                    // Select on the first click. A count: 2 onTapGesture placed
                    // before a single-tap one makes SwiftUI wait out the
                    // double-click interval before selecting, which felt laggy.
                    .onTapGesture { selectedDay = key }
                    .simultaneousGesture(
                        TapGesture(count: 2).onEnded { model.newItem(on: key) }
                    )
                }
            }
        }
    }

    private func shift(_ months: Int) {
        guard let moved = calendar.date(byAdding: .month, value: months, to: month) else { return }
        month = moved
        // Keep a day selected inside the month now on screen.
        if let interval = calendar.dateInterval(of: .month, for: moved) {
            selectedDay = calendar.isDate(moved, equalTo: .now, toGranularity: .month)
                ? Day.today : Day.string(interval.start)
        }
    }

    // MARK: Selected day

    private var dayPanel: some View {
        let items = model.itemsOn(selectedDay)
        let title = Day.date(selectedDay)?.formatted(.dateTime.weekday(.wide).day().month(.wide)) ?? selectedDay

        return VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(title).font(.headline)
                Spacer()
                Button("Add item", systemImage: "plus") { model.newItem(on: selectedDay) }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .help("Add an item on this day")
            }
            .padding(12)
            Divider()
            if items.isEmpty {
                ContentUnavailableView("Nothing on this day", systemImage: "calendar")
            } else {
                List(items) { item in
                    ItemRowView(item: item, showsDay: false)
                }
                .listStyle(.inset)
            }
        }
    }
}

private struct DayCell: View, Equatable {
    // Only the selection changes on a click; Equatable lets SwiftUI skip the
    // other 41 cells instead of re-formatting all of their dates.
    private static let labelFormatter: DateFormatter = {
        let f = DateFormatter()
        f.setLocalizedDateFormatFromTemplate("EEEEdMMMM")
        return f
    }()

    let date: Date
    let items: [Item]
    let inMonth: Bool
    let isToday: Bool
    let isSelected: Bool

    private static let shown = 3

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(String(Calendar.current.component(.day, from: date)))
                .font(.callout.weight(isToday ? .semibold : .regular))
                .foregroundStyle(isToday ? Color.white : (inMonth ? Color.primary : Color.secondary.opacity(0.6)))
                .frame(minWidth: 22, minHeight: 22)
                .background {
                    if isToday { Circle().fill(Color.accentColor) }
                }
                .frame(maxWidth: .infinity, alignment: .trailing)

            ForEach(items.prefix(Self.shown)) { item in
                HStack(spacing: 4) {
                    RoundedRectangle(cornerRadius: 1.5)
                        .fill(item.done ? Color.secondary.opacity(0.5) : Color.accentColor)
                        .frame(width: 3, height: 12)
                    Text(item.startTime.map { "\($0) \(item.title)" } ?? item.title)
                        .font(.caption)
                        .strikethrough(item.done)
                        .foregroundStyle(item.done ? .secondary : .primary)
                        .lineLimit(1)
                }
            }
            if items.count > Self.shown {
                Text("\(items.count - Self.shown) more")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .padding(4)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(isSelected ? Color.accentColor.opacity(0.12) : Color.clear)
        .overlay(alignment: .bottom) { Divider() }
        .overlay(alignment: .trailing) { Divider() }
        .opacity(inMonth ? 1 : 0.75)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(
            "\(Self.labelFormatter.string(from: date)), \(items.count) items")
    }
}
