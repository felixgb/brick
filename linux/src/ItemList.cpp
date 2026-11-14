#include "ItemList.h"

#include <QAbstractListModel>
#include <QApplication>
#include <QContextMenuEvent>
#include <QKeyEvent>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QScrollBar>
#include <QStyledItemDelegate>

namespace {

constexpr int Pad = 10;
constexpr int CheckSize = 18;
constexpr int RowHeight = 46;
constexpr int HeaderHeight = 30;
constexpr int EnvelopeW = 18;

// The tick circle sits at the left of a row, the envelope at the right.
QRect checkRect(const QRect &row) {
    return QRect(row.left() + Pad, row.top() + (RowHeight - CheckSize) / 2, CheckSize, CheckSize);
}

QRect envelopeRect(const QRect &row) {
    return QRect(row.right() - Pad - EnvelopeW, row.top() + (RowHeight - 13) / 2, EnvelopeW, 13);
}

QString detailOf(const Item &item, bool showsDay) {
    QStringList parts;
    if (showsDay && !item.whenLabel().isEmpty()) {
        parts << item.whenLabel();
    } else if (!item.startTime.isEmpty()) {
        parts << (item.endTime.isEmpty() ? item.startTime : item.startTime + QObject::tr(" to ") + item.endTime);
    }
    if (!item.location.isEmpty())
        parts << item.location;
    if (!item.repeat.isEmpty())
        parts << item.repeatLabel();
    return parts.join(QStringLiteral(" · "));
}

} // namespace

class ItemListModel : public QAbstractListModel {
public:
    struct Row {
        bool header = false;
        QString title;
        Item item;
    };
    enum { ItemRole = Qt::UserRole + 1 };

    using QAbstractListModel::QAbstractListModel;

    void set(const QList<Row> &rows, bool showsDay) {
        beginResetModel();
        m_rows = rows;
        m_showsDay = showsDay;
        endResetModel();
    }
    bool showsDay() const { return m_showsDay; }
    const Row &row(int i) const { return m_rows.at(i); }

    int rowCount(const QModelIndex &parent = {}) const override { return parent.isValid() ? 0 : m_rows.size(); }

    QVariant data(const QModelIndex &index, int role) const override {
        if (!index.isValid() || index.row() >= m_rows.size())
            return {};
        const Row &r = m_rows.at(index.row());
        if (role == Qt::DisplayRole)
            return r.header ? r.title : r.item.title;
        return {};
    }

    Qt::ItemFlags flags(const QModelIndex &index) const override {
        if (!index.isValid() || m_rows.at(index.row()).header)
            return Qt::NoItemFlags;
        return Qt::ItemIsEnabled | Qt::ItemIsSelectable;
    }

private:
    QList<Row> m_rows;
    bool m_showsDay = true;
};

namespace {

class Delegate : public QStyledItemDelegate {
public:
    Delegate(ItemListModel *model, QObject *parent) : QStyledItemDelegate(parent), m_model(model) {}

    QSize sizeHint(const QStyleOptionViewItem &, const QModelIndex &index) const override {
        return QSize(100, m_model->row(index.row()).header ? HeaderHeight : RowHeight);
    }

