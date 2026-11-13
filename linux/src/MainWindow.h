#pragma once

#include <QJsonObject>
#include <QMainWindow>

#include "ComposeWindow.h"

class Core;
class MessageModel;
class MessageView;
class QAction;
class QLabel;
class QLineEdit;
class QTimer;
class QToolButton;
class QTreeView;
class QTreeWidget;
class QTreeWidgetItem;

class MainWindow : public QMainWindow {
    Q_OBJECT
public:
    explicit MainWindow(Core *core, QWidget *parent = nullptr);

protected:
    void closeEvent(QCloseEvent *event) override;

private slots:
    void onEvent(const QJsonObject &event);
    void folderSelected();
    void messageSelected();
    void runSearch();

private:
    enum class ReplyKind { Reply, ReplyAll, Forward };

    void buildActions();
    void buildUi();
    void reloadFolders();
    void refreshMessages();
    void showMessage(qint64 id, bool allowRemote);
    void clearReader();
    void updateActions();
    void compose(const ComposeWindow::Draft &draft);
    void reply(ReplyKind kind);
    void moveSelected(const QString &to);
    void setSelectedFlags(const QJsonObject &flags);
    QList<qint64> selectedIds() const;
    QJsonObject currentMessage() const;
    qint64 currentAccountId() const;
    void addAccount();
    void removeAccount();

    Core *m_core;
    MessageModel *m_model;
    QTreeWidget *m_folders;
    QTreeView *m_list;
    MessageView *m_view;
    QWidget *m_reader;
    QLabel *m_subject;
    QLabel *m_meta;
    QWidget *m_remoteBar;
    QLabel *m_placeholder;
    QLineEdit *m_search;
    QLabel *m_statusLabel;
    QTimer *m_refreshTimer;

    QAction *m_newAct;
    QAction *m_replyAct;
    QAction *m_replyAllAct;
    QAction *m_forwardAct;
    QAction *m_archiveAct;
    QAction *m_deleteAct;
    QAction *m_readAct;
    QAction *m_starAct;
    QAction *m_syncAct;

    qint64 m_shownId = 0;
    qint64 m_pendingBodyId = 0;
    bool m_shownAllowRemote = false;
    QString m_shownText;
    QString m_shownCc;
    int m_syncing = 0;
};
