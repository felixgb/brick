#pragma once

#include <QList>
#include <QObject>

#include "Item.h"

class Core;

// Every to-do and calendar item, held in memory. The engine answers from its
// local database, so reloading after a change is instant; views redraw from
// changed().
class ItemStore : public QObject {
    Q_OBJECT
public:
    explicit ItemStore(Core *core, QObject *parent = nullptr);

    const QList<Item> &items() const { return m_items; }
    QList<Item> on(const QString &day) const;

    // Returns an error message, or an empty string on success.
    QString save(const Item &item);
    void setDone(const Item &item, bool done);
    void remove(const Item &item);
    struct ImportResult {
        int added = 0;
        int skipped = 0;
        int repeatsDropped = 0;
        QString error;
    };
    // The whole list as iCalendar text; empty with *error set on failure.
    QString exportIcs(QString *error) const;
    ImportResult importIcs(const QString &text);
    // Adds the calendar invitation carried by an email, linked back to it.
    ImportResult importInvitation(qint64 messageId);

    // Items made from an email, for showing under it.
    QList<Item> forMessage(qint64 messageId) const;

    void reload();

signals:
    void changed();
    void problem(const QString &message);

private:
    Core *m_core;
    QList<Item> m_items;
};
