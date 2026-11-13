import SwiftUI

/// One item in a list: a tick box, the title, and when and where it is.
struct ItemRowView: View {
    @Environment(AppModel.self) private var model
    let item: Item
    /// Hide the day when the surrounding list already says which day it is.
    var showsDay = true

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Button {
                model.setDone(item, !item.done)
            } label: {
                Image(systemName: item.done ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(item.done ? Color.accentColor : Color.secondary)
                    .font(.title3)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(item.done ? "Mark as not done" : "Mark as done")

            VStack(alignment: .leading, spacing: 2) {
                Text(item.title)
                    .strikethrough(item.done)
                    .foregroundStyle(item.done ? .secondary : .primary)
                    .lineLimit(2)
                if let detail {
                    Text(detail)
                        .font(.subheadline)
                        .foregroundStyle(item.isOverdue ? Color.red : Color.secondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            if item.isFromEmail {
                Button("Open email", systemImage: "envelope") { model.openSource(of: item) }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .help("Open the email this came from: \(item.sourceSubject)")
            }
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .onTapGesture(count: 2) { model.editingItem = item }
        .contextMenu {
            Button("Edit") { model.editingItem = item }
            Button(item.done ? "Mark as Not Done" : "Mark as Done") { model.setDone(item, !item.done) }
            if item.isFromEmail {
                Button("Open Email") { model.openSource(of: item) }
            }
            Divider()
            Button("Delete", role: .destructive) { model.delete(item) }
        }
    }

    private var detail: String? {
        var parts: [String] = []
        if showsDay, let when = item.whenLabel {
            parts.append(when)
        } else if let start = item.startTime {
            parts.append(item.endTime.map { "\(start) to \($0)" } ?? start)
        }
        if !item.location.isEmpty { parts.append(item.location) }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

/// The day and time controls, shared by the editor and the Claude review sheet.
struct ItemWhenFields: View {
    @Binding var item: Item

    var body: some View {
        Toggle("On a day", isOn: hasDay)
        if item.date != nil {
            DatePicker("Day", selection: day, displayedComponents: .date)
            Toggle("At a time", isOn: hasStart)
            if item.startTime != nil {
                DatePicker("Starts", selection: time(\.startTime), displayedComponents: .hourAndMinute)
                Toggle("Has an end time", isOn: hasEnd)
                if item.endTime != nil {
                    DatePicker("Ends", selection: time(\.endTime), displayedComponents: .hourAndMinute)
                }
            }
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

/// Sheet for creating or editing an item.
struct ItemEditor: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var item: Item
    @State private var problem: String?
    @FocusState private var titleFocused: Bool

    init(item: Item) {
        _item = State(initialValue: item)
    }

    private var isNew: Bool { item.id == 0 }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(isNew ? "New item" : "Edit item")
                .font(.headline)

            Form {
                TextField("Title", text: $item.title)
                    .focused($titleFocused)
                ItemWhenFields(item: $item)
                TextField("Location", text: $item.location)
                TextField("Notes", text: $item.notes, axis: .vertical)
                    .lineLimit(3...8)
                if !isNew {
                    Toggle("Done", isOn: $item.done)
                }
            }
            .formStyle(.columns)

            if item.isFromEmail {
                HStack(spacing: 6) {
                    Image(systemName: "envelope").foregroundStyle(.secondary)
                    Text("From “\(item.sourceSubject)”")
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Spacer()
                    if !isNew {
                        Button("Open email") { model.openSource(of: item) }
                    }
                }
                .font(.callout)
            }

            if let problem {
                Label(problem, systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .foregroundStyle(.red)
            }

            HStack {
                if !isNew {
                    Button("Delete", role: .destructive) {
                        model.delete(item)
                        dismiss()
                    }
                }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(isNew ? "Add" : "Save") { save() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(item.title.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 440)
        .onAppear { titleFocused = isNew }
    }

    private func save() {
        problem = model.save(item)
        if problem == nil { dismiss() }
    }
}

/// The To-do tab: every item, grouped by how soon it needs attention.
struct TodoView: View {
    @Environment(AppModel.self) private var model
    @State private var newTitle = ""
    @State private var showDone = false

    var body: some View {
        let today = Day.today
        let open = model.items.filter { !$0.done }
        let overdue = open.filter(\.isOverdue)
        let dueToday = open.filter { $0.occurs(on: today) }
        let upcoming = open.filter { ($0.date ?? "") > today }
        let undated = open.filter { $0.date == nil }
        let done = model.items.filter(\.done)

        List {
            Section {
                HStack(spacing: 8) {
                    Image(systemName: "plus.circle").foregroundStyle(.secondary).font(.title3)
                    TextField("Add a to-do", text: $newTitle)
                        .textFieldStyle(.plain)
                        .onSubmit(addQuick)
                }
                .padding(.vertical, 2)
            }
            group("Overdue", overdue)
            group("Today", dueToday, showsDay: false)
            group("Upcoming", upcoming)
            group("No date", undated)
            if showDone {
                group("Done", done)
            }
        }
        .listStyle(.inset)
        .frame(maxWidth: 760)
        .frame(maxWidth: .infinity)
        .background(.background)
        .overlay {
            if model.items.isEmpty {
                ContentUnavailableView {
                    Label("Nothing to do yet", systemImage: "checklist")
                } description: {
                    Text("Type a to-do above, or open an email and choose Add to To-do.")
                }
                .padding(.top, 60)
                .allowsHitTesting(false)
            }
        }
        .navigationTitle("To-do")
        .navigationSubtitle(open.isEmpty ? "All done" : "\(open.count) to do")
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Toggle("Show done", systemImage: "checkmark.circle", isOn: $showDone)
                    .help(showDone ? "Hide finished items" : "Show finished items")
                Button("New item", systemImage: "plus") { model.newItem() }
                    .help("Add an item with a date, time or notes")
            }
        }
    }

    @ViewBuilder
    private func group(_ title: String, _ items: [Item], showsDay: Bool = true) -> some View {
        if !items.isEmpty {
            Section(title) {
                ForEach(items) { item in
                    ItemRowView(item: item, showsDay: showsDay)
                }
            }
        }
    }

    private func addQuick() {
        let title = newTitle.trimmingCharacters(in: .whitespaces)
        guard !title.isEmpty else { return }
        if model.save(Item(title: title)) == nil {
            newTitle = ""
        }
    }
}
