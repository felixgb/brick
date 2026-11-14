#include "CalendarView.h"

#include <QFontMetrics>
#include <QFrame>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QLocale>
#include <QMouseEvent>
#include <QPainter>
#include <QPushButton>
#include <QToolButton>
#include <QVBoxLayout>
#include <QWheelEvent>

#include "ItemList.h"
#include "ItemStore.h"

namespace {

int firstDayOfWeek() {
    return int(QLocale().firstDayOfWeek()); // 1 = Monday ... 7 = Sunday
}

QDate firstOf(const QDate &d) {
    return QDate(d.year(), d.month(), 1);
}

} // namespace

// ---- MonthGrid -------------------------------------------------------------

MonthGrid::MonthGrid(ItemStore *store, QWidget *parent)
    : QWidget(parent), m_store(store), m_month(firstOf(QDate::currentDate())), m_selected(QDate::currentDate()) {
    setObjectName("monthGrid");
    setFocusPolicy(Qt::StrongFocus);
    setMinimumSize(420, 320);
    setAutoFillBackground(true);
    rebuildIndex();
    connect(m_store, &ItemStore::changed, this, [this] {
        rebuildIndex();
        update();
    });
}

QDate MonthGrid::gridStart() const {
    const int lead = (m_month.dayOfWeek() - firstDayOfWeek() + 7) % 7;
    return m_month.addDays(-lead);
}

void MonthGrid::rebuildIndex() {
    // Group once per change rather than filtering the list for all 42 cells
    // on every repaint.
    m_byDay.clear();
    const QDate first = gridStart();
    const QDate last = first.addDays(41);
    for (const Item &item : m_store->items()) {
        const QDate from = Day::date(item.date);
        if (!from.isValid())
            continue;
        QDate to = Day::date(item.endDate);
        if (!to.isValid() || to < from)
            to = from;
        for (QDate d = qMax(from, first); d <= qMin(to, last); d = d.addDays(1))
            m_byDay[Day::string(d)].append(item);
    }
}

void MonthGrid::setMonth(const QDate &anyDayInMonth) {
    const QDate first = firstOf(anyDayInMonth);
    if (first == m_month)
        return;
    m_month = first;
    rebuildIndex();
    update();
    emit monthChanged(m_month);
}

QRect MonthGrid::cellRect(const QDate &day) const {
    const int index = int(gridStart().daysTo(day));
    if (index < 0 || index >= 42)
        return {};
    const int col = index % 7;
    const int row = index / 7;
    const int gridH = height() - HeaderHeight;
    const int x0 = col * width() / 7, x1 = (col + 1) * width() / 7;
    const int y0 = HeaderHeight + row * gridH / 6, y1 = HeaderHeight + (row + 1) * gridH / 6;
    return QRect(x0, y0, x1 - x0, y1 - y0);
}

QDate MonthGrid::dayAt(const QPoint &pos) const {
    if (pos.y() < HeaderHeight || pos.x() < 0 || pos.x() >= width() || pos.y() >= height())
        return {};
    const int col = pos.x() * 7 / width();
    const int row = (pos.y() - HeaderHeight) * 6 / (height() - HeaderHeight);
    return gridStart().addDays(row * 7 + col);
}

void MonthGrid::select(const QDate &day) {
    if (!day.isValid() || day == m_selected)
        return;
    const QDate old = m_selected;
    m_selected = day;
    if (cellRect(day).isNull())
        setMonth(day);
    else {
        update(cellRect(old));
        update(cellRect(day));
    }
    emit selectionChanged(day);
}

