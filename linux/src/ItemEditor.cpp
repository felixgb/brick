#include "ItemEditor.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDateEdit>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QTimeEdit>
#include <QVBoxLayout>

#include "ItemStore.h"

ItemFields::ItemFields(const Item &item, QWidget *parent) : QWidget(parent), m_base(item) {
    m_title = new QLineEdit(item.title);
    m_title->setObjectName("itemTitle");
    m_title->setPlaceholderText(tr("What needs doing?"));

    m_hasDay = new QCheckBox(tr("On a day"));
    m_hasDay->setObjectName("itemHasDay");
    m_hasDay->setChecked(!item.date.isEmpty());
    m_day = new QDateEdit(Day::date(item.date).isValid() ? Day::date(item.date) : QDate::currentDate());
    m_day->setObjectName("itemDay");
    m_day->setCalendarPopup(true);
    m_day->setDisplayFormat(QStringLiteral("ddd d MMM yyyy"));

    m_hasEndDay = new QCheckBox(tr("Ends on another day"));
    m_hasEndDay->setObjectName("itemHasEndDay");
    m_hasEndDay->setChecked(Day::date(item.endDate).isValid());
    m_endDay = new QDateEdit(Day::date(item.endDate).isValid() ? Day::date(item.endDate)
                                                                  : m_day->date().addDays(1));
    m_endDay->setObjectName("itemEndDay");
    m_endDay->setCalendarPopup(true);
    m_endDay->setDisplayFormat(QStringLiteral("ddd d MMM yyyy"));

    m_hasStart = new QCheckBox(tr("At a time"));
    m_hasStart->setObjectName("itemHasStart");
    m_hasStart->setChecked(!item.startTime.isEmpty());
    m_start = new QTimeEdit(QTime::fromString(item.startTime, QStringLiteral("HH:mm")).isValid()
                                ? QTime::fromString(item.startTime, QStringLiteral("HH:mm"))
                                : QTime(9, 0));
    m_start->setDisplayFormat(QStringLiteral("HH:mm"));

    m_hasEnd = new QCheckBox(tr("Has an end time"));
    m_hasEnd->setObjectName("itemHasEnd");
    m_hasEnd->setChecked(!item.endTime.isEmpty());
    m_end = new QTimeEdit(QTime::fromString(item.endTime, QStringLiteral("HH:mm")).isValid()
                              ? QTime::fromString(item.endTime, QStringLiteral("HH:mm"))
                              : m_start->time().addSecs(3600));
    m_end->setDisplayFormat(QStringLiteral("HH:mm"));

    m_repeat = new QComboBox;
    m_repeat->setObjectName("itemRepeat");
    m_repeat->addItem(tr("Never"), QString());
    m_repeat->addItem(tr("Every day"), QStringLiteral("daily"));
    m_repeat->addItem(tr("Every week"), QStringLiteral("weekly"));
    m_repeat->addItem(tr("Every month"), QStringLiteral("monthly"));
    m_repeat->addItem(tr("Every year"), QStringLiteral("yearly"));
    m_repeat->setCurrentIndex(qMax(0, m_repeat->findData(item.repeat)));
    m_hasUntil = new QCheckBox(tr("Until"));
    m_hasUntil->setObjectName("itemHasUntil");
    m_hasUntil->setChecked(Day::date(item.repeatUntil).isValid());
    m_until = new QDateEdit(Day::date(item.repeatUntil).isValid() ? Day::date(item.repeatUntil)
                                                                     : m_day->date().addYears(1));
    m_until->setObjectName("itemUntil");
    m_until->setCalendarPopup(true);
    m_until->setDisplayFormat(QStringLiteral("ddd d MMM yyyy"));

    m_location = new QLineEdit(item.location);
    m_notes = new QPlainTextEdit(item.notes);
    m_notes->setTabChangesFocus(true);
    m_notes->setMinimumHeight(64);
    m_notes->setMaximumHeight(110);

    auto *form = new QFormLayout(this);
    form->setContentsMargins(0, 0, 0, 0);
    form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
    form->addRow(tr("Title"), m_title);
    form->addRow(m_hasDay, m_day);
    form->addRow(m_hasEndDay, m_endDay);
    form->addRow(m_hasStart, m_start);
    form->addRow(m_hasEnd, m_end);
    form->addRow(tr("Repeat"), m_repeat);
    form->addRow(m_hasUntil, m_until);
    form->addRow(tr("Location"), m_location);
    form->addRow(tr("Notes"), m_notes);

    // Starting a span later than it ends makes no sense; an end time only
    // makes sense with a start time, and a start time with a day.
    connect(m_hasDay, &QCheckBox::toggled, this, &ItemFields::syncEnabled);
    connect(m_hasEndDay, &QCheckBox::toggled, this, &ItemFields::syncEnabled);
    connect(m_hasUntil, &QCheckBox::toggled, this, &ItemFields::syncEnabled);
    connect(m_repeat, qOverload<int>(&QComboBox::currentIndexChanged), this, &ItemFields::syncEnabled);
    // The end can't come before the start; moving the start drags a too-early end along.
    connect(m_day, &QDateEdit::dateChanged, this, [this](const QDate &d) {
        if (m_endDay->date() <= d)
            m_endDay->setDate(d.addDays(1));
        if (m_until->date() < d)
            m_until->setDate(d);
    });
    connect(m_hasStart, &QCheckBox::toggled, this, &ItemFields::syncEnabled);
    connect(m_hasEnd, &QCheckBox::toggled, this, [this](bool on) {
        if (on)
            m_end->setTime(m_start->time().addSecs(3600));
        syncEnabled();
    });
    connect(m_title, &QLineEdit::textChanged, this, &ItemFields::titleChanged);
    syncEnabled();
}