    void paint(QPainter *p, const QStyleOptionViewItem &opt, const QModelIndex &index) const override {
        const ItemListModel::Row &r = m_model->row(index.row());
        const QPalette &pal = opt.palette;
        p->save();
        p->setRenderHint(QPainter::Antialiasing);

        if (r.header) {
            QFont f = opt.font;
            f.setBold(true);
            f.setPointSizeF(f.pointSizeF() * 0.9);
            p->setFont(f);
            p->setPen(pal.color(QPalette::PlaceholderText));
            p->drawText(opt.rect.adjusted(Pad, 8, -Pad, 0), Qt::AlignLeft | Qt::AlignVCenter, r.title.toUpper());
            p->restore();
            return;
        }

        const Item &item = r.item;
        if (opt.state & QStyle::State_Selected)
            p->fillRect(opt.rect, pal.color(QPalette::Highlight).lighter(item.done ? 170 : 150).toRgb());
        else if (opt.state & QStyle::State_MouseOver)
            p->fillRect(opt.rect, pal.color(QPalette::Highlight).lighter(190));

        // Tick circle.
        const QRect box = checkRect(opt.rect);
        const QColor accent = pal.color(QPalette::Highlight);
        if (item.done) {
            p->setPen(Qt::NoPen);
            p->setBrush(accent);
            p->drawEllipse(box);
            QPen tick(Qt::white, 2, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin);
            p->setPen(tick);
            p->drawPolyline(QPolygonF{QPointF(box.left() + 5, box.center().y() + 0.5),
                                      QPointF(box.left() + 8, box.bottom() - 5),
                                      QPointF(box.right() - 4, box.top() + 5)});
        } else {
            p->setPen(QPen(pal.color(QPalette::Mid), 1.6));
            p->setBrush(Qt::NoBrush);
            p->drawEllipse(box.adjusted(1, 1, -1, -1));
        }

        // Text.
        const int left = box.right() + 10;
        const int right = opt.rect.right() - Pad - (item.isFromEmail() ? EnvelopeW + 8 : 0);
        const QString detail = detailOf(item, m_model->showsDay());
        const QFontMetrics fm(opt.font);
        QFont title = opt.font;
        title.setStrikeOut(item.done);
        p->setFont(title);
        p->setPen(pal.color(item.done ? QPalette::PlaceholderText : QPalette::Text));
        const int titleY = detail.isEmpty() ? opt.rect.center().y() - fm.height() / 2 : opt.rect.top() + 6;
        p->drawText(QRect(left, titleY, right - left, fm.height()), Qt::AlignLeft | Qt::AlignVCenter,
                    fm.elidedText(item.title, Qt::ElideRight, right - left));
        if (!detail.isEmpty()) {
            QFont small = opt.font;
            small.setPointSizeF(small.pointSizeF() * 0.88);
            const QFontMetrics sfm(small);
            p->setFont(small);
            p->setPen(item.isOverdue() ? QColor(0xD0, 0x3B, 0x3B) : pal.color(QPalette::PlaceholderText));
            p->drawText(QRect(left, opt.rect.top() + 6 + fm.height() + 1, right - left, sfm.height()),
                        Qt::AlignLeft | Qt::AlignVCenter, sfm.elidedText(detail, Qt::ElideRight, right - left));
        }

        // Envelope: this item was made from an email.
        if (item.isFromEmail()) {
            const QRect e = envelopeRect(opt.rect);
            p->setPen(QPen(pal.color(QPalette::PlaceholderText), 1.3));
            p->setBrush(Qt::NoBrush);
            p->drawRoundedRect(QRectF(e), 2, 2);
            p->drawPolyline(QPolygonF{QPointF(e.left() + 1, e.top() + 1), QPointF(e.center().x() + 0.5, e.center().y() + 1),
                                      QPointF(e.right() - 1, e.top() + 1)});
        }

        // Row divider.
        p->setPen(pal.color(QPalette::Mid).lighter(150));
        p->drawLine(opt.rect.bottomLeft() + QPoint(Pad, 0), opt.rect.bottomRight() - QPoint(Pad, 0));
        p->restore();
    }

private:
    ItemListModel *m_model;
};

} // namespace

ItemList::ItemList(QWidget *parent) : QListView(parent), m_model(new ItemListModel(this)) {
    setModel(m_model);
    setItemDelegate(new Delegate(m_model, this));
    setUniformItemSizes(false);
    setFrameShape(QFrame::NoFrame);
    setMouseTracking(true);
    setSelectionMode(QAbstractItemView::SingleSelection);
    setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
    setEditTriggers(QAbstractItemView::NoEditTriggers);
}