void MonthGrid::paintEvent(QPaintEvent *event) {
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing);
    const QPalette &pal = palette();
    const QColor accent = pal.color(QPalette::Highlight);
    const QColor line = pal.color(QPalette::Mid).lighter(135);
    const QFontMetrics fm(font());
    const QDate today = QDate::currentDate();
    const QDate start = gridStart();

    // Weekday header.
    if (event->rect().top() < HeaderHeight) {
        QFont hf = font();
        hf.setPointSizeF(hf.pointSizeF() * 0.9);
        hf.setBold(true);
        p.setFont(hf);
        p.setPen(pal.color(QPalette::PlaceholderText));
        for (int c = 0; c < 7; ++c) {
            const int dow = (firstDayOfWeek() - 1 + c) % 7 + 1;
            const QRect r(c * width() / 7, 0, width() / 7, HeaderHeight);
            p.drawText(r, Qt::AlignCenter, QLocale().dayName(dow, QLocale::ShortFormat));
        }
        p.setPen(line);
        p.drawLine(0, HeaderHeight - 1, width(), HeaderHeight - 1);
    }

    QFont small = font();
    small.setPointSizeF(small.pointSizeF() * 0.85);
    const QFontMetrics sfm(small);
    const int lineH = sfm.height() + 3;

    for (int i = 0; i < 42; ++i) {
        const QDate day = start.addDays(i);
        const QRect cell = cellRect(day);
        if (!cell.intersects(event->rect()))
            continue;
        const bool inMonth = day.month() == m_month.month();
        const bool selected = day == m_selected;

        p.fillRect(cell, inMonth ? pal.color(QPalette::Base) : pal.color(QPalette::AlternateBase));
        if (selected) {
            QColor tint = accent;
            tint.setAlpha(36);
            p.fillRect(cell, tint);
        }
        p.setPen(line);
        p.drawLine(cell.bottomLeft(), cell.bottomRight());
        p.drawLine(cell.topRight(), cell.bottomRight());
        if (selected) {
            p.setPen(QPen(accent, 2));
            p.drawRect(cell.adjusted(1, 1, -2, -2));
        }

        // Day number, top right, with a filled circle for today.
        const QRect num(cell.right() - 30, cell.top() + 4, 24, 22);
        p.setFont(font());
        if (day == today) {
            p.setPen(Qt::NoPen);
            p.setBrush(accent);
            p.drawEllipse(num);
            p.setPen(pal.color(QPalette::HighlightedText));
            QFont bf = font();
            bf.setBold(true);
            p.setFont(bf);
        } else {
            p.setPen(pal.color(inMonth ? QPalette::Text : QPalette::PlaceholderText));
        }
        p.drawText(num, Qt::AlignCenter, QString::number(day.day()));
        p.setBrush(Qt::NoBrush);

        // The day's items.
        const QList<Item> items = m_byDay.value(Day::string(day));
        if (items.isEmpty())
            continue;
        p.setFont(small);
        const int room = cell.height() - 30 - 3;
        const int lines = qMax(1, room / lineH);
        const int shown = items.size() <= lines ? items.size() : qMax(0, lines - 1);
        int y = cell.top() + 30;
        for (int k = 0; k < shown; ++k, y += lineH) {
            const Item &item = items.at(k);
            const QRect bar(cell.left() + 5, y + 2, 3, lineH - 5);
            p.setPen(Qt::NoPen);
            p.setBrush(item.done ? pal.color(QPalette::Mid) : accent);
            p.drawRoundedRect(bar, 1.5, 1.5);
            p.setBrush(Qt::NoBrush);
            QFont f = small;
            f.setStrikeOut(item.done);
            p.setFont(f);
            p.setPen(pal.color(item.done ? QPalette::PlaceholderText : QPalette::Text));
            const QString text = item.startTime.isEmpty() ? item.title : item.startTime + QLatin1Char(' ') + item.title;
            const QRect tr(cell.left() + 12, y, cell.width() - 16, lineH);
            p.drawText(tr, Qt::AlignLeft | Qt::AlignVCenter, sfm.elidedText(text, Qt::ElideRight, tr.width()));
        }
        if (shown < items.size()) {
            p.setFont(small);
            p.setPen(pal.color(QPalette::PlaceholderText));
            const int more = items.size() - shown;
            p.drawText(QRect(cell.left() + 6, y, cell.width() - 10, lineH), Qt::AlignLeft | Qt::AlignVCenter,
                       shown == 0 ? tr("%n item(s)", nullptr, more) : tr("%1 more").arg(more));
        }
    }
}

