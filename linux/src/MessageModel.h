#pragma once

#include <QAbstractTableModel>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>

class Core;

// The message list. Rows are read from the engine a page at a time as the
// view asks for them, so a folder with 100,000 messages opens as fast as one
// with ten.
class MessageModel : public QAbstractTableModel {
    Q_OBJECT
public:
    enum Column { Star, From, Subject, Date, ColumnCount };

    explicit MessageModel(Core *core, QObject *parent = nullptr);

    void showFolder(qint64 folderId);
    // Every folder with this role across accounts ("inbox" is the unified inbox).
    void showRole(const QString &role);
    void showSearch(const QString &query);
    // Groups the list into conversations: one row per conversation, showing its
    // newest message. Search results are never grouped.
    void setThreaded(bool on);
    bool threaded() const { return m_threaded && m_query.isEmpty(); }
    // Re-reads the current folder or search. The view's selection and scroll
    // position are reset; MainWindow restores them by message id.
    void refresh();

    qint64 folderId() const { return m_folderId; }
    QString role() const { return m_role; }
    bool isUnified() const { return m_query.isEmpty() && !m_role.isEmpty(); }
    bool isSearch() const { return !m_query.isEmpty(); }
    QJsonObject message(int row) const;
    int rowOfMessage(qint64 id) const;
    // Like rowOfMessage, but reads further pages until the message is found.
    int ensureRow(qint64 id);

    int rowCount(const QModelIndex &parent = {}) const override;
    int columnCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QVariant headerData(int section, Qt::Orientation orientation, int role) const override;

private:
    static constexpr int PageSize = 200;

    bool listsMessages() const { return m_folderId != 0 || !m_role.isEmpty(); }
    QJsonObject listParams(int offset) const;
    void load();
    const QJsonArray &page(int pageIndex) const;

    Core *m_core;
    qint64 m_folderId = 0;
    QString m_role;
    QString m_query;
    bool m_threaded = false;
    int m_total = 0;
    mutable QHash<int, QJsonArray> m_pages;
};