void ItemList::setSections(const QList<Section> &sections, bool showsDay) {
    // Keep the selection across redraws, by item id.
    qint64 selected = 0;
    const QModelIndex current = currentIndex();
    if (current.isValid() && current.row() < m_model->rowCount() && !m_model->row(current.row()).header)
        selected = m_model->row(current.row()).item.id;
    const int scroll = verticalScrollBar()->value();

    QList<ItemListModel::Row> rows;
    for (const Section &s : sections) {
        if (s.items.isEmpty())
            continue;
        if (!s.title.isEmpty()) {
            ItemListModel::Row h;
            h.header = true;
            h.title = s.title;
            rows.append(h);
        }
        for (const Item &i : s.items) {
            ItemListModel::Row r;
            r.item = i;
            rows.append(r);
        }
    }
    m_model->set(rows, showsDay);
    verticalScrollBar()->setValue(scroll);
    if (selected != 0) {
        for (int i = 0; i < rows.size(); ++i) {
            if (!rows.at(i).header && rows.at(i).item.id == selected)
                setCurrentIndex(m_model->index(i));
        }
    }
}

int ItemList::itemCount() const {
    int n = 0;
    for (int i = 0; i < m_model->rowCount(); ++i)
        n += m_model->row(i).header ? 0 : 1;
    return n;
}

bool ItemList::itemAt(const QPoint &pos, Item *out) const {
    const QModelIndex index = indexAt(pos);
    if (!index.isValid() || m_model->row(index.row()).header)
        return false;
    *out = m_model->row(index.row()).item;
    return true;
}

void ItemList::mousePressEvent(QMouseEvent *event) {
    const QModelIndex index = indexAt(event->pos());
    if (event->button() == Qt::LeftButton && index.isValid() && !m_model->row(index.row()).header) {
        const Item &item = m_model->row(index.row()).item;
        const QRect row = visualRect(index);
        if (checkRect(row).adjusted(-6, -6, 6, 6).contains(event->pos())) {
            emit doneToggled(item, !item.done);
            return;
        }
        if (item.isFromEmail() && envelopeRect(row).adjusted(-6, -6, 6, 6).contains(event->pos())) {
            emit openEmailRequested(item);
            return;
        }
    }
    QListView::mousePressEvent(event);
}

void ItemList::mouseDoubleClickEvent(QMouseEvent *event) {
    Item item;
    if (event->button() == Qt::LeftButton && itemAt(event->pos(), &item))
        emit editRequested(item);
}

void ItemList::keyPressEvent(QKeyEvent *event) {
    Item item;
    const QModelIndex index = currentIndex();
    if (index.isValid() && !m_model->row(index.row()).header) {
        item = m_model->row(index.row()).item;
        switch (event->key()) {
        case Qt::Key_Return:
        case Qt::Key_Enter:
            emit editRequested(item);
            return;
        case Qt::Key_Space:
            emit doneToggled(item, !item.done);
            return;
        case Qt::Key_Delete:
        case Qt::Key_Backspace:
            emit deleteRequested(item);
            return;
        default:
            break;
        }
    }
    QListView::keyPressEvent(event);
}

void ItemList::contextMenuEvent(QContextMenuEvent *event) {
    Item item;
    if (!itemAt(event->pos(), &item))
        return;
    setCurrentIndex(indexAt(event->pos()));
    QMenu menu(this);
    menu.addAction(tr("Edit…"), this, [this, item] { emit editRequested(item); });
    menu.addAction(item.done ? tr("Mark as not done") : tr("Mark as done"), this,
                   [this, item] { emit doneToggled(item, !item.done); });
    if (item.isFromEmail())
        menu.addAction(tr("Open email"), this, [this, item] { emit openEmailRequested(item); });
    menu.addSeparator();
    menu.addAction(tr("Delete"), this, [this, item] { emit deleteRequested(item); });
    menu.exec(event->globalPos());
}
