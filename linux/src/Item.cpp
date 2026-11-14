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
    i.repeat = o.value("repeat").toString();
    i.repeatUntil = o.value("repeat_until").toString();
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
        {"repeat", repeat},
        {"repeat_until", orNull(repeatUntil)},
    };
}

bool Item::isOverdue() const {
    if (done || date.isEmpty())
        return false;
    return (endDate.isEmpty() ? date : endDate) < Day::today();
}

// Whether an occurrence of the repeating item starting at `start` begins on `o`.
// Monthly and yearly repeats on the 29th-31st fall on the last day of a short month.
static bool recursOn(const QDate &o, const QDate &start, const QString &rule) {
    if (rule == QLatin1String("daily"))
        return true;
    if (rule == QLatin1String("weekly"))
        return start.daysTo(o) % 7 == 0;
    const int wanted = qMin(start.day(), o.daysInMonth());
    if (rule == QLatin1String("monthly"))
        return o.day() == wanted;
    if (rule == QLatin1String("yearly"))
        return o.month() == start.month() && o.day() == wanted;
    return false;
}

bool Item::startsOn(const QDate &o) const {
    const QDate start = Day::date(date);
    if (!start.isValid() || !o.isValid() || o < start)
        return false;
    if (repeat.isEmpty())
        return o == start;
    const QDate until = Day::date(repeatUntil);
    if (until.isValid() && o > until)
        return false;
    return recursOn(o, start, repeat);
}

bool Item::occursOn(const QString &day) const {
    if (date.isEmpty())
        return false;
    if (repeat.isEmpty())
        return date <= day && day <= (endDate.isEmpty() ? date : endDate);

    const QDate d = Day::date(day);
    const QDate start = Day::date(date);
    if (!d.isValid() || !start.isValid() || d < start)
        return false;
    const QDate lastDay = Day::date(endDate);
    const int span = lastDay.isValid() ? int(start.daysTo(lastDay)) : 0;
    // An occurrence covering `d` started on d, or up to `span` days before.
    for (int k = 0; k <= span; ++k) {
        if (startsOn(d.addDays(-k)))
            return true;
    }
    return false;
}

QString Item::repeatLabel() const {
    if (repeat.isEmpty())
        return {};
    QString text = QObject::tr("Repeats %1").arg(repeat == QLatin1String("daily")     ? QObject::tr("daily")
                                                 : repeat == QLatin1String("weekly")  ? QObject::tr("weekly")
                                                 : repeat == QLatin1String("monthly") ? QObject::tr("monthly")
                                                                                      : QObject::tr("yearly"));
    const QDate u = Day::date(repeatUntil);
    if (u.isValid())
        text += QObject::tr(" until ") + QLocale().toString(u, QStringLiteral("d MMM yyyy"));
    return text;
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
