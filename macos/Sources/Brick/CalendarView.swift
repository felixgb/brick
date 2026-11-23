import SwiftUI

/// The Calendar tab: month, week or day on the left, the selected day's items alongside.
struct CalendarView: View {
    @Environment(AppModel.self) private var model
    @State private var mode: CalendarMode = .month
    /// The selected day; every view is shown around it.
    @State private var selectedDay = Day.today

    private let calendar = Calendar.current

    /// Any moment inside what is on screen.
    private var anchor: Date { Day.date(selectedDay) ?? .now }

    var body: some View {
        HStack(spacing: 0) {
            VStack(spacing: 0) {
                switch mode {
                case .month:
                    weekdayHeader
                    Divider()
                    monthGrid
                case .week:
                    TimeGridView(days: weekDays, selectedDay: $selectedDay)
                case .day:
                    TimeGridView(days: [anchor], selectedDay: $selectedDay)
                }
            }
            Divider()
            dayPanel
                .frame(width: 280)
        }
        .background(.background)
        .navigationTitle(title)
        .navigationSubtitle("")
        .toolbar {
            ToolbarItemGroup(placement: .navigation) {
                Button("Previous", systemImage: "chevron.left") { shift(-1) }
                    .help("Previous \(mode.rawValue)")
                Button("Today") { selectedDay = Day.today }
                    .help("Go to today")
                Button("Next", systemImage: "chevron.right") { shift(1) }
                    .help("Next \(mode.rawValue)")
            }
            ToolbarItem(placement: .primaryAction) {
                Picker("Show", selection: $mode) {
                    ForEach(CalendarMode.allCases) { mode in
                        Text(mode.title).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .help("Month, week or day")
            }
            ToolbarItem(placement: .primaryAction) {
                Button("New item", systemImage: "plus") { model.newItem(on: selectedDay) }
                    .help("Add an item on the selected day")
            }
        }
    }

    private var title: String {
        switch mode {
        case .month:
            return anchor.formatted(.dateTime.month(.wide).year())
        case .week:
            guard let first = weekDays.first, let last = weekDays.last else { return "" }
            let from = first.formatted(.dateTime.day().month(.abbreviated))
            return "\(from) to \(last.formatted(.dateTime.day().month(.abbreviated).year()))"
        case .day:
            return anchor.formatted(.dateTime.weekday(.wide).day().month(.wide).year())
        }
    }

    /// The seven days of the week the selected day is in.
    private var weekDays: [Date] {
        guard let start = calendar.dateInterval(of: .weekOfYear, for: anchor)?.start else { return [anchor] }
        return (0..<7).compactMap { calendar.date(byAdding: .day, value: $0, to: start) }
    }

    // MARK: Grid

    /// The 42 days shown: whole weeks covering the month.
    private var days: [Date] {
        guard let interval = calendar.dateInterval(of: .month, for: anchor) else { return [] }
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
                        inMonth: calendar.isDate(date, equalTo: anchor, toGranularity: .month),
                        isToday: calendar.isDateInToday(date),
                        isSelected: key == selectedDay
                    )
                    .equatable()
                    .frame(height: geometry.size.height / 6)
                    .contentShape(Rectangle())
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

    private func shift(_ steps: Int) {
        let unit: Calendar.Component = mode == .month ? .month : (mode == .week ? .weekOfYear : .day)
        if let moved = calendar.date(byAdding: unit, value: steps, to: anchor) {
            selectedDay = Day.string(moved)
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

/// The week and day views: an hour grid with an all-day strip above it. Timed
/// items are blocks (overlapping ones share the width); items with no time sit
/// in the strip.
private struct TimeGridView: View {
    @Environment(AppModel.self) private var model
    let days: [Date]
    @Binding var selectedDay: String

    private static let hourHeight: CGFloat = 48
    private static let gutter: CGFloat = 54

    private struct Segment: Identifiable {
        let item: Item
        var from: Int  // minutes from midnight
        var to: Int
        var lane = 0
        var lanes = 1
        var id: Int64 { item.id }
    }

    private struct DayData {
        var allDay: [Item] = []
        var timed: [Segment] = []
    }

    var body: some View {
        // Work everything out once per redraw rather than once per use.
        let data = days.map(layout)
        let earliest = data.flatMap(\.timed).map(\.from).min() ?? 8 * 60
        let scrollHour = max(0, min(earliest / 60, 8) - 1)

        VStack(spacing: 0) {
            header(data)
            Divider()
            ScrollViewReader { proxy in
                ScrollView {
                    HStack(alignment: .top, spacing: 0) {
                        hourLabels
                        ForEach(Array(days.enumerated()), id: \.offset) { index, date in
                            column(date, data[index])
                        }
                    }
                    .padding(.top, 8)
                }
                .onAppear { proxy.scrollTo(scrollHour, anchor: .top) }
            }
        }
    }

    // MARK: Layout

    private func minutes(_ time: String?) -> Int? {
        guard let time else { return nil }
        let parts = time.split(separator: ":")
        guard parts.count >= 2, let h = Int(parts[0]), let m = Int(parts[1]) else { return nil }
        return h * 60 + m
    }

    private func layout(_ date: Date) -> DayData {
        let calendar = Calendar.current
        var result = DayData()
        for item in model.itemsOn(Day.string(date)) {
            guard let start = minutes(item.startTime), let first = item.date.flatMap(Day.date) else {
                result.allDay.append(item)
                continue
            }
            // A timed item that spans days runs from its start to midnight,
            // fills the days between, and ends at its end time.
            let last = item.endDate.flatMap(Day.date) ?? first
            let span = max(0, calendar.dateComponents([.day], from: first, to: last).day ?? 0)
            var startOffset = 0
            for back in 0...span {
                guard let day = calendar.date(byAdding: .day, value: -back, to: date) else { continue }
                if item.startsOn(Day.string(day)) {
                    startOffset = back
                    break
                }
            }
            var from = startOffset == 0 ? start : 0
            var to = startOffset == span ? (minutes(item.endTime) ?? from + 60) : 24 * 60
            if to <= from {
                from = min(from, 24 * 60 - 15)
                to = min(24 * 60, from + 60)
            }
            result.timed.append(Segment(item: item, from: from, to: to))
        }

        // Lanes: overlapping blocks sit side by side, each in the first free column.
        result.timed.sort { $0.from < $1.from }
        var laneEnd: [Int] = []
        var clusterStart = 0
        for k in result.timed.indices {
            var lane = 0
            while lane < laneEnd.count && laneEnd[lane] > result.timed[k].from { lane += 1 }
            if lane == laneEnd.count { laneEnd.append(0) }
            laneEnd[lane] = result.timed[k].to
            result.timed[k].lane = lane
            // A gap with no overlap ends the cluster: its blocks share one lane count.
            let isLast = k + 1 == result.timed.count
            if isLast || result.timed[k + 1].from >= (laneEnd.max() ?? 0) {
                for j in clusterStart...k { result.timed[j].lanes = laneEnd.count }
                laneEnd.removeAll()
                clusterStart = k + 1
            }
        }
        return result
    }

    // MARK: Pieces

    private func header(_ data: [DayData]) -> some View {
        HStack(alignment: .top, spacing: 0) {
            Color.clear.frame(width: Self.gutter, height: 1)
            ForEach(Array(days.enumerated()), id: \.offset) { index, date in
                let key = Day.string(date)
                let isToday = Calendar.current.isDateInToday(date)
                VStack(spacing: 3) {
                    Text(date.formatted(.dateTime.weekday(.abbreviated)))
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.secondary)
                    Text(date.formatted(.dateTime.day()))
                        .font(.callout.weight(isToday ? .semibold : .regular))
                        .foregroundStyle(isToday ? Color.white : Color.primary)
                        .frame(minWidth: 24, minHeight: 24)
                        .background {
                            if isToday { Circle().fill(Color.accentColor) }
                        }
                    ForEach(data[index].allDay.prefix(3)) { item in
                        Text(item.title)
                            .font(.caption)
                            .strikethrough(item.done)
                            .lineLimit(1)
                            .padding(.horizontal, 4)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(Color.accentColor.opacity(item.done ? 0.08 : 0.2), in: RoundedRectangle(cornerRadius: 3))
                            .onTapGesture(count: 2) { model.editingItem = item }
                    }
                    if data[index].allDay.count > 3 {
                        Text("\(data[index].allDay.count - 3) more")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 4)
                .padding(.horizontal, 2)
                .frame(maxWidth: .infinity)
                .background(key == selectedDay ? Color.accentColor.opacity(0.12) : Color.clear)
                .contentShape(Rectangle())
                .onTapGesture { selectedDay = key }
            }
        }
    }

    private var hourLabels: some View {
        VStack(spacing: 0) {
            ForEach(0..<24, id: \.self) { hour in
                Text(hour == 0 ? "" : String(format: "%02d:00", hour))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .frame(width: Self.gutter - 6, height: Self.hourHeight, alignment: .topTrailing)
                    .offset(y: -6)
                    .id(hour)
            }
        }
        .frame(width: Self.gutter - 6)
        .padding(.trailing, 6)
    }

    private func column(_ date: Date, _ data: DayData) -> some View {
        let key = Day.string(date)
        let height = Self.hourHeight * 24
        let isToday = Calendar.current.isDateInToday(date)

        return ZStack(alignment: .topLeading) {
            // Hour lines, with the selected day tinted.
            VStack(spacing: 0) {
                ForEach(0..<24, id: \.self) { _ in
                    Color.clear
                        .frame(height: Self.hourHeight)
                        .overlay(alignment: .top) { Divider() }
                }
            }
            .background(key == selectedDay ? Color.accentColor.opacity(0.06) : Color.clear)

            GeometryReader { geometry in
                ForEach(data.timed) { segment in
                    let width = geometry.size.width / CGFloat(segment.lanes)
                    block(segment)
                        .frame(
                            width: max(8, width - 2),
                            height: max(18, CGFloat(segment.to - segment.from) * Self.hourHeight / 60 - 1),
                            alignment: .topLeading
                        )
                        .offset(x: CGFloat(segment.lane) * width, y: CGFloat(segment.from) * Self.hourHeight / 60)
                }
                if isToday {
                    let now = Calendar.current.dateComponents([.hour, .minute], from: .now)
                    let y = CGFloat((now.hour ?? 0) * 60 + (now.minute ?? 0)) * Self.hourHeight / 60
                    Rectangle()
                        .fill(Color.red)
                        .frame(width: geometry.size.width, height: 1.5)
                        .offset(y: y)
                        .allowsHitTesting(false)
                }
            }
        }
        .frame(maxWidth: .infinity, minHeight: height, maxHeight: height)
        .overlay(alignment: .leading) { Divider() }
        .contentShape(Rectangle())
        // Select on the first click; a double click on an empty slot starts an
        // item at that time (see the note in CalendarView on gesture ordering).
        .onTapGesture { selectedDay = key }
        .simultaneousGesture(
            SpatialTapGesture(count: 2).onEnded { tap in
                let half = min(47, max(0, Int(tap.location.y / Self.hourHeight * 2)))
                model.newItem(on: key, at: String(format: "%02d:%02d", half / 2, (half % 2) * 30))
            }
        )
    }

    private func block(_ segment: Segment) -> some View {
        let item = segment.item
        return VStack(alignment: .leading, spacing: 1) {
            Text(item.title)
                .font(.caption.weight(.medium))
                .strikethrough(item.done)
                .lineLimit(1)
            if segment.to - segment.from >= 45, !item.location.isEmpty {
                Text(item.location)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 4)
        .padding(.vertical, 2)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color.accentColor.opacity(item.done ? 0.08 : 0.22), in: RoundedRectangle(cornerRadius: 4))
        .overlay(alignment: .leading) {
            RoundedRectangle(cornerRadius: 1.5)
                .fill(item.done ? Color.secondary.opacity(0.5) : Color.accentColor)
                .frame(width: 3)
        }
        .contentShape(Rectangle())
        .onTapGesture(count: 2) { model.editingItem = item }
        .help(item.whenLabel ?? item.title)
    }
}
