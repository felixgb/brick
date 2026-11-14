#pragma once

#include <QJsonObject>
#include <QMainWindow>

#include "ComposeWindow.h"
#include "Item.h"

class CalendarView;
class Core;
class ItemStore;
class MessageModel;
class MessageView;
class QAction;
class QLabel;
class QLineEdit;
class QStackedWidget;
class QTabBar;
class QTimer;
class QListWidget;
class QToolBar;
class QToolButton;
class QTreeView;
class QTreeWidget;
class QTreeWidgetItem;
class TodoView;

class MainWindow : public QMainWindow {
    Q_OBJECT
public:
    explicit MainWindow(Core *core, QWidget *parent = nullptr);

    // For the debug server and tests.
    int view() const;

protected:
    void closeEvent(QCloseEvent *event) override;

private slots:
    void onEvent(const QJsonObject &event);
    void folderSelected();
    void messageSelected();
    void runSearch();
    void setView(int index);

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
    // The selection with every message of each selected conversation (in the
    // folder being shown) added, for actions that apply to whole conversations.
    QList<qint64> expandedIds() const;
    void showThread(qint64 id);
    QJsonObject currentMessage() const;
    qint64 currentAccountId() const;
    Q_INVOKABLE void addAccount();
    void removeAccount();
    void newItem(const QString &day = QString(), const QString &time = QString());
    Q_INVOKABLE void importCalendar();
    Q_INVOKABLE void exportCalendar();
    void addInvitation();
    void moveCurrent(int delta);
    Q_INVOKABLE void accountOptions();
    void editItem(const Item &item);
    void openSource(const Item &item);
    void extractItems();
    void showLinkedItems();
    Q_INVOKABLE void claudeSettings();

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
    QLabel *m_linked;
    QWidget *m_inviteBar;
    QListWidget *m_threadList;
    QJsonObject m_shown; // the message in the reader
    QAction *m_threadAct;
    QAction *m_nextAct;
    QAction *m_prevAct;

    ItemStore *m_items;
    QStackedWidget *m_stack;
    QTabBar *m_tabs;
    QToolBar *m_mailBar;
    TodoView *m_todo;
    CalendarView *m_calendar;
    QAction *m_extractAct;
    QAction *m_newItemAct;
    QString m_extractToken;
    qint64 m_extractingId = 0;

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
