#include "TodoView.h"

#include <QCheckBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QVBoxLayout>

#include "ItemList.h"
#include "ItemStore.h"

TodoView::TodoView(ItemStore *store, QWidget *parent) : QWidget(parent), m_store(store) {
    setObjectName("todoView");

    m_quick = new QLineEdit;
    m_quick->setObjectName("todoQuickAdd");
    m_quick->setPlaceholderText(tr("Add a to-do"));
    m_quick->setClearButtonEnabled(true);
    connect(m_quick, &QLineEdit::returnPressed, this, &TodoView::addQuick);

    m_showDone = new QCheckBox(tr("Show done"));
    m_showDone->setObjectName("todoShowDone");
    connect(m_showDone, &QCheckBox::toggled, this, &TodoView::rebuild);

    auto *more = new QPushButton(tr("New item…"));
    more->setToolTip(tr("Add an item with a date, time or notes"));
    connect(more, &QPushButton::clicked, this, &TodoView::newItemRequested);

    m_count = new QLabel;
    m_count->setEnabled(false);

    m_list = new ItemList;
    m_list->setObjectName("todoList");
    connect(m_list, &ItemList::editRequested, this, &TodoView::editRequested);
    connect(m_list, &ItemList::openEmailRequested, this, &TodoView::openEmailRequested);
    connect(m_list, &ItemList::doneToggled, m_store, &ItemStore::setDone);
    connect(m_list, &ItemList::deleteRequested, m_store, &ItemStore::remove);

    m_empty = new QLabel(tr("Nothing to do yet.\nType a to-do above, or open an email and choose Add to To-do."));
    m_empty->setAlignment(Qt::AlignCenter);
    m_empty->setEnabled(false);

    auto *top = new QHBoxLayout;
    top->setContentsMargins(0, 0, 0, 0);
    top->addWidget(m_quick, 1);
    top->addWidget(m_showDone);
    top->addWidget(more);

    auto *column = new QWidget;
    column->setMaximumWidth(780);
    auto *layout = new QVBoxLayout(column);
    layout->setContentsMargins(0, 14, 0, 6);
    layout->setSpacing(8);
    layout->addLayout(top);
    layout->addWidget(m_list, 1);
    layout->addWidget(m_empty, 1);
    layout->addWidget(m_count, 0, Qt::AlignRight);

    auto *outer = new QHBoxLayout(this);
    outer->setContentsMargins(16, 0, 16, 0);
    outer->addStretch(1);
    outer->addWidget(column, 100);
    outer->addStretch(1);

    connect(m_store, &ItemStore::changed, this, &TodoView::rebuild);
    rebuild();
}

void TodoView::rebuild() {
    const QString today = Day::today();
    QList<Item> overdue, dueToday, upcoming, undated, done;
    int open = 0;
    for (const Item &i : m_store->items()) {
        if (i.done) {
            done.append(i);
            continue;
        }
        ++open;
        if (i.isOverdue())
            overdue.append(i);
        else if (i.occursOn(today))
            dueToday.append(i);
        else if (i.date > today)
            upcoming.append(i);
        else if (i.date.isEmpty())
            undated.append(i);
    }

    QList<ItemList::Section> sections{
        {tr("Overdue"), overdue}, {tr("Today"), dueToday}, {tr("Upcoming"), upcoming}, {tr("No date"), undated}};
    if (m_showDone->isChecked())
        sections.append({tr("Done"), done});
    m_list->setSections(sections);

    const bool empty = m_store->items().isEmpty();
    m_empty->setVisible(empty);
    m_list->setVisible(!empty);
    m_count->setText(open == 0 ? tr("All done") : tr("%n to do", nullptr, open));
}

void TodoView::addQuick() {
    const QString title = m_quick->text().trimmed();
    if (title.isEmpty())
        return;
    Item item;
    item.title = title;
    if (m_store->save(item).isEmpty())
        m_quick->clear();
}
