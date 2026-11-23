import SwiftUI

/// The day and time controls, shared by the editor and the Claude review sheet.
struct ItemWhenFields: View {
    @Binding var item: Item

    var body: some View {
        Toggle("On a day", isOn: hasDay)
        if item.date != nil {
            DatePicker("Day", selection: day, displayedComponents: .date)
            Toggle("Ends on another day", isOn: hasEndDay)
            if item.endDate != nil {
                DatePicker("Ends on", selection: endDay, in: dayAfter..., displayedComponents: .date)
            }
            Toggle("At a time", isOn: hasStart)
            if item.startTime != nil {
                DatePicker("Starts", selection: time(\.startTime), displayedComponents: .hourAndMinute)
                Toggle("Has an end time", isOn: hasEnd)
                if item.endTime != nil {
                    DatePicker("Ends", selection: time(\.endTime), displayedComponents: .hourAndMinute)
                }
            }
            Picker("Repeat", selection: repeatRule) {
                Text("Never").tag("")
                Text("Every day").tag("daily")
                Text("Every week").tag("weekly")
                Text("Every month").tag("monthly")
                Text("Every year").tag("yearly")
            }
            if item.isRepeating {
                Toggle("Until", isOn: hasUntil)
                if item.repeatUntil != nil {
                    DatePicker("Last day", selection: until, in: dayStart..., displayedComponents: .date)
                }
            }
        }
    }

    private var dayStart: Date { item.date.flatMap(Day.date) ?? .now }

    private var dayAfter: Date {
        Calendar.current.date(byAdding: .day, value: 1, to: dayStart) ?? dayStart
    }

    private var hasEndDay: Binding<Bool> {
        Binding {
            item.endDate != nil
        } set: { on in
            item.endDate = on ? Day.string(dayAfter) : nil
        }
    }

    private var endDay: Binding<Date> {
        Binding {
            item.endDate.flatMap(Day.date) ?? dayAfter
        } set: { new in
            item.endDate = Day.string(new)
        }
    }

    private var repeatRule: Binding<String> {
        Binding {
            item.repeat
        } set: { rule in
            item.repeat = rule
            if rule.isEmpty { item.repeatUntil = nil }
        }
    }

    private var hasUntil: Binding<Bool> {
        Binding {
            item.repeatUntil != nil
        } set: { on in
            let year = Calendar.current.date(byAdding: .year, value: 1, to: dayStart) ?? dayStart
            item.repeatUntil = on ? Day.string(year) : nil
        }
    }

    private var until: Binding<Date> {
        Binding {
            item.repeatUntil.flatMap(Day.date) ?? dayStart
        } set: { new in
            item.repeatUntil = Day.string(new)
        }
    }

    private var hasDay: Binding<Bool> {
        Binding {
            item.date != nil
        } set: { on in
            if on {
                item.date = item.date ?? Day.today
            } else {
                item.date = nil
                item.endDate = nil
                item.startTime = nil
                item.endTime = nil
                item.repeat = ""
                item.repeatUntil = nil
            }
        }
    }

    private var day: Binding<Date> {
        Binding {
            item.date.flatMap(Day.date) ?? .now
        } set: { new in
            item.date = Day.string(new)
            // A span that would now end before it starts no longer makes sense.
            if let end = item.endDate, end <= Day.string(new) { item.endDate = nil }
            if let last = item.repeatUntil, last < Day.string(new) { item.repeatUntil = Day.string(new) }
        }
    }

    private var hasStart: Binding<Bool> {
        Binding {
            item.startTime != nil
        } set: { on in
            item.startTime = on ? (item.startTime ?? "09:00") : nil
            if !on { item.endTime = nil }
        }
    }

    private var hasEnd: Binding<Bool> {
        Binding {
            item.endTime != nil
        } set: { on in
            guard on else {
                item.endTime = nil
                return
            }
            // Default to an hour after the start.
            let start = item.startTime.flatMap(Day.time) ?? .now
            item.endTime = Day.timeString(start.addingTimeInterval(3600))
        }
    }

    private func time(_ field: WritableKeyPath<Item, String?>) -> Binding<Date> {
        Binding {
            item[keyPath: field].flatMap(Day.time) ?? .now
        } set: { new in
            item[keyPath: field] = Day.timeString(new)
        }
    }
}
