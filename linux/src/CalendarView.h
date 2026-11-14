#pragma once

#include <QDate>
#include <QHash>
#include <QWidget>

#include "Item.h"

class ItemList;
class ItemStore;
class QLabel;

// The month grid. It paints itself and selects on mouse press, so a click
// redraws two cells and nothing else.
class MonthGrid : public QWidget {
    Q_OBJECT
public:
    explicit MonthGrid(ItemStore *store, QWidget *parent = nullptr);

    QDate month() const { return m_month; }
    QDate selected() const { return m_selected; }
    void setMonth(const QDate &anyDayInMonth);
    void select(const QDate &day);
    QRect cellRect(const QDate &day) const;
    QDate dayAt(const QPoint &pos) const;

    QSize sizeHint() const override { return {700, 520}; }

signals:
    void monthChanged(const QDate &firstOfMonth);
    void selectionChanged(const QDate &day);
    void newItemRequested(const QDate &day);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseDoubleClickEvent(QMouseEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;

private:
    static constexpr int HeaderHeight = 26;

    QDate gridStart() const;
    void rebuildIndex();

    ItemStore *m_store;
    QDate m_month; // first of the month on screen
    QDate m_selected;
    QHash<QString, QList<Item>> m_byDay;
};

// The Calendar tab: a month grid of the same items the To-do tab lists, with
// the selected day's items alongside.
class CalendarView : public QWidget {
    Q_OBJECT
public:
    explicit CalendarView(ItemStore *store, QWidget *parent = nullptr);

    MonthGrid *grid() const { return m_grid; }
    ItemList *dayList() const { return m_dayList; }
    QString selectedDay() const { return Day::string(m_grid->selected()); }
    void goToday();

signals:
    void newItemRequested(const QString &day);
    void editRequested(const Item &item);
    void openEmailRequested(const Item &item);

private:
    void refreshPanel();
    void refreshTitle();

    ItemStore *m_store;
    MonthGrid *m_grid;
    QLabel *m_title;
    QLabel *m_dayTitle;
    QLabel *m_empty;
    ItemList *m_dayList;
};
