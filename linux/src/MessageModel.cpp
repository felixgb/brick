#include "MessageModel.h"

#include <QApplication>
#include <QDateTime>
#include <QFont>
#include <QLocale>
#include <QPalette>

#include "Core.h"

MessageModel::MessageModel(Core *core, QObject *parent) : QAbstractTableModel(parent), m_core(core) {}

void MessageModel::showFolder(qint64 folderId) {
    beginResetModel();
    m_folderId = folderId;
    m_query.clear();
    load();
    endResetModel();
}

void MessageModel::showSearch(const QString &query) {
    beginResetModel();
    m_query = query;
    load();
    endResetModel();
}

void MessageModel::refresh() {
    beginResetModel();
    load();
    endResetModel();
}

void MessageModel::load() {
    m_pages.clear();
    m_total = 0;
    if (!m_query.isEmpty()) {
        const QJsonObject r = m_core->call("messages.search", {{"query", m_query}, {"limit", 500}}).toObject();
        const QJsonArray rows = r.value("rows").toArray();
        m_total = rows.size();
        // Search results arrive whole; file them as pages so lookups are uniform.
        for (int i = 0; i < rows.size(); i += PageSize) {
            QJsonArray chunk;
            for (int j = i; j < qMin(i + PageSize, int(rows.size())); ++j)
                chunk.append(rows.at(j));
            m_pages.insert(i / PageSize, chunk);
        }
    } else if (m_folderId != 0) {
        const QJsonObject r =
            m_core->call("messages.list", {{"folder_id", m_folderId}, {"offset", 0}, {"limit", PageSize}}).toObject();
        m_total = r.value("total").toInt();
        m_pages.insert(0, r.value("rows").toArray());
    }
}

const QJsonArray &MessageModel::page(int pageIndex) const {
    auto it = m_pages.constFind(pageIndex);
    if (it == m_pages.constEnd()) {
        QJsonArray rows;
        if (m_query.isEmpty() && m_folderId != 0) {
            rows = m_core
                       ->call("messages.list",
                              {{"folder_id", m_folderId}, {"offset", pageIndex * PageSize}, {"limit", PageSize}})
                       .toObject()
                       .value("rows")
                       .toArray();
        }
        it = m_pages.insert(pageIndex, rows);
    }
    return it.value();
}

QJsonObject MessageModel::message(int row) const {
    if (row < 0 || row >= m_total)
        return {};
    return page(row / PageSize).at(row % PageSize).toObject();
}

int MessageModel::rowOfMessage(qint64 id) const {
    // Only pages already in memory are searched; that covers what is on screen.
    for (auto it = m_pages.constBegin(); it != m_pages.constEnd(); ++it) {
        const QJsonArray &rows = it.value();
        for (int i = 0; i < rows.size(); ++i) {
            if (qint64(rows.at(i).toObject().value("id").toDouble()) == id)
                return it.key() * PageSize + i;
        }
    }
    return -1;
}

int MessageModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : m_total;
}

int MessageModel::columnCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : ColumnCount;
}

static QString formatDate(qint64 secs) {
    if (secs <= 0)
        return {};
    const QDateTime when = QDateTime::fromSecsSinceEpoch(secs);
    const QDate today = QDate::currentDate();
    const QLocale locale;
    if (when.date() == today)
        return locale.toString(when.time(), QLocale::ShortFormat);
    if (when.date().daysTo(today) < 7)
        return locale.toString(when, QStringLiteral("ddd HH:mm"));
    if (when.date().year() == today.year())
        return locale.toString(when.date(), QStringLiteral("d MMM"));
    return locale.toString(when.date(), QLocale::ShortFormat);
}

QVariant MessageModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid())
        return {};
    const QJsonObject m = message(index.row());
    if (m.isEmpty())
        return {};
    const bool unread = !m.value("seen").toBool();

    switch (role) {
    case Qt::DisplayRole:
        switch (index.column()) {
        case Star:
            return m.value("flagged").toBool() ? QStringLiteral("★") : QString();
        case From: {
            const QString name = m.value("from_name").toString();
            return name.isEmpty() ? m.value("from_addr").toString() : name;
        }
        case Subject: {
            QString subject = m.value("subject").toString();
            if (subject.isEmpty())
                subject = tr("(no subject)");
            if (m.value("has_attachments").toBool())
                subject.prepend(QStringLiteral("📎 "));
            const QString snippet = m.value("snippet").toString();
            return snippet.isEmpty() ? subject : subject + QStringLiteral("  —  ") + snippet;
        }
        case Date:
            return formatDate(qint64(m.value("date").toDouble()));
        }
        break;
    case Qt::FontRole:
        if (unread) {
            QFont f = QApplication::font();
            f.setBold(true);
            return f;
        }
        break;
    case Qt::ForegroundRole:
        if (index.column() == Star)
            return QColor(0xE0, 0x9B, 0x00);
        break;
    case Qt::TextAlignmentRole:
        if (index.column() == Date)
            return int(Qt::AlignRight | Qt::AlignVCenter);
        if (index.column() == Star)
            return int(Qt::AlignCenter);
        break;
    case Qt::ToolTipRole:
        if (index.column() == From)
            return m.value("from_addr").toString();
        break;
    }
    return {};
}

QVariant MessageModel::headerData(int section, Qt::Orientation orientation, int role) const {
    if (orientation != Qt::Horizontal || role != Qt::DisplayRole)
        return {};
    switch (section) {
    case Star:
        return QStringLiteral("★");
    case From:
        return tr("From");
    case Subject:
        return tr("Subject");
    case Date:
        return tr("Date");
    }
    return {};
}