void ItemFields::syncEnabled() {
    const bool day = m_hasDay->isChecked();
    const bool start = day && m_hasStart->isChecked();
    m_day->setEnabled(day);
    m_hasEndDay->setEnabled(day);
    m_endDay->setEnabled(day && m_hasEndDay->isChecked());
    m_repeat->setEnabled(day);
    m_hasUntil->setEnabled(day && !m_repeat->currentData().toString().isEmpty());
    m_until->setEnabled(m_hasUntil->isEnabled() && m_hasUntil->isChecked());
    m_hasStart->setEnabled(day);
    m_start->setEnabled(start);
    m_hasEnd->setEnabled(start);
    m_end->setEnabled(start && m_hasEnd->isChecked());
}

void ItemFields::focusTitle() {
    m_title->setFocus();
    m_title->selectAll();
}

Item ItemFields::item() const {
    Item i = m_base;
    i.title = m_title->text().trimmed();
    i.location = m_location->text().trimmed();
    i.notes = m_notes->toPlainText();
    if (!m_hasDay->isChecked()) {
        i.date.clear();
        i.endDate.clear();
        i.startTime.clear();
        i.endTime.clear();
        i.repeat.clear();
        i.repeatUntil.clear();
        return i;
    }
    i.date = Day::string(m_day->date());
    // A span that would end before it starts makes no sense.
    i.endDate = m_hasEndDay->isChecked() && m_endDay->date() > m_day->date() ? Day::string(m_endDay->date()) : QString();
    i.repeat = m_repeat->currentData().toString();
    i.repeatUntil = !i.repeat.isEmpty() && m_hasUntil->isChecked() ? Day::string(m_until->date()) : QString();
    if (!m_hasStart->isChecked()) {
        i.startTime.clear();
        i.endTime.clear();
    } else {
        i.startTime = m_start->time().toString(QStringLiteral("HH:mm"));
        i.endTime = m_hasEnd->isChecked() ? m_end->time().toString(QStringLiteral("HH:mm")) : QString();
    }
    return i;
}

// ---------------------------------------------------------------------------

ItemEditor::ItemEditor(ItemStore *store, const Item &item, QWidget *parent)
    : QDialog(parent), m_store(store), m_item(item) {
    const bool isNew = item.id == 0;
    setObjectName("itemEditor");
    setWindowTitle(isNew ? tr("New item") : tr("Edit item"));
    setMinimumWidth(440);

    m_fields = new ItemFields(item);

    m_done = new QCheckBox(tr("Done"));
    m_done->setChecked(item.done);
    m_done->setVisible(!isNew);

    m_problem = new QLabel;
    m_problem->setStyleSheet(QStringLiteral("color:#c0392b"));
    m_problem->setWordWrap(true);
    m_problem->hide();

    auto *buttons = new QDialogButtonBox;
    auto *save = buttons->addButton(isNew ? tr("Add") : tr("Save"), QDialogButtonBox::AcceptRole);
    save->setObjectName("itemSave");
    save->setDefault(true);
    save->setEnabled(!item.title.trimmed().isEmpty());
    buttons->addButton(QDialogButtonBox::Cancel);
    connect(m_fields, &ItemFields::titleChanged, this,
            [save](const QString &t) { save->setEnabled(!t.trimmed().isEmpty()); });
    connect(buttons, &QDialogButtonBox::accepted, this, &ItemEditor::save);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->setSpacing(12);
    layout->addWidget(m_fields);
    layout->addWidget(m_done);

    if (item.isFromEmail()) {
        auto *row = new QHBoxLayout;
        auto *from = new QLabel(tr("From “%1”").arg(item.sourceSubject));
        from->setEnabled(false);
        row->addWidget(from, 1);
        if (!isNew) {
            auto *open = new QPushButton(tr("Open email"));
            connect(open, &QPushButton::clicked, this, [this] {
                emit openEmailRequested(m_item);
                reject();
            });
            row->addWidget(open);
        }
        layout->addLayout(row);
    }
    layout->addWidget(m_problem);

    auto *bottom = new QHBoxLayout;
    if (!isNew) {
        auto *del = new QPushButton(tr("Delete"));
        connect(del, &QPushButton::clicked, this, [this] {
            m_store->remove(m_item);
            reject();
        });
        bottom->addWidget(del);
    }
    bottom->addStretch(1);
    bottom->addWidget(buttons);
    layout->addLayout(bottom);

    if (isNew)
        m_fields->focusTitle();
}

void ItemEditor::save() {
    Item i = m_fields->item();
    if (m_item.id != 0)
        i.done = m_done->isChecked();
    const QString error = m_store->save(i);
    if (error.isEmpty()) {
        accept();
        return;
    }
    m_problem->setText(error);
    m_problem->show();
}
