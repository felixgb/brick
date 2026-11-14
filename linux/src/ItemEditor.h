#pragma once

#include <QDialog>

#include "Item.h"

class ItemStore;
class QCheckBox;
class QComboBox;
class QDateEdit;
class QLabel;
class QLineEdit;
class QPlainTextEdit;
class QTimeEdit;

// The title, day, time, location and notes controls. Shared by the editor and
// the Claude review dialog.
class ItemFields : public QWidget {
    Q_OBJECT
public:
    explicit ItemFields(const Item &item, QWidget *parent = nullptr);

    // The edited item: everything not shown here (id, source, done) is kept.
    Item item() const;
    void focusTitle();

signals:
    void titleChanged(const QString &title);

private:
    void syncEnabled();

    Item m_base;
    QLineEdit *m_title;
    QCheckBox *m_hasDay;
    QDateEdit *m_day;
    QCheckBox *m_hasEndDay;
    QDateEdit *m_endDay;
    QComboBox *m_repeat;
    QCheckBox *m_hasUntil;
    QDateEdit *m_until;
    QCheckBox *m_hasStart;
    QTimeEdit *m_start;
    QCheckBox *m_hasEnd;
    QTimeEdit *m_end;
    QLineEdit *m_location;
    QPlainTextEdit *m_notes;
};

// Dialog for creating or editing one item. Saves through the store itself.
class ItemEditor : public QDialog {
    Q_OBJECT
public:
    ItemEditor(ItemStore *store, const Item &item, QWidget *parent = nullptr);

signals:
    void openEmailRequested(const Item &item);

private:
    void save();

    ItemStore *m_store;
    ItemFields *m_fields;
    QCheckBox *m_done;
    QLabel *m_problem;
    Item m_item;
};
