#include "AccountDialog.h"

#include <QComboBox>
#include <QDesktopServices>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QUrl>
#include <QVBoxLayout>

#include "Core.h"

AccountDialog::AccountDialog(Core *core, QWidget *parent) : QDialog(parent), m_core(core) {
    setWindowTitle(tr("Add account"));
    setMinimumWidth(460);

    m_providers = m_core->call("providers.list").toArray();
    m_provider = new QComboBox;
    for (const QJsonValue &v : m_providers)
        m_provider->addItem(v.toObject().value("label").toString());

    m_name = new QLineEdit;
    m_name->setPlaceholderText(tr("Shown to people you write to"));
    m_email = new QLineEdit;
    m_email->setPlaceholderText(QStringLiteral("name@example.com"));
    m_password = new QLineEdit;
    m_password->setEchoMode(QLineEdit::Password);
    m_clientId = new QLineEdit;
    m_clientSecret = new QLineEdit;
    m_clientSecret->setEchoMode(QLineEdit::Password);
    m_username = new QLineEdit;
    m_username->setPlaceholderText(tr("Defaults to the email address"));
    m_imapHost = new QLineEdit;
    m_imapPort = new QSpinBox;
    m_imapPort->setRange(1, 65535);
    m_smtpHost = new QLineEdit;
    m_smtpPort = new QSpinBox;
    m_smtpPort->setRange(1, 65535);
    m_smtpSecurity = new QComboBox;
    m_smtpSecurity->addItem(tr("TLS"), QStringLiteral("tls"));
    m_smtpSecurity->addItem(tr("STARTTLS"), QStringLiteral("starttls"));

    m_form = new QFormLayout;
    m_form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
    m_form->addRow(tr("Provider"), m_provider);
    m_form->addRow(tr("Your name"), m_name);
    m_form->addRow(tr("Email"), m_email);
    m_form->addRow(tr("Password"), m_password);
    m_form->addRow(tr("OAuth client ID"), m_clientId);
    m_form->addRow(tr("OAuth client secret"), m_clientSecret);
    m_form->addRow(tr("Username"), m_username);
    m_form->addRow(tr("IMAP server"), m_imapHost);
    m_form->addRow(tr("IMAP port"), m_imapPort);
    m_form->addRow(tr("SMTP server"), m_smtpHost);
    m_form->addRow(tr("SMTP port"), m_smtpPort);
    m_form->addRow(tr("SMTP security"), m_smtpSecurity);

    m_hint = new QLabel;
    m_hint->setWordWrap(true);
    m_status = new QLabel;
    m_status->setWordWrap(true);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Cancel);
    m_submit = buttons->addButton(tr("Add account"), QDialogButtonBox::AcceptRole);
    m_submit->setDefault(true);

    auto *layout = new QVBoxLayout(this);
    layout->addLayout(m_form);
    layout->addWidget(m_hint);
    layout->addWidget(m_status);
    layout->addStretch(1);
    layout->addWidget(buttons);

    connect(m_provider, &QComboBox::currentIndexChanged, this, &AccountDialog::providerChanged);
    connect(buttons, &QDialogButtonBox::accepted, this, &AccountDialog::submit);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_core, &Core::event, this, &AccountDialog::onEvent);
    providerChanged();
}

QJsonObject AccountDialog::provider() const {
    return m_providers.at(m_provider->currentIndex()).toObject();
}

void AccountDialog::setRowVisible(QWidget *field, bool visible) {
    field->setVisible(visible);
    if (QWidget *label = m_form->labelForField(field))
        label->setVisible(visible);
}

void AccountDialog::providerChanged() {
    const QJsonObject p = provider();
    const QJsonObject oauth = p.value("oauth").toObject();
    const bool isOAuth = !oauth.isEmpty();
    const bool isCustom = p.value("id").toString() == QLatin1String("custom");
    const bool needsSecret = p.value("id").toString() == QLatin1String("gmail");

    setRowVisible(m_password, !isOAuth);
    setRowVisible(m_clientId, isOAuth);
    setRowVisible(m_clientSecret, isOAuth && needsSecret);
    for (QWidget *w : {static_cast<QWidget *>(m_username), static_cast<QWidget *>(m_imapHost),
                       static_cast<QWidget *>(m_imapPort), static_cast<QWidget *>(m_smtpHost),
                       static_cast<QWidget *>(m_smtpPort), static_cast<QWidget *>(m_smtpSecurity)})
        setRowVisible(w, isCustom);

    if (isOAuth) {
        m_clientId->setText(m_core->call("settings.get", {{"key", oauth.value("client_id_key")}}).toString());
        m_clientSecret->setText(m_core->call("settings.get", {{"key", oauth.value("client_secret_key")}}).toString());
    }
    m_imapPort->setValue(p.value("imap_port").toInt(993));
    m_smtpPort->setValue(p.value("smtp_port").toInt(465));
    m_hint->setText(p.value("hint").toString());
    m_submit->setText(isOAuth ? tr("Sign in with browser") : tr("Add account"));
    m_status->clear();
    adjustSize();
}

void AccountDialog::setBusy(bool busy, const QString &status) {
    m_waiting = busy;
    m_submit->setEnabled(!busy);
    m_provider->setEnabled(!busy);
    m_status->setText(status);
}

void AccountDialog::submit() {
    const QJsonObject p = provider();
    const QJsonObject oauth = p.value("oauth").toObject();
    QJsonObject params{
        {"provider", p.value("id")},
        {"email", m_email->text().trimmed()},
        {"name", m_name->text().trimmed()},
    };
    QString error;

    if (!oauth.isEmpty()) {
        m_core->call("settings.set",
                     {{"key", oauth.value("client_id_key")}, {"value", m_clientId->text().trimmed()}});
        m_core->call("settings.set",
                     {{"key", oauth.value("client_secret_key")}, {"value", m_clientSecret->text().trimmed()}});
        const QJsonObject r = m_core->call("accounts.oauth", params, &error).toObject();
        if (!error.isEmpty()) {
            m_status->setText(error);
            return;
        }
        QDesktopServices::openUrl(QUrl(r.value("auth_url").toString()));
        setBusy(true, tr("Finish signing in in your browser…"));
        return;
    }

    params.insert("password", m_password->text());
    if (p.value("id").toString() == QLatin1String("custom")) {
        params.insert("username", m_username->text().trimmed());
        params.insert("imap_host", m_imapHost->text().trimmed());
        params.insert("imap_port", m_imapPort->value());
        params.insert("smtp_host", m_smtpHost->text().trimmed());
        params.insert("smtp_port", m_smtpPort->value());
        params.insert("smtp_security", m_smtpSecurity->currentData().toString());
    }
    m_core->call("accounts.add", params, &error);
    if (!error.isEmpty()) {
        m_status->setText(error);
        return;
    }
    setBusy(true, tr("Checking with the server…"));
}

void AccountDialog::onEvent(const QJsonObject &event) {
    if (!m_waiting)
        return;
    const QString type = event.value("type").toString();
    if (type == QLatin1String("account_added")) {
        accept();
    } else if (type == QLatin1String("account_error")) {
        setBusy(false, event.value("message").toString());
    }
}
