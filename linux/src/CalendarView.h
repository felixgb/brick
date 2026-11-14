#pragma once

#include <QDate>
#include <QAbstractScrollArea>
#include <QHash>
#include <QWidget>

#include "Item.h"

class ItemList;
class ItemStore;
class QLabel;
class QStackedWidget;
class QTabBar;

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

// The week and day views: an hour grid with an all-day strip above it. Timed
// items are blocks (overlapping ones share the width); items with no time sit
// in the strip. Paints itself like MonthGrid does.
class TimeGrid : public QAbstractScrollArea {
    Q_OBJECT
public:
    explicit TimeGrid(ItemStore *store, QWidget *parent = nullptr);

    // Shows `days` consecutive days starting at `first`.
    void setRange(const QDate &first, int days);
    QDate first() const { return m_first; }
    int days() const { return m_days; }
    QDate selected() const { return m_selected; }
    void select(const QDate &day);
    // Scrolls so the working day (or the earliest item) is in view.
    void scrollToWork();

signals:
    void selectionChanged(const QDate &day);
    void newItemRequested(const QDate &day, const QString &time);
    void editRequested(const Item &item);

protected:
    void paintEvent(QPaintEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseDoubleClickEvent(QMouseEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;

private:
    struct Segment {
        Item item;
        int from = 0; // minutes from midnight
        int to = 0;
        int lane = 0;
        int lanes = 1;
    };
    struct DayData {
        QList<Item> allDay;
        QList<Segment> timed;
    };
    struct Hit {
        QRect rect;
        Item item;
    };

    static constexpr int HourHeight = 48;
    static constexpr int Gutter = 54;
    static constexpr int DayRow = 36;

    void rebuild();
    void updateScrollBars();
    int headerHeight() const;
    int columnLeft(int i) const;
    int dayIndexAt(int x) const;

    ItemStore *m_store;
    QDate m_first;
    int m_days = 7;
    QDate m_selected;
    QList<DayData> m_data;
    mutable QList<Hit> m_hits;
};

// The Calendar tab: a month grid of the same items the To-do tab lists, with
// the selected day's items alongside.
class CalendarView : public QWidget {
    Q_OBJECT
public:
    explicit CalendarView(ItemStore *store, QWidget *parent = nullptr);

    enum Mode { Month, Week, Day };

    MonthGrid *grid() const { return m_grid; }
    TimeGrid *timeGrid() const { return m_time; }
    ItemList *dayList() const { return m_dayList; }
    QString selectedDay() const { return Day::string(m_selected); }
    Mode mode() const { return Mode(m_modes_index()); }
    void setMode(Mode mode);
    void goToday();

signals:
    void newItemRequested(const QString &day, const QString &time = QString());
    void editRequested(const Item &item);
    void openEmailRequested(const Item &item);

private:
    int m_modes_index() const;
    void setSelected(const QDate &day);
    void shift(int steps);
    void syncTimeGrid();
    void refreshPanel();
    void refreshTitle();

    ItemStore *m_store;
    QDate m_selected;
    QTabBar *m_modes;
    QStackedWidget *m_views;
    MonthGrid *m_grid;
    TimeGrid *m_time;
    QLabel *m_title;
    QLabel *m_dayTitle;
    QLabel *m_empty;
    ItemList *m_dayList;
};