void MonthGrid::mousePressEvent(QMouseEvent *event) {
    setFocus();
    if (event->button() == Qt::LeftButton)
        select(dayAt(event->pos()));
}

void MonthGrid::mouseDoubleClickEvent(QMouseEvent *event) {
    const QDate day = dayAt(event->pos());
    if (event->button() == Qt::LeftButton && day.isValid()) {
        select(day);
        emit newItemRequested(day);
    }
}

void MonthGrid::keyPressEvent(QKeyEvent *event) {
    QDate target;
    switch (event->key()) {
    case Qt::Key_Left: target = m_selected.addDays(-1); break;
    case Qt::Key_Right: target = m_selected.addDays(1); break;
    case Qt::Key_Up: target = m_selected.addDays(-7); break;
    case Qt::Key_Down: target = m_selected.addDays(7); break;
    case Qt::Key_PageUp: target = m_selected.addMonths(-1); break;
    case Qt::Key_PageDown: target = m_selected.addMonths(1); break;
    case Qt::Key_Home: target = QDate::currentDate(); break;
    case Qt::Key_Return:
    case Qt::Key_Enter:
        emit newItemRequested(m_selected);
        return;
    default:
        QWidget::keyPressEvent(event);
        return;
    }
    if (target.month() != m_month.month() || target.year() != m_month.year())
        setMonth(target);
    select(target);
}

void MonthGrid::wheelEvent(QWheelEvent *event) {
    static int accumulated = 0;
    accumulated += event->angleDelta().y();
    while (accumulated >= 120) {
        accumulated -= 120;
        setMonth(m_month.addMonths(-1));
    }
    while (accumulated <= -120) {
        accumulated += 120;
        setMonth(m_month.addMonths(1));
    }
}

// ---- CalendarView ----------------------------------------------------------

