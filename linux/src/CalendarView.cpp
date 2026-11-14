#include "CalendarView.h"

#include <QFontMetrics>
#include <QFrame>
#include <QHBoxLayout>
#include <QKeyEvent>
#include <QLabel>
#include <QScrollBar>
#include <QStackedWidget>
#include <QTabBar>
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
        if (!item.repeat.isEmpty()) {
            // A repeating item can land anywhere in the grid.
            for (QDate d = first; d <= last; d = d.addDays(1)) {
                if (item.occursOn(Day::string(d)))
                    m_byDay[Day::string(d)].append(item);
            }
            continue;
        }
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

// ---- TimeGrid --------------------------------------------------------------

TimeGrid::TimeGrid(ItemStore *store, QWidget *parent)
    : QAbstractScrollArea(parent), m_store(store), m_first(QDate::currentDate()), m_selected(QDate::currentDate()) {
    setObjectName("timeGrid");
    setFrameShape(QFrame::NoFrame);
    setFocusPolicy(Qt::StrongFocus);
    setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    viewport()->setAutoFillBackground(true);
    verticalScrollBar()->setSingleStep(24);
    connect(m_store, &ItemStore::changed, this, [this] {
        rebuild();
        viewport()->update();
    });
    rebuild();
}

void TimeGrid::setRange(const QDate &first, int days) {
    if (first == m_first && days == m_days)
        return;
    m_first = first;
    m_days = days;
    rebuild();
    viewport()->update();
}

void TimeGrid::select(const QDate &day) {
    if (!day.isValid() || day == m_selected)
        return;
    m_selected = day;
    viewport()->update();
    emit selectionChanged(day);
}

void TimeGrid::rebuild() {
    m_data.clear();
    for (int i = 0; i < m_days; ++i) {
        const QDate date = m_first.addDays(i);
        const QString key = Day::string(date);
        DayData d;
        for (const Item &item : m_store->on(key)) {
            if (item.startTime.isEmpty()) {
                d.allDay.append(item);
                continue;
            }
            // A timed item that spans days runs from its start to midnight,
            // fills the days between, and ends at its end time.
            const QTime start = QTime::fromString(item.startTime, QStringLiteral("HH:mm"));
            const QTime end = QTime::fromString(item.endTime, QStringLiteral("HH:mm"));
            const QDate lastDay = Day::date(item.endDate).isValid() ? Day::date(item.endDate) : Day::date(item.date);
            // Which day of its span is this? (0 for a single-day item.)
            const int span = int(Day::date(item.date).daysTo(lastDay));
            int startOffset = 0;
            for (int k = 0; k <= span; ++k) {
                if (item.startsOn(date.addDays(-k))) {
                    startOffset = k;
                    break;
                }
            }
            Segment seg;
            seg.item = item;
            seg.from = startOffset == 0 ? start.hour() * 60 + start.minute() : 0;
            const bool endsToday = startOffset == span;
            seg.to = endsToday ? (end.isValid() ? end.hour() * 60 + end.minute() : seg.from + 60) : 24 * 60;
            if (seg.to <= seg.from)
                seg.to = qMin(24 * 60, seg.from + 60);
            d.timed.append(seg);
        }
        // Lanes: overlapping blocks sit side by side, each in the first free column.
        std::sort(d.timed.begin(), d.timed.end(), [](const Segment &a, const Segment &b) { return a.from < b.from; });
        QList<int> laneEnd;
        int clusterStart = 0;
        for (int k = 0; k < d.timed.size(); ++k) {
            Segment &seg = d.timed[k];
            int lane = 0;
            while (lane < laneEnd.size() && laneEnd.at(lane) > seg.from)
                ++lane;
            if (lane == laneEnd.size())
                laneEnd.append(0);
            laneEnd[lane] = seg.to;
            seg.lane = lane;
            // A gap with no overlap ends the cluster: its blocks share one lane count.
            const bool last = k + 1 == d.timed.size();
            if (last || d.timed.at(k + 1).from >= *std::max_element(laneEnd.begin(), laneEnd.end())) {
                for (int j = clusterStart; j <= k; ++j)
                    d.timed[j].lanes = laneEnd.size();
                laneEnd.clear();
                clusterStart = k + 1;
            }
        }
        m_data.append(d);
    }
    updateScrollBars();
}

int TimeGrid::headerHeight() const {
    int rows = 0;
    for (const DayData &d : m_data)
        rows = qMax(rows, qMin(3, int(d.allDay.size())));
    return DayRow + (rows == 0 ? 8 : rows * 19 + 6);
}

