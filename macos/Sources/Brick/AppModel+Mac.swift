import AppKit
import UniformTypeIdentifiers

/// The Mac's save and open panels around the shared model's file actions.
extension AppModel {
    private static var calendarTypes: [UTType] { [UTType(filenameExtension: "ics")].compactMap { $0 } }

    func saveAttachment(_ attachment: Attachment) {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = attachment.name
        guard panel.runModal() == .OK, let url = panel.url else { return }
        saveAttachment(attachment, to: url)
    }

    func exportCalendar() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "Brick calendar.ics"
        panel.allowedContentTypes = Self.calendarTypes
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try calendarText().write(to: url, atomically: true, encoding: .utf8)
            notify("Exported to \(url.lastPathComponent).")
        } catch {
            banner = error.localizedDescription
        }
    }

    func importCalendar() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = Self.calendarTypes
        panel.allowsMultipleSelection = true
        guard panel.runModal() == .OK else { return }
        importCalendars(panel.urls)
    }
}
