#pragma once

#include <QWidget>

#include "Item.h"

class ItemList;
class ItemStore;
class QCheckBox;
class QLabel;
class QLineEdit;

// The To-do tab: every item, grouped by how soon it needs attention.
class TodoView : public QWidget {
    Q_OBJECT
public:
    explicit TodoView(ItemStore *store, QWidget *parent = nullptr);

    ItemList *list() const { return m_list; }

signals:
    void newItemRequested();
    void editRequested(const Item &item);
    void openEmailRequested(const Item &item);

private:
    void rebuild();
    void addQuick();

    ItemStore *m_store;
    QLineEdit *m_quick;
    QCheckBox *m_showDone;
    QLabel *m_count;
    QLabel *m_empty;
    ItemList *m_list;
};
