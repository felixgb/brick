#pragma once

#include <QDialog>

class Core;
class QComboBox;
class QPlainTextEdit;

// Per-account extras kept in the engine's settings: other addresses the user
// sends as, and the signature added to new messages.
class AccountOptionsDialog : public QDialog {
    Q_OBJECT
public:
    explicit AccountOptionsDialog(Core *core, QWidget *parent = nullptr);

private:
    void load();
    void store();

    Core *m_core;
    QComboBox *m_account;
    QPlainTextEdit *m_aliases;
    QPlainTextEdit *m_signature;
    qint64 m_shownAccount = 0;
};
