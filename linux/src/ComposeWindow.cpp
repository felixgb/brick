#include "ComposeWindow.h"

#include <QComboBox>
#include <QFontDatabase>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QShortcut>
#include <QUuid>
#include <QVBoxLayout>

#include "Core.h"

ComposeWindow::ComposeWindow(Core *core, const Draft &draft, QWidget *parent)
    : QWidget(parent, Qt::Window), m_core(core), m_token(QUuid::createUuid().toString(QUuid::WithoutBraces)),
      m_replyToId(draft.replyToId) {
    setAttribute(Qt::WA_DeleteOnClose);
    setWindowTitle(draft.subject.isEmpty() ? tr("New message") : draft.subject);
    resize(720, 560);

    m_from = new QComboBox;
    const QJsonArray accounts = m_core->call("accounts.list").toArray();
    for (const QJsonValue &v : accounts) {
        const QJsonObject a = v.toObject();
        const QString name = a.value("name").toString();
        const QString email = a.value("email").toString();
        m_from->addItem(name.isEmpty() ? email : QStringLiteral("%1 <%2>").arg(name, email),
                        qint64(a.value("id").toDouble()));
    }
    const int idx = m_from->findData(draft.accountId);
    if (idx >= 0)
        m_from->setCurrentIndex(idx);

    m_to = new QLineEdit(draft.to);
    m_to->setPlaceholderText(QStringLiteral("name@example.com"));
    m_cc = new QLineEdit(draft.cc);
    m_subject = new QLineEdit(draft.subject);
    m_body = new QPlainTextEdit(draft.text);
    m_body->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));

    auto *form = new QFormLayout;
    form->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);
    form->addRow(tr("From"), m_from);
    form->addRow(tr("To"), m_to);
    form->addRow(tr("Cc"), m_cc);
    form->addRow(tr("Subject"), m_subject);

    m_status = new QLabel;
    m_status->setWordWrap(true);
    m_send = new QPushButton(tr("Send"));
    m_send->setDefault(true);
    auto *bottom = new QHBoxLayout;
    bottom->addWidget(m_status, 1);
    bottom->addWidget(m_send);

    auto *layout = new QVBoxLayout(this);
    layout->addLayout(form);
    layout->addWidget(m_body, 1);
    layout->addLayout(bottom);

    connect(m_send, &QPushButton::clicked, this, &ComposeWindow::send);
    connect(new QShortcut(QKeySequence(Qt::CTRL | Qt::Key_Return), this), &QShortcut::activated, this,
            &ComposeWindow::send);
    connect(m_subject, &QLineEdit::textChanged, this,
            [this](const QString &s) { setWindowTitle(s.isEmpty() ? tr("New message") : s); });
    connect(m_core, &Core::event, this, &ComposeWindow::onEvent);

    // Put the cursor where the user will type first.
    if (draft.to.isEmpty()) {
        m_to->setFocus();
    } else {
        m_body->setFocus();
        m_body->moveCursor(QTextCursor::Start);
    }
}

void ComposeWindow::setBusy(bool busy) {
    m_send->setEnabled(!busy);
    for (QWidget *w : {static_cast<QWidget *>(m_from), static_cast<QWidget *>(m_to), static_cast<QWidget *>(m_cc),
                       static_cast<QWidget *>(m_subject), static_cast<QWidget *>(m_body)})
        w->setEnabled(!busy);
}

void ComposeWindow::send() {
    if (!m_send->isEnabled())
        return;
    QJsonObject params{
        {"account_id", m_from->currentData().toLongLong()},
        {"to", m_to->text()},
        {"cc", m_cc->text()},
        {"subject", m_subject->text()},
        {"text", m_body->toPlainText()},
        {"token", m_token},
    };
    if (m_replyToId != 0)
        params.insert("reply_to_id", m_replyToId);

    QString error;
    m_core->call("send", params, &error);
    if (!error.isEmpty()) {
        m_status->setText(error);
        return;
    }
    setBusy(true);
    m_status->setText(tr("Sending…"));
}

void ComposeWindow::onEvent(const QJsonObject &event) {
    if (event.value("token").toString() != m_token)
        return;
    const QString type = event.value("type").toString();
    if (type == QLatin1String("send_done")) {
        close();
    } else if (type == QLatin1String("send_error")) {
        setBusy(false);
        m_status->setText(tr("Not sent. %1").arg(event.value("message").toString()));
    }
}
