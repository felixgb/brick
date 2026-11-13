#pragma once

#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>

class Core;
class QComboBox;
class QFormLayout;
class QLabel;
class QLineEdit;
class QPushButton;
class QSpinBox;

// Adds an account. Password providers are checked against the server before
// being saved; OAuth providers hand off to the system browser.
class AccountDialog : public QDialog {
    Q_OBJECT
public:
    explicit AccountDialog(Core *core, QWidget *parent = nullptr);

private slots:
    void providerChanged();
    void submit();
    void onEvent(const QJsonObject &event);

private:
    QJsonObject provider() const;
    void setRowVisible(QWidget *field, bool visible);
    void setBusy(bool busy, const QString &status = {});

    Core *m_core;
    QJsonArray m_providers;
    QFormLayout *m_form;
    QComboBox *m_provider;
    QLineEdit *m_name;
    QLineEdit *m_email;
    QLineEdit *m_password;
    QLineEdit *m_clientId;
    QLineEdit *m_clientSecret;
    QLineEdit *m_username;
    QLineEdit *m_imapHost;
    QSpinBox *m_imapPort;
    QLineEdit *m_smtpHost;
    QSpinBox *m_smtpPort;
    QComboBox *m_smtpSecurity;
    QLabel *m_hint;
    QLabel *m_status;
    QPushButton *m_submit;
    bool m_waiting = false;
};