void TimeGrid::updateScrollBars() {
    const int visible = viewport()->height() - headerHeight();
    verticalScrollBar()->setRange(0, qMax(0, 24 * HourHeight - visible));
    verticalScrollBar()->setPageStep(qMax(0, visible));
}

void TimeGrid::resizeEvent(QResizeEvent *event) {
    QAbstractScrollArea::resizeEvent(event);
    updateScrollBars();
}

void TimeGrid::scrollToWork() {
    int earliest = 8 * 60;
    for (const DayData &d : m_data) {
        for (const Segment &s : d.timed)
            earliest = qMin(earliest, s.from);
    }
    verticalScrollBar()->setValue(qMax(0, (earliest - 60) * HourHeight / 60));
}

int TimeGrid::columnLeft(int i) const {
    return Gutter + i * (viewport()->width() - Gutter) / m_days;
}

int TimeGrid::dayIndexAt(int x) const {
    const int width = viewport()->width() - Gutter;
    if (x < Gutter || x >= viewport()->width() || width <= 0)
        return -1;
    return (x - Gutter) * m_days / width;
}

void TimeGrid::paintEvent(QPaintEvent *) {
    QPainter p(viewport());
    p.setRenderHint(QPainter::Antialiasing);
    const QPalette &pal = palette();
    const QColor accent = pal.color(QPalette::Highlight);
    const QColor line = pal.color(QPalette::Mid).lighter(135);
    const int vw = viewport()->width();
    const int vh = viewport()->height();
    const int header = headerHeight();
    const int scroll = verticalScrollBar()->value();
    const QDate today = QDate::currentDate();
    const QFontMetrics fm(font());
    m_hits.clear();

    QFont small = font();
    small.setPointSizeF(small.pointSizeF() * 0.85);
    const QFontMetrics sfm(small);

    // ---- hour area, scrolled under the header ----
    p.save();
    p.setClipRect(QRect(0, header, vw, vh - header));
    p.translate(0, header - scroll);
    p.fillRect(QRect(0, 0, vw, 24 * HourHeight), pal.color(QPalette::Base));
    for (int i = 0; i < m_days; ++i) {
        const QDate date = m_first.addDays(i);
        const QRect col(columnLeft(i), 0, columnLeft(i + 1) - columnLeft(i), 24 * HourHeight);
        if (date == m_selected) {
            QColor tint = accent;
            tint.setAlpha(22);
            p.fillRect(col, tint);
        }
        p.setPen(line);
        p.drawLine(col.left(), 0, col.left(), 24 * HourHeight);
    }
    p.setFont(small);
    for (int h = 0; h < 24; ++h) {
        const int y = h * HourHeight;
        p.setPen(line);
        p.drawLine(Gutter - 4, y, vw, y);
        // Skip a label that would slide under the header.
        if (h > 0 && y - scroll >= 8) {
            p.setPen(pal.color(QPalette::PlaceholderText));
            p.drawText(QRect(0, y - 8, Gutter - 8, 16), Qt::AlignRight | Qt::AlignVCenter,
                       QStringLiteral("%1:00").arg(h, 2, 10, QLatin1Char('0')));
        }
    }

    for (int i = 0; i < m_data.size(); ++i) {
        const int left = columnLeft(i) + 2;
        const int width = columnLeft(i + 1) - columnLeft(i) - 5;
        for (const Segment &seg : m_data.at(i).timed) {
            const int laneW = width / seg.lanes;
            const QRect r(left + seg.lane * laneW, seg.from * HourHeight / 60 + 1, laneW - 2,
                          qMax(16, (seg.to - seg.from) * HourHeight / 60 - 2));
            QColor fill = seg.item.done ? pal.color(QPalette::Mid) : accent;
            fill.setAlpha(seg.item.done ? 60 : 55);
            p.setPen(Qt::NoPen);
            p.setBrush(fill);
            p.drawRoundedRect(r, 3, 3);
            p.setBrush(seg.item.done ? pal.color(QPalette::Mid) : accent);
            p.drawRoundedRect(QRect(r.left(), r.top(), 3, r.height()), 1.5, 1.5);

            QFont f = small;
            f.setStrikeOut(seg.item.done);
            p.setFont(f);
            p.setPen(pal.color(seg.item.done ? QPalette::PlaceholderText : QPalette::Text));
            // Title on one line (elided); the place on a second line when there is room.
            const QRect text = r.adjusted(7, 2, -3, -1);
            p.drawText(QRect(text.left(), text.top(), text.width(), sfm.height()), Qt::AlignLeft | Qt::AlignVCenter,
                       sfm.elidedText(seg.item.startTime + QLatin1Char(' ') + seg.item.title, Qt::ElideRight, text.width()));
            if (!seg.item.location.isEmpty() && r.height() >= 2 * sfm.height() + 6) {
                p.setPen(pal.color(QPalette::PlaceholderText));
                p.drawText(QRect(text.left(), text.top() + sfm.height(), text.width(), sfm.height()),
                           Qt::AlignLeft | Qt::AlignVCenter, sfm.elidedText(seg.item.location, Qt::ElideRight, text.width()));
            }
            // Hit rect in viewport coordinates.
            m_hits.append({r.translated(0, header - scroll), seg.item});
        }
        // "Now" line.
        if (m_first.addDays(i) == today) {
            const QTime now = QTime::currentTime();
            const int y = (now.hour() * 60 + now.minute()) * HourHeight / 60;
            p.setPen(QPen(QColor(0xD0, 0x3B, 0x3B), 1.6));
            p.drawLine(columnLeft(i), y, columnLeft(i + 1), y);
            p.setBrush(QColor(0xD0, 0x3B, 0x3B));
            p.drawEllipse(QPoint(columnLeft(i), y), 3, 3);
        }
    }
    p.restore();

    // ---- header: day names and the all-day strip ----
    p.fillRect(QRect(0, 0, vw, header), pal.color(QPalette::Window));
    for (int i = 0; i < m_days; ++i) {
        const QDate date = m_first.addDays(i);
        const QRect col(columnLeft(i), 0, columnLeft(i + 1) - columnLeft(i), header);
        const bool isToday = date == today;
        if (date == m_selected) {
            QColor tint = accent;
            tint.setAlpha(28);
            p.fillRect(col, tint);
        }
        p.setPen(line);
        p.drawLine(col.left(), 0, col.left(), header);

        QFont df = font();
        df.setBold(true);
        p.setFont(small);
        p.setPen(pal.color(QPalette::PlaceholderText));
        p.drawText(QRect(col.left(), 2, col.width(), 14), Qt::AlignCenter,
                   QLocale().dayName(date.dayOfWeek(), QLocale::ShortFormat).toUpper());
        const QRect num(col.center().x() - 12, 15, 24, 20);
        p.setFont(df);
        if (isToday) {
            p.setPen(Qt::NoPen);
            p.setBrush(accent);
            p.drawEllipse(num);
            p.setPen(pal.color(QPalette::HighlightedText));
        } else {
            p.setPen(pal.color(QPalette::Text));
        }
        p.drawText(num, Qt::AlignCenter, QString::number(date.day()));
        p.setBrush(Qt::NoBrush);

        // All-day chips.
        const QList<Item> &all = m_data.at(i).allDay;
        const int shown = all.size() > 3 ? 2 : int(all.size());
        int y = DayRow + 2;
        p.setFont(small);
        for (int k = 0; k < shown; ++k, y += 19) {
            const Item &item = all.at(k);
            const QRect chip(col.left() + 3, y, col.width() - 6, 17);
            QColor fill = item.done ? pal.color(QPalette::Mid) : accent;
            fill.setAlpha(item.done ? 70 : 60);
            p.setPen(Qt::NoPen);
            p.setBrush(fill);
            p.drawRoundedRect(chip, 3, 3);
            QFont f = small;
            f.setStrikeOut(item.done);
            p.setFont(f);
            p.setPen(pal.color(item.done ? QPalette::PlaceholderText : QPalette::Text));
            p.drawText(chip.adjusted(5, 0, -3, 0), Qt::AlignLeft | Qt::AlignVCenter,
                       sfm.elidedText(item.title, Qt::ElideRight, chip.width() - 8));
            m_hits.append({chip, item});
        }
        if (shown < all.size()) {
            p.setFont(small);
            p.setPen(pal.color(QPalette::PlaceholderText));
            p.drawText(QRect(col.left() + 6, y, col.width() - 8, 17), Qt::AlignLeft | Qt::AlignVCenter,
                       tr("%1 more").arg(all.size() - shown));
        }
    }
    p.setPen(line);
    p.drawLine(0, header - 1, vw, header - 1);
    p.setFont(small);
    p.setPen(pal.color(QPalette::PlaceholderText));
    p.drawText(QRect(0, DayRow, Gutter - 8, header - DayRow), Qt::AlignRight | Qt::AlignTop, tr("all-day"));
}

