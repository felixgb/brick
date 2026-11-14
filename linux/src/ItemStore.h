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