CalendarView::CalendarView(ItemStore *store, QWidget *parent) : QWidget(parent), m_store(store) {
    setObjectName("calendarView");
    m_grid = new MonthGrid(store);

    auto *prev = new QToolButton;
    prev->setObjectName("calPrev");
    prev->setArrowType(Qt::LeftArrow);
    prev->setToolTip(tr("Previous month"));
    auto *next = new QToolButton;
    next->setObjectName("calNext");
    next->setArrowType(Qt::RightArrow);
    next->setToolTip(tr("Next month"));
    auto *today = new QPushButton(tr("Today"));
    today->setObjectName("calToday");
    today->setToolTip(tr("Go to today"));

    m_title = new QLabel;
    QFont tf = m_title->font();
    tf.setBold(true);
    tf.setPointSizeF(tf.pointSizeF() * 1.4);
    m_title->setFont(tf);

    auto *add = new QPushButton(tr("New item…"));
    add->setObjectName("calNew");
    add->setToolTip(tr("Add an item on the selected day"));

    auto *bar = new QHBoxLayout;
    bar->setContentsMargins(12, 8, 12, 8);
    bar->addWidget(m_title);
    bar->addStretch(1);
    bar->addWidget(prev);
    bar->addWidget(today);
    bar->addWidget(next);
    bar->addSpacing(10);
    bar->addWidget(add);

    auto *left = new QVBoxLayout;
    left->setContentsMargins(0, 0, 0, 0);
    left->setSpacing(0);
    left->addLayout(bar);
    left->addWidget(m_grid, 1);

    // Day panel.
    m_dayTitle = new QLabel;
    QFont df = m_dayTitle->font();
    df.setBold(true);
    df.setPointSizeF(df.pointSizeF() * 1.15);
    m_dayTitle->setFont(df);
    m_dayTitle->setWordWrap(true);
    auto *panelAdd = new QToolButton;
    panelAdd->setText(QStringLiteral("+"));
    panelAdd->setToolTip(tr("Add an item on this day"));
    auto *panelHead = new QHBoxLayout;
    panelHead->setContentsMargins(12, 12, 12, 10);
    panelHead->addWidget(m_dayTitle, 1);
    panelHead->addWidget(panelAdd);

    m_dayList = new ItemList;
    m_dayList->setObjectName("dayList");
    m_empty = new QLabel(tr("Nothing on this day"));
    m_empty->setAlignment(Qt::AlignCenter);
    m_empty->setEnabled(false);

    auto *panel = new QWidget;
    panel->setObjectName("dayPanel");
    panel->setFixedWidth(300);
    auto *pl = new QVBoxLayout(panel);
    pl->setContentsMargins(0, 0, 0, 0);
    pl->setSpacing(0);
    pl->addLayout(panelHead);
    auto *rule = new QFrame;
    rule->setFrameShape(QFrame::HLine);
    rule->setFrameShadow(QFrame::Sunken);
    pl->addWidget(rule);
    pl->addWidget(m_dayList, 1);
    pl->addWidget(m_empty, 1);

    auto *divider = new QFrame;
    divider->setFrameShape(QFrame::VLine);
    divider->setFrameShadow(QFrame::Sunken);

    auto *outer = new QHBoxLayout(this);
    outer->setContentsMargins(0, 0, 0, 0);
    outer->setSpacing(0);
    outer->addLayout(left, 1);
    outer->addWidget(divider);
    outer->addWidget(panel);

    // Keep a day selected inside the month now on screen.
    auto shift = [this](int months) {
        const QDate moved = m_grid->month().addMonths(months);
        m_grid->setMonth(moved);
        const bool current = moved.year() == QDate::currentDate().year() && moved.month() == QDate::currentDate().month();
        m_grid->select(current ? QDate::currentDate() : moved);
    };
    connect(prev, &QToolButton::clicked, this, [shift] { shift(-1); });
    connect(next, &QToolButton::clicked, this, [shift] { shift(1); });
    connect(today, &QPushButton::clicked, this, &CalendarView::goToday);
    connect(add, &QPushButton::clicked, this, [this] { emit newItemRequested(selectedDay()); });
    connect(panelAdd, &QToolButton::clicked, this, [this] { emit newItemRequested(selectedDay()); });
    connect(m_grid, &MonthGrid::newItemRequested, this, [this](const QDate &d) { emit newItemRequested(Day::string(d)); });
    connect(m_grid, &MonthGrid::selectionChanged, this, &CalendarView::refreshPanel);
    connect(m_grid, &MonthGrid::monthChanged, this, &CalendarView::refreshTitle);
    connect(m_store, &ItemStore::changed, this, &CalendarView::refreshPanel);

    connect(m_dayList, &ItemList::editRequested, this, &CalendarView::editRequested);
    connect(m_dayList, &ItemList::openEmailRequested, this, &CalendarView::openEmailRequested);
    connect(m_dayList, &ItemList::doneToggled, m_store, &ItemStore::setDone);
    connect(m_dayList, &ItemList::deleteRequested, m_store, &ItemStore::remove);

    refreshTitle();
    refreshPanel();
}

void CalendarView::goToday() {
    m_grid->setMonth(QDate::currentDate());
    m_grid->select(QDate::currentDate());
    // select() is a no-op when today is already selected; the panel is current.
}

void CalendarView::refreshTitle() {
    m_title->setText(QLocale().toString(m_grid->month(), QStringLiteral("MMMM yyyy")));
}

void CalendarView::refreshPanel() {
    const QList<Item> items = m_store->on(selectedDay());
    m_dayTitle->setText(QLocale().toString(m_grid->selected(), QStringLiteral("dddd d MMMM")));
    m_dayList->setSections({{QString(), items}}, false);
    m_empty->setVisible(items.isEmpty());
    m_dayList->setVisible(!items.isEmpty());
}
