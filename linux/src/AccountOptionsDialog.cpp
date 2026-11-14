#include "AccountOptionsDialog.h"

#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QPlainTextEdit>
#include <QRegularExpression>
#include <QVBoxLayout>

#include "Core.h"

AccountOptionsDialog::AccountOptionsDialog(Core *core, QWidget *parent) : QDialog(parent), m_core(core) {
    setObjectName("accountOptions");
    setWindowTitle(tr("Signatures and addresses"));
    setMinimumWidth(480);

    m_account = new QComboBox;
    m_account->setObjectName("optionsAccount");
    for (const QJsonValue &v : m_core->call("accounts.list").toArray()) {
        const QJsonObject a = v.toObject();
        m_account->addItem(a.value("email").toString(), qint64(a.value("id").toDouble()));
    }

    m_aliases = new QPlainTextEdit;
    m_aliases->setObjectName("optionsAliases");
    m_aliases->setPlaceholderText(tr("One address per line"));
    m_aliases->setMaximumHeight(80);
    m_signature = new QPlainTextEdit;
    m_signature->setObjectName("optionsSignature");
    m_signature->setPlaceholderText(tr("Added below what you write in new messages"));
    m_signature->setMinimumHeight(90);

    auto *note = new QLabel(tr("Other addresses must be set up with your mail provider as ones this account may "
                               "send from. They then appear in the From list when writing."));
    note->setWordWrap(true);
    note->setEnabled(false);

    auto *form = new QFormLayout;
    form->addRow(tr("Account"), m_account);
    form->addRow(tr("Also send as"), m_aliases);
    form->addRow(tr("Signature"), m_signature);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Save | QDialogButtonBox::Cancel);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] {
        store();
        accept();
    });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->addLayout(form);
    layout->addWidget(note);
    layout->addWidget(buttons);

    connect(m_account, qOverload<int>(&QComboBox::currentIndexChanged), this, [this] {
        store(); // keep edits made for the account being left
        load();
    });
    load();
}

void AccountOptionsDialog::load() {
    m_shownAccount = m_account->currentData().toLongLong();
    if (m_shownAccount == 0)
        return;
    auto get = [this](const char *prefix) {
        return m_core->call("settings.get", {{"key", QStringLiteral("%1_%2").arg(QLatin1String(prefix)).arg(m_shownAccount)}})
            .toString();
    };
    m_aliases->setPlainText(get("aliases"));
    m_signature->setPlainText(get("signature"));
}

void AccountOptionsDialog::store() {
    if (m_shownAccount == 0)
        return;
    // Store aliases one per line.
    const QStringList aliases = m_aliases->toPlainText().split(QRegularExpression(QStringLiteral("[,;\\s]+")), Qt::SkipEmptyParts);
    m_core->call("settings.set", {{"key", QStringLiteral("aliases_%1").arg(m_shownAccount)},
                                  {"value", aliases.join(QLatin1Char('\n'))}});
    m_core->call("settings.set", {{"key", QStringLiteral("signature_%1").arg(m_shownAccount)},
                                  {"value", m_signature->toPlainText().trimmed()}});
}
