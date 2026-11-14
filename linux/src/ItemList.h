#pragma once

#include <QListView>

#include "Item.h"

class ItemListModel;

// A list of items with a tick circle, title and "when · where" line, and
// optional section headers. Used by the To-do tab and the calendar's day panel.
class ItemList : public QListView {
    Q_OBJECT
public:
    struct Section {
        QString title; // empty: no header
        QList<Item> items;
    };

    explicit ItemList(QWidget *parent = nullptr);

    // showsDay: false hides the day when the surrounding list already says which day it is.
    void setSections(const QList<Section> &sections, bool showsDay = true);
    int itemCount() const;

signals:
    void editRequested(const Item &item);
    void doneToggled(const Item &item, bool done);
    void deleteRequested(const Item &item);
    void openEmailRequested(const Item &item);

protected:
    void mousePressEvent(QMouseEvent *event) override;
    void mouseDoubleClickEvent(QMouseEvent *event) override;
    void contextMenuEvent(QContextMenuEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;

private:
    bool itemAt(const QPoint &pos, Item *out) const;

    ItemListModel *m_model;
};