void TimeGrid::mousePressEvent(QMouseEvent *event) {
    setFocus();
    if (event->button() != Qt::LeftButton)
        return;
    const int col = dayIndexAt(event->pos().x());
    if (col >= 0 && col < m_days)
        select(m_first.addDays(col));
}

void TimeGrid::mouseDoubleClickEvent(QMouseEvent *event) {
    if (event->button() != Qt::LeftButton)
        return;
    const int col = dayIndexAt(event->pos().x());
    if (col < 0 || col >= m_days)
        return;
    select(m_first.addDays(col));
    for (const Hit &hit : std::as_const(m_hits)) {
        // Blocks scrolled under the header are not clickable.
        if (hit.rect.contains(event->pos()) && (event->pos().y() >= headerHeight() || hit.rect.bottom() < headerHeight())) {
            emit editRequested(hit.item);
            return;
        }
    }
    if (event->pos().y() < headerHeight()) {
        emit newItemRequested(m_first.addDays(col), QString());
        return;
    }
    // Empty space in the hour grid: a new item at that time, to the half hour.
    const int minutes = (event->pos().y() - headerHeight() + verticalScrollBar()->value()) * 60 / HourHeight;
    const int snapped = qBound(0, minutes / 30 * 30, 23 * 60 + 30);
    emit newItemRequested(m_first.addDays(col),
                          QStringLiteral("%1:%2").arg(snapped / 60, 2, 10, QLatin1Char('0')).arg(snapped % 60, 2, 10, QLatin1Char('0')));
}

