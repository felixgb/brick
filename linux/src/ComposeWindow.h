#pragma once

#include <QJsonObject>
#include <QWidget>

class Core;
class QComboBox;
class QLabel;
class QLineEdit;
class QPlainTextEdit;
class QPushButton;

class ComposeWindow : public QWidget {
    Q_OBJECT
public:
    struct Draft {
        qint64 accountId = 0;
        QString to;
        QString cc;
        QString subject;
        QString text;
        qint64 replyToId = 0;
    };

    ComposeWindow(Core *core, const Draft &draft, QWidget *parent = nullptr);

private slots:
    void send();
    void onEvent(const QJsonObject &event);

private:
    void setBusy(bool busy);

    Core *m_core;
    QString m_token;
    qint64 m_replyToId;
    QComboBox *m_from;
    QLineEdit *m_to;
    QLineEdit *m_cc;
    QLineEdit *m_subject;
    QPlainTextEdit *m_body;
    QPushButton *m_send;
    QLabel *m_status;
};
