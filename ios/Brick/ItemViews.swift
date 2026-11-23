import SwiftUI

/// One item in a list: a tick box, the title, and when and where it is.
/// Tapping the row opens the editor.
struct ItemRowView: View {
    @Environment(AppModel.self) private var model
    let item: Item
    /// Hide the day when the surrounding list already says which day it is.
    var showsDay = true

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Button {
                model.setDone(item, !item.done)
            } label: {
                Image(systemName: item.done ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(item.done ? Color.accentColor : Color.secondary)
                    .font(.title3)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(item.done ? "Mark as not done" : "Mark as done")

            Button {
                model.editingItem = item
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 4) {
                        Text(item.title)
                            .strikethrough(item.done)
                            .foregroundStyle(item.done ? .secondary : .primary)
                            .lineLimit(2)
                        if item.isRepeating {
                            Image(systemName: "repeat")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        if item.isFromEmail {
                            Image(systemName: "envelope")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    if let detail {
                        Text(detail)
                            .font(.subheadline)
                            .foregroundStyle(item.isOverdue ? Color.red : Color.secondary)
                            .lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
        .swipeActions(edge: .trailing) {
            Button("Delete", systemImage: "trash", role: .destructive) { model.delete(item) }
        }
        .contextMenu {
            Button(item.done ? "Mark as Not Done" : "Mark as Done", systemImage: "checkmark.circle") {
                model.setDone(item, !item.done)
            }
            if item.isFromEmail {
                Button("Open Email", systemImage: "envelope") { model.openSource(of: item) }
            }
            Button("Delete", systemImage: "trash", role: .destructive) { model.delete(item) }
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
        NavigationStack {
            Form {
                Section {
                    TextField("Title", text: $item.title)
                        .focused($titleFocused)
                    TextField("Location", text: $item.location)
                }
                Section {
                    ItemWhenFields(item: $item)
                }
                Section {
                    TextField("Notes", text: $item.notes, axis: .vertical)
                        .lineLimit(3...10)
                    if !isNew {
                        Toggle("Done", isOn: $item.done)
                    }
                }
                if item.isFromEmail {
                    Section("From email") {
                        if isNew {
                            Text(item.sourceSubject)
                        } else {
                            Button {
                                model.openSource(of: item)
                            } label: {
                                Label(item.sourceSubject, systemImage: "envelope")
                            }
                        }
                    }
                }
                if let problem {
                    Section {
                        Label(problem, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.red)
                    }
                }
                if !isNew {
                    Section {
                        Button("Delete", role: .destructive) {
                            model.delete(item)
                            dismiss()
                        }
                    }
                }
            }
            .navigationTitle(isNew ? "New Item" : "Edit Item")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(isNew ? "Add" : "Save") {
                        problem = model.save(item)
                        if problem == nil { dismiss() }
                    }
                    .disabled(item.title.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            .onAppear { titleFocused = isNew }
        }
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

        NavigationStack {
            List {
                Section {
                    HStack(spacing: 10) {
                        Image(systemName: "plus.circle").foregroundStyle(.secondary).font(.title3)
                        TextField("Add a to-do", text: $newTitle)
                            .submitLabel(.done)
                            .onSubmit(addQuick)
                    }
                }
                group("Overdue", overdue)
                group("Today", dueToday, showsDay: false)
                group("Upcoming", upcoming)
                group("No date", undated)
                if showDone {
                    group("Done", done)
                }
            }
            .overlay {
                if model.items.isEmpty {
                    ContentUnavailableView {
                        Label("Nothing to do yet", systemImage: "checklist")
                    } description: {
                        Text("Type a to-do above, or tap + for one with a date.")
                    }
                    .allowsHitTesting(false)
                }
            }
            .navigationTitle("To-do")
            .navigationSubtitle(open.isEmpty ? "All done" : "\(open.count) to do")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { SettingsButton() }
                ToolbarItemGroup(placement: .topBarTrailing) {
                    Button(showDone ? "Hide Done" : "Show Done",
                           systemImage: showDone ? "checkmark.circle.fill" : "checkmark.circle") {
                        showDone.toggle()
                    }
                    Button("New Item", systemImage: "plus") { model.newItem() }
                }
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
