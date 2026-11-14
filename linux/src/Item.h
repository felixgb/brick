#pragma once

#include <QDate>
#include <QJsonObject>
#include <QString>

// Days are "yyyy-MM-dd" strings and times "HH:mm", local time, exactly as the
// engine stores them. Comparing the strings compares the dates.
namespace Day {
inline QString string(const QDate &d) { return d.toString(Qt::ISODate); }
inline QDate date(const QString &s) { return QDate::fromString(s, Qt::ISODate); }
inline QString today() { return string(QDate::currentDate()); }
} // namespace Day

// One thing to do or attend. With no date it is only a to-do; with a day it is
// also on the calendar; with times it is a timed entry. Empty strings mean
// "not set" for the date and time fields.
struct Item {
    qint64 id = 0;
    QString title;
    QString notes;
    QString location;
    QString date;
    QString endDate;
    QString startTime;
    QString endTime;
    bool done = false;
    qint64 sourceAccountId = 0;
    QString sourceMessageId;
    QString sourceSubject;
    QString sourceFrom;

    static Item fromJson(const QJsonObject &o);
    // The form the engine's items.save expects.
    QJsonObject toJson() const;

    bool isFromEmail() const { return !sourceMessageId.isEmpty(); }
    bool isOverdue() const;
    // Whether the item falls on the given day, counting every day of a span.
    bool occursOn(const QString &day) const;
    // "Thu 12 Nov, 14:00 to 15:00", or empty for an undated item.
    QString whenLabel() const;
};
