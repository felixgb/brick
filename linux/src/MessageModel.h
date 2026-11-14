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
    void showSearch(const QString &query);
    // Re-reads the current folder or search. The view's selection and scroll
    // position are reset; MainWindow restores them by message id.
    void refresh();

    qint64 folderId() const { return m_folderId; }
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

    void load();
    const QJsonArray &page(int pageIndex) const;

    Core *m_core;
    qint64 m_folderId = 0;
    QString m_query;
    int m_total = 0;
    mutable QHash<int, QJsonArray> m_pages;
};
