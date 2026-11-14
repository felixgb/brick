#include "ItemStore.h"

#include <QJsonArray>

#include "Core.h"

ItemStore::ItemStore(Core *core, QObject *parent) : QObject(parent), m_core(core) {
    connect(core, &Core::event, this, [this](const QJsonObject &e) {
        if (e.value("type").toString() == QLatin1String("items_changed"))
            reload();
    });
    reload();
}

void ItemStore::reload() {
    m_items.clear();
    const QJsonArray rows = m_core->call("items.list").toArray();
    m_items.reserve(rows.size());
    for (const QJsonValue &v : rows)
        m_items.append(Item::fromJson(v.toObject()));
    emit changed();
}

QList<Item> ItemStore::on(const QString &day) const {
    QList<Item> out;
    for (const Item &i : m_items) {
        if (i.occursOn(day))
            out.append(i);
    }
    return out;
}

QString ItemStore::save(const Item &item) {
    QString error;
    m_core->call("items.save", {{"item", item.toJson()}}, &error);
    if (error.isEmpty())
        reload();
    return error;
}

void ItemStore::setDone(const Item &item, bool done) {
    // Show the tick at once; the engine confirms with items_changed. A
    // repeating item doesn't stay ticked: the engine moves it to its next
    // occurrence, so wait for that instead.
    if (item.repeat.isEmpty()) {
        for (Item &i : m_items) {
            if (i.id == item.id)
                i.done = done;
        }
        emit changed();
    }
    QString error;
    m_core->call("items.set_done", {{"id", double(item.id)}, {"done", done}}, &error);
    if (!error.isEmpty()) {
        reload();
        emit problem(error);
    } else if (!item.repeat.isEmpty()) {
        reload();
    }
}

static ItemStore::ImportResult resultOf(const QJsonValue &v, const QString &error) {
    ItemStore::ImportResult r;
    r.error = error;
    const QJsonObject o = v.toObject();
    r.added = o.value("added").toInt();
    r.skipped = o.value("skipped").toInt();
    r.repeatsDropped = o.value("repeats_dropped").toInt();
    return r;
}

QString ItemStore::exportIcs(QString *error) const {
    const QJsonValue v = m_core->call("items.export_ics", {}, error);
    return v.toString();
}

ItemStore::ImportResult ItemStore::importIcs(const QString &text) {
    QString error;
    const QJsonValue v = m_core->call("items.import_ics", {{"text", text}}, &error);
    reload();
    return resultOf(v, error);
}

ItemStore::ImportResult ItemStore::importInvitation(qint64 messageId) {
    QString error;
    const QJsonValue v = m_core->call("items.import_invitation", {{"id", double(messageId)}}, &error);
    reload();
    return resultOf(v, error);
}

void ItemStore::remove(const Item &item) {
    QString error;
    m_core->call("items.delete", {{"id", double(item.id)}}, &error);
    reload();
    if (!error.isEmpty())
        emit problem(error);
}

QList<Item> ItemStore::forMessage(qint64 messageId) const {
    QList<Item> out;
    const QJsonArray rows = m_core->call("items.for_message", {{"id", double(messageId)}}).toArray();
    for (const QJsonValue &v : rows)
        out.append(Item::fromJson(v.toObject()));
    return out;
}
