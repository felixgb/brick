#include "Item.h"

#include <QLocale>

namespace {

QJsonValue orNull(const QString &s) {
    return s.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(s);
}

QString dayLabel(const QDate &d) {
    const QDate today = QDate::currentDate();
    if (d == today)
        return QObject::tr("Today");
    if (d == today.addDays(1))
        return QObject::tr("Tomorrow");
    if (d == today.addDays(-1))
        return QObject::tr("Yesterday");
    if (d.year() == today.year())
        return QLocale().toString(d, QStringLiteral("ddd d MMM"));
    return QLocale().toString(d, QStringLiteral("d MMM yyyy"));
}

} // namespace

Item Item::fromJson(const QJsonObject &o) {
    Item i;
    i.id = qint64(o.value("id").toDouble());
    i.title = o.value("title").toString();
    i.notes = o.value("notes").toString();
    i.location = o.value("location").toString();
    i.date = o.value("date").toString();
    i.endDate = o.value("end_date").toString();
    i.startTime = o.value("start_time").toString();
    i.endTime = o.value("end_time").toString();
    i.done = o.value("done").toBool();
    i.sourceAccountId = qint64(o.value("source_account_id").toDouble());
    i.sourceMessageId = o.value("source_message_id").toString();
    i.sourceSubject = o.value("source_subject").toString();
    i.sourceFrom = o.value("source_from").toString();
    return i;
}

QJsonObject Item::toJson() const {
    return {
        {"id", double(id)},
        {"title", title},
        {"notes", notes},
        {"location", location},
        {"date", orNull(date)},
        {"end_date", orNull(endDate)},
        {"start_time", orNull(startTime)},
        {"end_time", orNull(endTime)},
        {"done", done},
        {"source_account_id", sourceAccountId ? QJsonValue(double(sourceAccountId)) : QJsonValue(QJsonValue::Null)},
        {"source_message_id", sourceMessageId},
        {"source_subject", sourceSubject},
        {"source_from", sourceFrom},
    };
}

bool Item::isOverdue() const {
    if (done || date.isEmpty())
        return false;
    return (endDate.isEmpty() ? date : endDate) < Day::today();
}

bool Item::occursOn(const QString &day) const {
    if (date.isEmpty())
        return false;
    return date <= day && day <= (endDate.isEmpty() ? date : endDate);
}

QString Item::whenLabel() const {
    const QDate start = Day::date(date);
    if (!start.isValid())
        return {};
    QString text = dayLabel(start);
    const QDate end = Day::date(endDate);
    if (end.isValid())
        text += QObject::tr(" to ") + dayLabel(end);
    if (!startTime.isEmpty()) {
        text += QStringLiteral(", ") + startTime;
        if (!endTime.isEmpty())
            text += QObject::tr(" to ") + endTime;
    }
    return text;
}