void TimeGrid::keyPressEvent(QKeyEvent *event) {
    switch (event->key()) {
    case Qt::Key_Left: select(m_selected.addDays(-1)); break;
    case Qt::Key_Right: select(m_selected.addDays(1)); break;
    case Qt::Key_PageUp: select(m_selected.addDays(-m_days)); break;
    case Qt::Key_PageDown: select(m_selected.addDays(m_days)); break;
    case Qt::Key_Home: select(QDate::currentDate()); break;
    case Qt::Key_Return:
    case Qt::Key_Enter: emit newItemRequested(m_selected, QString()); break;
    default: QAbstractScrollArea::keyPressEvent(event);
    }
}

// ---- CalendarView ----------------------------------------------------------

CalendarView::CalendarView(ItemStore *store, QWidget *parent)
    : QWidget(parent), m_store(store), m_selected(QDate::currentDate()) {
    setObjectName("calendarView");
    m_grid = new MonthGrid(store);
    m_time = new TimeGrid(store);

    m_views = new QStackedWidget;
    m_views->addWidget(m_grid);
    m_views->addWidget(m_time);

    m_modes = new QTabBar;
    m_modes->setObjectName("calModes");
    m_modes->setDrawBase(false);
    m_modes->setExpanding(false);
    m_modes->setFocusPolicy(Qt::NoFocus);
    m_modes->addTab(tr("Month"));
    m_modes->addTab(tr("Week"));
    m_modes->addTab(tr("Day"));

    auto *prev = new QToolButton;
    prev->setObjectName("calPrev");
    prev->setArrowType(Qt::LeftArrow);
    prev->setToolTip(tr("Previous"));
    auto *next = new QToolButton;
    next->setObjectName("calNext");
    next->setArrowType(Qt::RightArrow);
    next->setToolTip(tr("Next"));
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
    bar->addWidget(m_modes);
    bar->addSpacing(10);
    bar->addWidget(prev);
    bar->addWidget(today);
    bar->addWidget(next);
    bar->addSpacing(10);
    bar->addWidget(add);

    auto *left = new QVBoxLayout;
    left->setContentsMargins(0, 0, 0, 0);
    left->setSpacing(0);
    left->addLayout(bar);
    left->addWidget(m_views, 1);

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

    connect(m_modes, &QTabBar::currentChanged, this, [this](int i) { setMode(Mode(i)); });
    connect(prev, &QToolButton::clicked, this, [this] { shift(-1); });
    connect(next, &QToolButton::clicked, this, [this] { shift(1); });
    connect(today, &QPushButton::clicked, this, &CalendarView::goToday);
    connect(add, &QPushButton::clicked, this, [this] { emit newItemRequested(selectedDay(), QString()); });
    connect(panelAdd, &QToolButton::clicked, this, [this] { emit newItemRequested(selectedDay(), QString()); });

    // The month grid and the time grid each select days; both feed setSelected().
    connect(m_grid, &MonthGrid::newItemRequested, this,
            [this](const QDate &d) { emit newItemRequested(Day::string(d), QString()); });
    connect(m_grid, &MonthGrid::selectionChanged, this, &CalendarView::setSelected);
    connect(m_grid, &MonthGrid::monthChanged, this, &CalendarView::refreshTitle);
    connect(m_time, &TimeGrid::selectionChanged, this, &CalendarView::setSelected);
    connect(m_time, &TimeGrid::newItemRequested, this,
            [this](const QDate &d, const QString &t) { emit newItemRequested(Day::string(d), t); });
    connect(m_time, &TimeGrid::editRequested, this, &CalendarView::editRequested);
    connect(m_store, &ItemStore::changed, this, &CalendarView::refreshPanel);

    connect(m_dayList, &ItemList::editRequested, this, &CalendarView::editRequested);
    connect(m_dayList, &ItemList::openEmailRequested, this, &CalendarView::openEmailRequested);
    connect(m_dayList, &ItemList::doneToggled, m_store, &ItemStore::setDone);
    connect(m_dayList, &ItemList::deleteRequested, m_store, &ItemStore::remove);

    syncTimeGrid();
    refreshTitle();
    refreshPanel();
}

