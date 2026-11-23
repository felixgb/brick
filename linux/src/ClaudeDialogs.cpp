#include "ClaudeDialogs.h"

#include <QCheckBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QScrollArea>
#include <QVBoxLayout>

#include "Core.h"
#include "ItemEditor.h"
#include "ItemStore.h"

bool ClaudeSettingsDialog::isConfigured(Core *core) {
    return core->call("claude.status").toObject().value("configured").toBool();
}

ClaudeSettingsDialog::ClaudeSettingsDialog(Core *core, QWidget *parent) : QDialog(parent), m_core(core) {
    setObjectName("claudeSettings");
    setWindowTitle(tr("Claude settings"));
    setMinimumWidth(460);

    const QJsonObject status = core->call("claude.status").toObject();
    const bool configured = status.value("configured").toBool();
    const QString defaultModel = status.value("default_model").toString();
    const QString current = status.value("model").toString();

    m_key = new QLineEdit;
    m_key->setObjectName("claudeKey");
    m_key->setEchoMode(QLineEdit::Password);
    m_key->setPlaceholderText(configured ? tr("Saved") : QStringLiteral("sk-ant-…"));
    m_model = new QLineEdit(current == defaultModel ? QString() : current);
    m_model->setPlaceholderText(defaultModel);

    auto *form = new QFormLayout;
    form->addRow(tr("Anthropic API key"), m_key);
    form->addRow(tr("Model"), m_model);

    auto *note = new QLabel(
        tr("When you choose Add to To-do on an email, its sender, recipients, subject, date and text are sent to "
           "Anthropic's API using this key. Attachments are not sent. Nothing is sent at any other time."));
    note->setWordWrap(true);
    note->setEnabled(false);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Save | QDialogButtonBox::Cancel);
    if (configured) {
        auto *remove = buttons->addButton(tr("Remove key"), QDialogButtonBox::DestructiveRole);
        connect(remove, &QPushButton::clicked, this, [this] {
            m_core->call("claude.set_key", {{"key", QString()}});
            accept();
        });
    }
    connect(buttons, &QDialogButtonBox::accepted, this, &ClaudeSettingsDialog::save);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->setSpacing(14);
    layout->addLayout(form);
    layout->addWidget(note);
    layout->addWidget(buttons);
}

void ClaudeSettingsDialog::save() {
    if (!m_key->text().trimmed().isEmpty())
        m_core->call("claude.set_key", {{"key", m_key->text().trimmed()}});
    m_core->call("settings.set", {{"key", "claude_model"}, {"value", m_model->text().trimmed()}});
    accept();
}

// ---------------------------------------------------------------------------

ExtractionDialog::ExtractionDialog(ItemStore *store, const QString &subject, const QString &summary, bool truncated,
                                   const QList<Item> &proposals, QWidget *parent)
    : QDialog(parent), m_store(store) {
    setObjectName("extractionDialog");
    setWindowTitle(tr("Add to To-do"));
    setMinimumWidth(540);

    auto *layout = new QVBoxLayout(this);
    layout->setSpacing(10);

    auto *heading = new QLabel(proposals.isEmpty() ? tr("Nothing to add") : tr("Add to your to-do list and calendar"));
    QFont f = heading->font();
    f.setBold(true);
    f.setPointSizeF(f.pointSizeF() * 1.15);
    heading->setFont(f);
    layout->addWidget(heading);
    auto *from = new QLabel(tr("From “%1”").arg(subject));
    from->setEnabled(false);
    layout->addWidget(from);

    if (!summary.isEmpty()) {
        auto *s = new QLabel(summary);
        s->setWordWrap(true);
        layout->addWidget(s);
    }
    if (truncated) {
        auto *t = new QLabel(tr("This email was very long, so Claude only read the first part."));
        t->setEnabled(false);
        layout->addWidget(t);
    }

    if (!proposals.isEmpty()) {
        auto *inner = new QWidget;
        auto *cards = new QVBoxLayout(inner);
        cards->setContentsMargins(0, 0, 4, 0);
        for (const Item &p : proposals) {
            auto *frame = new QFrame;
            frame->setFrameShape(QFrame::StyledPanel);
            auto *row = new QHBoxLayout(frame);
            auto *include = new QCheckBox;
            include->setChecked(true);
            include->setToolTip(tr("Include this item"));
            auto *fields = new ItemFields(p);
            row->addWidget(include, 0, Qt::AlignTop);
            row->addWidget(fields, 1);
            connect(include, &QCheckBox::toggled, fields, &QWidget::setEnabled);
            m_cards.append({include, fields});
            cards->addWidget(frame);
        }
        cards->addStretch(1);
        auto *scroll = new QScrollArea;
        scroll->setWidget(inner);
        scroll->setWidgetResizable(true);
        scroll->setFrameShape(QFrame::NoFrame);
        scroll->setMinimumHeight(260);
        layout->addWidget(scroll, 1);
    }

    auto *buttons = new QDialogButtonBox;
    if (proposals.isEmpty()) {
        buttons->addButton(QDialogButtonBox::Close);
        connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    } else {
        auto *add = buttons->addButton(tr("Add"), QDialogButtonBox::AcceptRole);
        add->setObjectName("extractionAdd");
        buttons->addButton(QDialogButtonBox::Cancel);
        connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
        connect(buttons, &QDialogButtonBox::accepted, this, [this] {
            QStringList failures;
            for (const Card &c : m_cards) {
                const Item item = c.fields->item();
                if (!c.include->isChecked() || item.title.isEmpty())
                    continue;
                const QString error = m_store->save(item);
                if (error.isEmpty())
                    c.include->setChecked(false); // saved; don't save it twice on a retry
                else
                    failures << error;
            }
            if (failures.isEmpty())
                accept();
            else
                // Keep the dialog open so nothing the user edited is lost.
                setWindowTitle(failures.first());
        });
    }
    layout->addWidget(buttons);
}