int CalendarView::m_modes_index() const {
    return m_modes->currentIndex();
}

void CalendarView::setMode(Mode mode) {
    if (m_modes->currentIndex() != int(mode)) {
        m_modes->setCurrentIndex(int(mode)); // calls back here
        return;
    }
    m_views->setCurrentIndex(mode == Month ? 0 : 1);
    syncTimeGrid();
    refreshTitle();
    if (mode == Month)
        m_grid->setFocus();
    else {
        m_time->setFocus();
        m_time->scrollToWork();
    }
}

// The single place the selected day changes. The month grid follows the
// selection when it moves off the grid; the time grid follows when it leaves
// the range on screen.
void CalendarView::setSelected(const QDate &day) {
    if (!day.isValid())
        return;
    m_selected = day;
    if (m_grid->cellRect(day).isNull())
        m_grid->setMonth(day);
    m_grid->select(day);
    syncTimeGrid();
    m_time->select(day);
    refreshTitle();
    refreshPanel();
}

void CalendarView::syncTimeGrid() {
    if (mode() == Day) {
        m_time->setRange(m_selected, 1);
    } else {
        // The week containing the selection, starting on the locale's first day.
        const int lead = (m_selected.dayOfWeek() - int(QLocale().firstDayOfWeek()) + 7) % 7;
        m_time->setRange(m_selected.addDays(-lead), 7);
    }
}

void CalendarView::shift(int steps) {
    switch (mode()) {
    case Month: {
        // Keep a day selected inside the month now on screen.
        const QDate moved = m_grid->month().addMonths(steps);
        m_grid->setMonth(moved);
        const bool current = moved.year() == QDate::currentDate().year() && moved.month() == QDate::currentDate().month();
        setSelected(current ? QDate::currentDate() : moved);
        break;
    }
    case Week: setSelected(m_selected.addDays(7 * steps)); break;
    case Day: setSelected(m_selected.addDays(steps)); break;
    }
}

void CalendarView::goToday() {
    m_grid->setMonth(QDate::currentDate());
    setSelected(QDate::currentDate());
}

void CalendarView::refreshTitle() {
    QString text;
    switch (mode()) {
    case Month: text = QLocale().toString(m_grid->month(), QStringLiteral("MMMM yyyy")); break;
    case Week: {
        const QDate a = m_time->first(), b = a.addDays(6);
        text = a.month() == b.month()
                   ? QStringLiteral("%1–%2").arg(a.day()).arg(QLocale().toString(b, QStringLiteral("d MMMM yyyy")))
                   : QStringLiteral("%1 – %2").arg(QLocale().toString(a, QStringLiteral("d MMM")),
                                                  QLocale().toString(b, QStringLiteral("d MMM yyyy")));
        break;
    }
    case Day: text = QLocale().toString(m_selected, QStringLiteral("dddd d MMMM yyyy")); break;
    }
    m_title->setText(text);
}

void CalendarView::refreshPanel() {
    const QList<Item> items = m_store->on(selectedDay());
    m_dayTitle->setText(QLocale().toString(m_selected, QStringLiteral("dddd d MMMM")));
    m_dayList->setSections({{QString(), items}}, false);
    m_empty->setVisible(items.isEmpty());
    m_dayList->setVisible(!items.isEmpty());
}
