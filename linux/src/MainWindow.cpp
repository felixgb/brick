#include "MainWindow.h"

#include <QAction>
#include <QCloseEvent>
#include <QFile>
#include <QFileDialog>
#include <QDateTime>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QIcon>
#include <QItemSelectionModel>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QLocale>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QPushButton>
#include <QScrollBar>
#include <QListWidget>
#include <QSet>
#include <QSettings>
#include <QSplitter>
#include <QStackedWidget>
#include <QTabBar>
#include <QUuid>
#include <QStatusBar>
#include <QTimer>
#include <QToolBar>
#include <QTreeView>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>

#include "AccountDialog.h"
#include "AccountOptionsDialog.h"
#include "CalendarView.h"
#include "ClaudeDialogs.h"
#include "Core.h"
#include "ItemEditor.h"
#include "ItemStore.h"
#include "TodoView.h"
#include "MessageModel.h"
#include "MessageView.h"

namespace {

enum { FolderIdRole = Qt::UserRole, AccountIdRole, FolderRoleRole, UnifiedRole };

qint64 jsonId(const QJsonValue &v) {
    return qint64(v.toDouble());
}

QString senderLabel(const QJsonObject &m) {
    const QString name = m.value("from_name").toString();
    const QString addr = m.value("from_addr").toString();
    return name.isEmpty() ? addr : QStringLiteral("%1 <%2>").arg(name, addr);
}

QString withPrefix(const QString &prefix, const QString &subject) {
    return subject.startsWith(prefix, Qt::CaseInsensitive) ? subject : prefix + QLatin1Char(' ') + subject;
}

QString quoted(const QString &text) {
    QStringList lines = text.split(QLatin1Char('\n'));
    for (QString &line : lines)
        line = line.startsWith(QLatin1Char('>')) ? QLatin1Char('>') + line : QStringLiteral("> ") + line;
    return lines.join(QLatin1Char('\n'));
}

QIcon themed(const char *name) {
    return QIcon::fromTheme(QString::fromLatin1(name));
}

} // namespace

MainWindow::MainWindow(Core *core, QWidget *parent) : QMainWindow(parent), m_core(core) {
    setWindowTitle(QStringLiteral("Brick"));
    m_model = new MessageModel(core, this);
    m_items = new ItemStore(core, this);

    // Sync can report many changes in a burst; redraw the list once per burst.
    m_refreshTimer = new QTimer(this);
    m_refreshTimer->setSingleShot(true);
    m_refreshTimer->setInterval(60);
    connect(m_refreshTimer, &QTimer::timeout, this, &MainWindow::refreshMessages);

    buildActions();
    buildUi();
    connect(m_core, &Core::event, this, &MainWindow::onEvent);

    QSettings settings;
    restoreGeometry(settings.value("window/geometry").toByteArray());
    restoreState(settings.value("window/state").toByteArray());
    if (auto *split = findChild<QSplitter *>("mainSplit"))
        split->restoreState(settings.value("window/mainSplit").toByteArray());
    if (auto *split = findChild<QSplitter *>("readSplit"))
        split->restoreState(settings.value("window/readSplit").toByteArray());

    reloadFolders();
    updateActions();
    if (m_core->call("accounts.list").toArray().isEmpty() && qEnvironmentVariableIsEmpty("BRICK_DEBUG_SOCKET"))
        QTimer::singleShot(0, this, &MainWindow::addAccount);
}

void MainWindow::closeEvent(QCloseEvent *event) {
    QSettings settings;
    settings.setValue("window/geometry", saveGeometry());
    settings.setValue("window/state", saveState());
    if (auto *split = findChild<QSplitter *>("mainSplit"))
        settings.setValue("window/mainSplit", split->saveState());
    if (auto *split = findChild<QSplitter *>("readSplit"))
        settings.setValue("window/readSplit", split->saveState());
    QMainWindow::closeEvent(event);
}

void MainWindow::buildActions() {
    m_newAct = new QAction(themed("mail-message-new"), tr("New"), this);
    m_newAct->setShortcut(QKeySequence::New);
    connect(m_newAct, &QAction::triggered, this, [this] {
        ComposeWindow::Draft d;
        d.accountId = currentAccountId();
        compose(d);
    });

    m_replyAct = new QAction(themed("mail-reply-sender"), tr("Reply"), this);
    m_replyAct->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_R));
    connect(m_replyAct, &QAction::triggered, this, [this] { reply(ReplyKind::Reply); });

    m_replyAllAct = new QAction(themed("mail-reply-all"), tr("Reply all"), this);
    m_replyAllAct->setShortcut(QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_R));
    connect(m_replyAllAct, &QAction::triggered, this, [this] { reply(ReplyKind::ReplyAll); });

    m_forwardAct = new QAction(themed("mail-forward"), tr("Forward"), this);
    m_forwardAct->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_L));
    connect(m_forwardAct, &QAction::triggered, this, [this] { reply(ReplyKind::Forward); });

    m_archiveAct = new QAction(themed("mail-archive"), tr("Archive"), this);
    m_archiveAct->setShortcut(QKeySequence(Qt::Key_A));
    connect(m_archiveAct, &QAction::triggered, this, [this] { moveSelected(QStringLiteral("archive")); });

    m_deleteAct = new QAction(themed("edit-delete"), tr("Delete"), this);
    m_deleteAct->setShortcut(QKeySequence::Delete);
    connect(m_deleteAct, &QAction::triggered, this, [this] { moveSelected(QStringLiteral("trash")); });

    m_readAct = new QAction(themed("mail-mark-unread"), tr("Mark unread"), this);
    m_readAct->setShortcut(QKeySequence(Qt::Key_M));
    connect(m_readAct, &QAction::triggered, this, [this] {
        const bool makeRead = !currentMessage().value("seen").toBool();
        setSelectedFlags({{"seen", makeRead}});
    });

    m_starAct = new QAction(themed("starred"), tr("Star"), this);
    m_starAct->setShortcut(QKeySequence(Qt::Key_S));
    connect(m_starAct, &QAction::triggered, this, [this] {
        const bool makeStarred = !currentMessage().value("flagged").toBool();
        setSelectedFlags({{"flagged", makeStarred}});
    });

    m_syncAct = new QAction(themed("view-refresh"), tr("Check mail"), this);
    m_syncAct->setShortcut(QKeySequence::Refresh);
    connect(m_syncAct, &QAction::triggered, this, [this] { m_core->call("sync.now"); });

    m_nextAct = new QAction(tr("Next message"), this);
    m_nextAct->setShortcut(QKeySequence(Qt::Key_J));
    connect(m_nextAct, &QAction::triggered, this, [this] { moveCurrent(1); });
    m_prevAct = new QAction(tr("Previous message"), this);
    m_prevAct->setShortcut(QKeySequence(Qt::Key_K));
    connect(m_prevAct, &QAction::triggered, this, [this] { moveCurrent(-1); });

    m_extractAct = new QAction(themed("view-task"), tr("Add to To-do"), this);
    m_extractAct->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_T));
    m_extractAct->setToolTip(tr("Ask Claude to find to-dos and events in this email"));
    connect(m_extractAct, &QAction::triggered, this, &MainWindow::extractItems);

    m_newItemAct = new QAction(themed("list-add"), tr("New item…"), this);
    m_newItemAct->setShortcut(QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_N));
    connect(m_newItemAct, &QAction::triggered, this,
            [this] { newItem(m_stack->currentIndex() == 2 ? m_calendar->selectedDay() : QString()); });
    addAction(m_newItemAct);
}

void MainWindow::buildUi() {
    // Menus
    QMenu *file = menuBar()->addMenu(tr("&File"));
    file->addAction(m_newAct);
    file->addAction(m_syncAct);
    file->addSeparator();
    file->addAction(m_newItemAct);
    file->addAction(tr("Import calendar…"), this, &MainWindow::importCalendar);
    file->addAction(tr("Export calendar…"), this, &MainWindow::exportCalendar);
    file->addSeparator();
    file->addAction(tr("Add account…"), this, &MainWindow::addAccount);
    file->addAction(tr("Remove account…"), this, &MainWindow::removeAccount);
    file->addAction(tr("Signatures and addresses…"), this, &MainWindow::accountOptions);
    file->addAction(tr("Claude settings…"), this, &MainWindow::claudeSettings);
    file->addSeparator();
    QAction *quit = file->addAction(tr("Quit"), this, &QWidget::close);
    quit->setShortcut(QKeySequence::Quit);

    QMenu *message = menuBar()->addMenu(tr("&Message"));
    message->addActions({m_replyAct, m_replyAllAct, m_forwardAct});
    message->addSeparator();
    message->addActions({m_nextAct, m_prevAct});
    message->addSeparator();
    message->addActions({m_archiveAct, m_deleteAct});
    message->addSeparator();
    message->addActions({m_readAct, m_starAct});
    message->addSeparator();
    message->addAction(m_extractAct);

    QMenu *viewMenu = menuBar()->addMenu(tr("&View"));
    const QStringList viewNames{tr("Mail"), tr("To-do"), tr("Calendar")};
    for (int i = 0; i < viewNames.size(); ++i) {
        QAction *a = viewMenu->addAction(viewNames.at(i), this, [this, i] { m_tabs->setCurrentIndex(i); });
        a->setShortcut(QKeySequence(Qt::CTRL | (Qt::Key_1 + i)));
    }

    viewMenu->addSeparator();
    m_threadAct = viewMenu->addAction(tr("Group into &conversations"));
    m_threadAct->setObjectName("threadAction");
    m_threadAct->setCheckable(true);
    m_threadAct->setChecked(QSettings().value("mail/threaded", true).toBool());
    m_model->setThreaded(m_threadAct->isChecked());
    connect(m_threadAct, &QAction::toggled, this, [this](bool on) {
        QSettings().setValue("mail/threaded", on);
        m_model->setThreaded(on);
        clearReader();
        updateActions();
    });

    // View switcher
    QToolBar *viewBar = addToolBar(tr("View"));
    viewBar->setObjectName("viewToolBar");
    viewBar->setMovable(false);
    m_tabs = new QTabBar;
    m_tabs->setObjectName("viewTabs");
    m_tabs->setDrawBase(false);
    m_tabs->setExpanding(false);
    m_tabs->setFocusPolicy(Qt::NoFocus);
    for (const QString &name : viewNames)
        m_tabs->addTab(name);
    viewBar->addWidget(m_tabs);

    // Mail toolbar
    QToolBar *bar = addToolBar(tr("Main"));
    m_mailBar = bar;
    bar->setObjectName("mainToolBar");
    bar->setMovable(false);
    bar->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    bar->addAction(m_newAct);
    bar->addAction(m_syncAct);
    bar->addSeparator();
    bar->addActions({m_replyAct, m_replyAllAct, m_forwardAct});
    bar->addSeparator();
    bar->addActions({m_archiveAct, m_deleteAct});
    bar->addSeparator();
    bar->addAction(m_extractAct);

    auto *spacer = new QWidget;
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    bar->addWidget(spacer);
    m_search = new QLineEdit;
    m_search->setPlaceholderText(tr("Search all mail"));
    m_search->setClearButtonEnabled(true);
    m_search->setMaximumWidth(280);
    bar->addWidget(m_search);
    auto *focusSearch = new QAction(this);
    focusSearch->setShortcut(QKeySequence::Find);
    connect(focusSearch, &QAction::triggered, m_search, qOverload<>(&QWidget::setFocus));
    addAction(focusSearch);

    // Search as you type, but not on every keystroke.
    auto *searchDelay = new QTimer(this);
    searchDelay->setSingleShot(true);
    searchDelay->setInterval(150);
    connect(m_search, &QLineEdit::textChanged, searchDelay, qOverload<>(&QTimer::start));
    connect(searchDelay, &QTimer::timeout, this, &MainWindow::runSearch);

    // Folder tree
    m_folders = new QTreeWidget;
    m_folders->setObjectName("folderTree");
    m_folders->setHeaderHidden(true);
    m_folders->setRootIsDecorated(true);
    m_folders->setIndentation(14);
    m_folders->setMinimumWidth(160);
    connect(m_folders, &QTreeWidget::itemSelectionChanged, this, &MainWindow::folderSelected);

    // Message list
    m_list = new QTreeView;
    m_list->setObjectName("messageList");
    m_list->setModel(m_model);
    m_list->setRootIsDecorated(false);
    m_list->setUniformRowHeights(true); // lets the view skip measuring every row
    m_list->setAlternatingRowColors(true);
    m_list->setSelectionMode(QAbstractItemView::ExtendedSelection);
    m_list->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_list->setAllColumnsShowFocus(true);
    m_list->setTextElideMode(Qt::ElideRight);
    QHeaderView *header = m_list->header();
    header->setStretchLastSection(false);
    header->setSectionResizeMode(MessageModel::Star, QHeaderView::Fixed);
    header->setSectionResizeMode(MessageModel::From, QHeaderView::Interactive);
    header->setSectionResizeMode(MessageModel::Subject, QHeaderView::Stretch);
    header->setSectionResizeMode(MessageModel::Date, QHeaderView::Fixed);
    header->resizeSection(MessageModel::Star, 28);
    header->resizeSection(MessageModel::From, 200);
    header->resizeSection(MessageModel::Date, 96);
    connect(m_list->selectionModel(), &QItemSelectionModel::selectionChanged, this, &MainWindow::messageSelected);
    m_list->addActions({m_replyAct, m_replyAllAct, m_forwardAct, m_archiveAct, m_deleteAct, m_readAct, m_starAct});
    m_list->setContextMenuPolicy(Qt::ActionsContextMenu);

    // Reader
    m_subject = new QLabel;
    QFont big = m_subject->font();
    big.setPointSizeF(big.pointSizeF() * 1.25);
    big.setBold(true);
    m_subject->setFont(big);
    m_subject->setWordWrap(true);
    m_subject->setTextFormat(Qt::PlainText);
    m_subject->setTextInteractionFlags(Qt::TextSelectableByMouse);
    m_meta = new QLabel;
    m_meta->setWordWrap(true);
    m_meta->setTextFormat(Qt::PlainText);
    m_meta->setTextInteractionFlags(Qt::TextSelectableByMouse);

    m_remoteBar = new QWidget;
    auto *remoteLayout = new QHBoxLayout(m_remoteBar);
    remoteLayout->setContentsMargins(0, 0, 0, 0);
    remoteLayout->addWidget(new QLabel(tr("Remote images are hidden to protect your privacy.")), 1);
    auto *showImages = new QPushButton(tr("Show images"));
    remoteLayout->addWidget(showImages);
    connect(showImages, &QPushButton::clicked, this, [this] {
        if (m_shownId != 0)
            showMessage(m_shownId, true);
    });

    m_view = new MessageView;
    m_placeholder = new QLabel(tr("Select a message to read it"));
    m_placeholder->setAlignment(Qt::AlignCenter);
    m_placeholder->setEnabled(false);

    auto *headerBox = new QWidget;
    auto *headerLayout = new QVBoxLayout(headerBox);
    headerLayout->setContentsMargins(12, 10, 12, 8);
    headerLayout->setSpacing(4);
    headerLayout->addWidget(m_subject);
    headerLayout->addWidget(m_meta);
    headerLayout->addWidget(m_remoteBar);
    m_inviteBar = new QWidget;
    m_inviteBar->setObjectName("inviteBar");
    auto *inviteLayout = new QHBoxLayout(m_inviteBar);
    inviteLayout->setContentsMargins(0, 0, 0, 0);
    inviteLayout->addWidget(new QLabel(tr("This email has a calendar invitation.")), 1);
    auto *addInvite = new QPushButton(tr("Add to calendar"));
    addInvite->setObjectName("addInvitation");
    inviteLayout->addWidget(addInvite);
    connect(addInvite, &QPushButton::clicked, this, &MainWindow::addInvitation);
    m_inviteBar->hide();
    headerLayout->addWidget(m_inviteBar);

    m_threadList = new QListWidget;
    m_threadList->setObjectName("threadList");
    m_threadList->setSelectionMode(QAbstractItemView::SingleSelection);
    m_threadList->setTextElideMode(Qt::ElideRight);
    m_threadList->setFrameShape(QFrame::NoFrame);
    m_threadList->hide();
    headerLayout->addWidget(m_threadList);
    connect(m_threadList, &QListWidget::itemClicked, this, [this](QListWidgetItem *item) {
        const qint64 id = item->data(Qt::UserRole).toLongLong();
        if (id == 0 || id == m_shownId)
            return;
        const bool wasRead = item->data(Qt::UserRole + 1).toBool();
        showMessage(id, false);
        if (!wasRead)
            m_core->call("messages.set_flags", {{"ids", QJsonArray{double(id)}}, {"seen", true}});
    });

    m_linked = new QLabel;
    m_linked->setObjectName("linkedItems");
    m_linked->setWordWrap(true);
    m_linked->setTextFormat(Qt::PlainText);
    m_linked->setEnabled(false);
    m_linked->hide();
    headerLayout->addWidget(m_linked);

    m_reader = new QWidget;
    auto *readerLayout = new QVBoxLayout(m_reader);
    readerLayout->setContentsMargins(0, 0, 0, 0);
    readerLayout->setSpacing(0);
    readerLayout->addWidget(headerBox);
    readerLayout->addWidget(m_view, 1);
    readerLayout->addWidget(m_placeholder, 1);

    auto *readSplit = new QSplitter(Qt::Vertical);
    readSplit->setObjectName("readSplit");
    readSplit->addWidget(m_list);
    readSplit->addWidget(m_reader);
    readSplit->setStretchFactor(0, 2);
    readSplit->setStretchFactor(1, 3);

    auto *mainSplit = new QSplitter(Qt::Horizontal);
    mainSplit->setObjectName("mainSplit");
    mainSplit->addWidget(m_folders);
    mainSplit->addWidget(readSplit);
    mainSplit->setStretchFactor(0, 0);
    mainSplit->setStretchFactor(1, 1);
    mainSplit->setSizes({220, 900});

    m_todo = new TodoView(m_items);
    m_calendar = new CalendarView(m_items);
    m_stack = new QStackedWidget;
    m_stack->setObjectName("viewStack");
    m_stack->addWidget(mainSplit);
    m_stack->addWidget(m_todo);
    m_stack->addWidget(m_calendar);
    setCentralWidget(m_stack);

    connect(m_tabs, &QTabBar::currentChanged, this, &MainWindow::setView);
    connect(m_todo, &TodoView::newItemRequested, this, [this] { newItem(); });
    connect(m_todo, &TodoView::editRequested, this, &MainWindow::editItem);
    connect(m_todo, &TodoView::openEmailRequested, this, &MainWindow::openSource);
    connect(m_calendar, &CalendarView::newItemRequested, this, &MainWindow::newItem);
    connect(m_calendar, &CalendarView::editRequested, this, &MainWindow::editItem);
    connect(m_calendar, &CalendarView::openEmailRequested, this, &MainWindow::openSource);
    connect(m_items, &ItemStore::changed, this, &MainWindow::showLinkedItems);
    connect(m_items, &ItemStore::problem, this, [this](const QString &m) { statusBar()->showMessage(m, 8000); });

    m_statusLabel = new QLabel;
    statusBar()->addWidget(m_statusLabel, 1);
    resize(1180, 760);
    clearReader();
}

// ---- folders ---------------------------------------------------------------

void MainWindow::reloadFolders() {
    const qint64 selected = m_model->isSearch() || m_model->isUnified() ? 0 : m_model->folderId();
    QSignalBlocker block(m_folders);
    m_folders->clear();

    QTreeWidgetItem *toSelect = nullptr;
    QTreeWidgetItem *firstInbox = nullptr;
    const QJsonArray accounts = m_core->call("accounts.list").toArray();

    // With more than one account, one entry for all their inboxes.
    QTreeWidgetItem *unified = nullptr;
    if (accounts.size() > 1) {
        int unread = 0;
        for (const QJsonValue &av : accounts) {
            const QJsonArray fs = m_core->call("folders.list", {{"account_id", av.toObject().value("id")}}).toArray();
            for (const QJsonValue &fv : fs) {
                if (fv.toObject().value("role").toString() == QLatin1String("inbox"))
                    unread += fv.toObject().value("unread").toInt();
            }
        }
        unified = new QTreeWidgetItem(m_folders, {unread > 0 ? tr("All inboxes (%1)").arg(unread) : tr("All inboxes")});
        unified->setData(0, UnifiedRole, QStringLiteral("inbox"));
        QFont f = unified->font(0);
        f.setBold(true);
        unified->setFont(0, f);
        if (m_model->isUnified())
            toSelect = unified;
    }
    for (const QJsonValue &av : accounts) {
        const QJsonObject account = av.toObject();
        const qint64 accountId = jsonId(account.value("id"));
        auto *top = new QTreeWidgetItem(m_folders, {account.value("email").toString()});
        top->setData(0, AccountIdRole, accountId);
        top->setFlags(Qt::ItemIsEnabled);
        QFont f = top->font(0);
        f.setBold(true);
        top->setFont(0, f);

        const QJsonArray folders = m_core->call("folders.list", {{"account_id", accountId}}).toArray();
        for (const QJsonValue &fv : folders) {
            const QJsonObject folder = fv.toObject();
            if (!folder.value("selectable").toBool())
                continue;
            const int unread = folder.value("unread").toInt();
            QString label = folder.value("name").toString();
            if (unread > 0)
                label += QStringLiteral(" (%1)").arg(unread);
            auto *item = new QTreeWidgetItem(top, {label});
            item->setData(0, FolderIdRole, jsonId(folder.value("id")));
            item->setData(0, AccountIdRole, accountId);
            item->setData(0, FolderRoleRole, folder.value("role").toString());
            item->setToolTip(0, folder.value("path").toString());
            if (unread > 0) {
                QFont bf = item->font(0);
                bf.setBold(true);
                item->setFont(0, bf);
            }
            if (jsonId(folder.value("id")) == selected)
                toSelect = item;
            if (!firstInbox && folder.value("role").toString() == QLatin1String("inbox"))
                firstInbox = item;
        }
        top->setExpanded(true);
    }

    if (toSelect) {
        m_folders->setCurrentItem(toSelect);
    } else if (!m_model->isSearch() && !m_model->isUnified() && firstInbox) {
        // Nothing open yet (first run, or the open folder went away).
        block.unblock();
        m_folders->setCurrentItem(firstInbox);
    }
}

void MainWindow::folderSelected() {
    QTreeWidgetItem *item = m_folders->currentItem();
    if (!item || (!item->data(0, FolderIdRole).isValid() && !item->data(0, UnifiedRole).isValid()))
        return;
    if (!m_search->text().isEmpty()) {
        QSignalBlocker block(m_search);
        m_search->clear();
    }
    if (item->data(0, UnifiedRole).isValid())
        m_model->showRole(item->data(0, UnifiedRole).toString());
    else
        m_model->showFolder(item->data(0, FolderIdRole).toLongLong());
    clearReader();
    updateActions();
}

qint64 MainWindow::currentAccountId() const {
    const QJsonObject m = currentMessage();
    if (!m.isEmpty())
        return jsonId(m.value("account_id"));
    if (QTreeWidgetItem *item = m_folders->currentItem())
        return item->data(0, AccountIdRole).toLongLong();
    return 0;
}

// ---- messages --------------------------------------------------------------

void MainWindow::runSearch() {
    const QString query = m_search->text().trimmed();
    if (query.isEmpty()) {
        folderSelected();
        return;
    }
    m_model->showSearch(query);
    clearReader();
    updateActions();
}

QList<qint64> MainWindow::selectedIds() const {
    QList<qint64> ids;
    const QModelIndexList rows = m_list->selectionModel()->selectedRows();
    for (const QModelIndex &index : rows)
        ids.append(jsonId(m_model->message(index.row()).value("id")));
    return ids;
}

QList<qint64> MainWindow::expandedIds() const {
    QList<qint64> ids;
    QSet<qint64> known;
    auto add = [&](qint64 id) {
        if (!known.contains(id)) {
            known.insert(id);
            ids.append(id);
        }
    };
    for (const QModelIndex &index : m_list->selectionModel()->selectedRows()) {
        const QJsonObject m = m_model->message(index.row());
        const qint64 id = jsonId(m.value("id"));
        add(id);
        if (!m_model->threaded() || m.value("thread_count").toInt() < 2)
            continue;
        QJsonObject p{{"id", double(id)}};
        if (m_model->isUnified())
            p.insert("role", m_model->role());
        else
            p.insert("folder_id", m.value("folder_id"));
        for (const QJsonValue &row : m_core->call("messages.thread", p).toObject().value("rows").toArray())
            add(jsonId(row.toObject().value("id")));
    }
    return ids;
}

static bool isUnread(const QJsonObject &m) {
    return m.contains("thread_unread") ? m.value("thread_unread").toInt() > 0 : !m.value("seen").toBool();
}

QJsonObject MainWindow::currentMessage() const {
    const QModelIndexList rows = m_list->selectionModel()->selectedRows();
    return rows.size() == 1 ? m_model->message(rows.first().row()) : QJsonObject();
}

void MainWindow::refreshMessages() {
    // Rows shift when mail arrives or leaves; re-find the selection by id.
    const QList<qint64> ids = selectedIds();
    const int scroll = m_list->verticalScrollBar()->value();
    {
        QSignalBlocker block(m_list->selectionModel());
        m_model->refresh();
        bool first = true;
        for (qint64 id : ids) {
            const int row = m_model->rowOfMessage(id);
            if (row < 0)
                continue;
            const QModelIndex index = m_model->index(row, 0);
            m_list->selectionModel()->select(index, QItemSelectionModel::Select | QItemSelectionModel::Rows);
            if (first)
                m_list->selectionModel()->setCurrentIndex(index, QItemSelectionModel::NoUpdate);
            first = false;
        }
    }
    m_list->verticalScrollBar()->setValue(scroll);
    if (m_shownId != 0 && m_model->rowOfMessage(m_shownId) < 0 && selectedIds().isEmpty())
        clearReader();
    else if (m_shownId != 0)
        showThread(m_shownId);
    updateActions();
}

void MainWindow::messageSelected() {
    updateActions();
    const QJsonObject m = currentMessage();
    if (m.isEmpty()) {
        if (selectedIds().isEmpty())
            clearReader();
        return;
    }
    const qint64 id = jsonId(m.value("id"));
    if (id == m_shownId)
        return;
    showMessage(id, false);
    if (isUnread(m)) {
        // Opening a conversation reads all of it.
        QJsonArray ids;
        for (qint64 i : expandedIds())
            ids.append(double(i));
        m_core->call("messages.set_flags", {{"ids", ids}, {"seen", true}});
    }
}

void MainWindow::showMessage(qint64 id, bool allowRemote) {
    QString error;
    const QJsonObject r = m_core->call("messages.get", {{"id", id}, {"allow_remote", allowRemote}}, &error).toObject();
    if (!error.isEmpty()) {
        clearReader();
        return;
    }
    const QJsonObject m = r.value("message").toObject();
    m_shownId = id;
    m_shownAllowRemote = allowRemote;
    m_shownCc = r.value("cc").toString();
    m_shown = m;
    showLinkedItems();
    showThread(id);

    const QString subject = m.value("subject").toString();
    m_subject->setText(subject.isEmpty() ? tr("(no subject)") : subject);
    const QDateTime when = QDateTime::fromSecsSinceEpoch(qint64(m.value("date").toDouble()));
    QString meta = tr("From: %1\nTo: %2").arg(senderLabel(m), m.value("to").toString());
    if (!m_shownCc.isEmpty())
        meta += tr("\nCc: %1").arg(m_shownCc);
    meta += QLatin1Char('\n') + QLocale().toString(when, QLocale::LongFormat);

    m_subject->show();
    m_meta->show();
    m_placeholder->hide();
    m_view->show();

    if (r.value("body").isNull()) {
        // Not downloaded yet: the engine is fetching it and will send body_ready.
        m_pendingBodyId = id;
        m_shownText.clear();
        m_remoteBar->hide();
        m_inviteBar->hide();
        m_meta->setText(meta);
        m_view->showHtml(QStringLiteral("<body style=\"font:14px sans-serif;color:#777;margin:16px\">%1</body>")
                             .arg(tr("Loading…")),
                         false);
        return;
    }
    m_pendingBodyId = 0;
    const QJsonObject body = r.value("body").toObject();
    m_shownText = body.value("text").toString();

    const QJsonArray attachments = body.value("attachments").toArray();
    if (!attachments.isEmpty()) {
        QStringList names;
        for (const QJsonValue &a : attachments)
            names << a.toObject().value("name").toString();
        meta += tr("\nAttachments: %1").arg(names.join(QStringLiteral(", ")));
    }
    m_meta->setText(meta);
    m_remoteBar->setVisible(body.value("blocked_remote").toBool());
    m_inviteBar->setVisible(body.value("invitation").toBool());
    m_view->showHtml(body.value("html").toString(), allowRemote);
}

void MainWindow::showThread(qint64 id) {
    const QJsonArray rows = m_core->call("messages.thread", {{"id", double(id)}}).toObject().value("rows").toArray();
    QSignalBlocker block(m_threadList);
    m_threadList->clear();
    if (rows.size() < 2) {
        m_threadList->hide();
        return;
    }
    for (const QJsonValue &v : rows) {
        const QJsonObject m = v.toObject();
        const qint64 mid = jsonId(m.value("id"));
        const QDateTime when = QDateTime::fromSecsSinceEpoch(qint64(m.value("date").toDouble()));
        auto *item = new QListWidgetItem(QStringLiteral("%1    %2    %3")
                                             .arg(QLocale().toString(when, QStringLiteral("d MMM HH:mm")),
                                                  senderLabel(m), m.value("snippet").toString()));
        item->setData(Qt::UserRole, double(mid));
        item->setData(Qt::UserRole + 1, m.value("seen").toBool());
        QFont f = item->font();
        f.setBold(!m.value("seen").toBool());
        item->setFont(f);
        m_threadList->addItem(item);
        if (mid == id)
            m_threadList->setCurrentItem(item);
    }
    m_threadList->setFixedHeight(qMin(5, int(rows.size())) * m_threadList->sizeHintForRow(0) + 6);
    m_threadList->show();
}

void MainWindow::clearReader() {
    m_shownId = 0;
    m_shown = QJsonObject();
    m_threadList->hide();
    m_pendingBodyId = 0;
    m_shownText.clear();
    m_shownCc.clear();
    m_inviteBar->hide();
    m_linked->hide();
    m_subject->hide();
    m_meta->hide();
    m_remoteBar->hide();
    m_view->clear();
    m_view->hide();
    m_placeholder->show();
}

void MainWindow::updateActions() {
    // Mail shortcuts (A, M, S, J, K ...) must not act on a hidden mail list.
    if (m_stack->currentIndex() != 0) {
        for (QAction *a : {m_replyAct, m_replyAllAct, m_forwardAct, m_archiveAct, m_deleteAct, m_readAct, m_starAct,
                           m_extractAct, m_nextAct, m_prevAct})
            a->setEnabled(false);
        return;
    }
    m_nextAct->setEnabled(true);
    m_prevAct->setEnabled(true);
    const QList<qint64> ids = selectedIds();
    const QJsonObject m = currentMessage();
    const bool one = !m.isEmpty();
    const bool any = !ids.isEmpty();
    m_replyAct->setEnabled(one);
    m_replyAllAct->setEnabled(one);
    m_forwardAct->setEnabled(one);
    m_archiveAct->setEnabled(any);
    m_deleteAct->setEnabled(any);
    m_readAct->setEnabled(any);
    m_starAct->setEnabled(any);
    m_extractAct->setEnabled(one && m_extractingId == 0);
    m_readAct->setText(one && isUnread(m) ? tr("Mark read") : tr("Mark unread"));
    m_starAct->setText(one && m.value("flagged").toBool() ? tr("Remove star") : tr("Star"));
    m_newAct->setEnabled(!m_core->call("accounts.list").toArray().isEmpty());

    QString status;
    if (m_syncing > 0)
        status = tr("Checking mail…");
    else if (m_model->isSearch())
        status = tr("%n result(s)", nullptr, m_model->rowCount());
    else
        status = m_model->threaded() ? tr("%n conversation(s)", nullptr, m_model->rowCount())
                                     : tr("%n message(s)", nullptr, m_model->rowCount());
    m_statusLabel->setText(status);
}

void MainWindow::setSelectedFlags(const QJsonObject &flags) {
    QJsonArray ids;
    for (qint64 id : expandedIds())
        ids.append(double(id));
    if (ids.isEmpty())
        return;
    QJsonObject params = flags;
    params.insert("ids", ids);
    m_core->call("messages.set_flags", params);
}

void MainWindow::moveSelected(const QString &to) {
    const QModelIndexList rows = m_list->selectionModel()->selectedRows();
    if (rows.isEmpty())
        return;
    QJsonArray ids;
    for (qint64 id : expandedIds())
        ids.append(double(id));
    int lowest = rows.first().row();
    for (const QModelIndex &index : rows)
        lowest = qMin(lowest, index.row());
    QString error;
    m_core->call("messages.move", {{"ids", ids}, {"to", to}}, &error);
    if (!error.isEmpty()) {
        statusBar()->showMessage(error, 6000);
        return;
    }
    // The engine has already removed the rows locally. Redraw now and move the
    // selection to whatever slid into place, so triage is one key per message.
    m_refreshTimer->stop();
    const int scroll = m_list->verticalScrollBar()->value();
    clearReader();
    m_model->refresh();
    m_list->verticalScrollBar()->setValue(scroll);
    if (m_model->rowCount() > 0) {
        const int row = qMin(lowest, m_model->rowCount() - 1);
        m_list->setCurrentIndex(m_model->index(row, 0));
    }
    updateActions();
}

// ---- composing -------------------------------------------------------------

void MainWindow::compose(const ComposeWindow::Draft &draft) {
    auto *window = new ComposeWindow(m_core, draft);
    window->show();
}

void MainWindow::reply(ReplyKind kind) {
    // The message in the reader, which in a conversation may not be the one the list row shows.
    const QJsonObject m = !m_shown.isEmpty() && !currentMessage().isEmpty() ? m_shown : currentMessage();
    if (m.isEmpty())
        return;
    const QString subject = m.value("subject").toString();
    const QDateTime when = QDateTime::fromSecsSinceEpoch(qint64(m.value("date").toDouble()));
    const QString date = QLocale().toString(when, QLocale::LongFormat);

    ComposeWindow::Draft d;
    d.accountId = jsonId(m.value("account_id"));

    if (kind == ReplyKind::Forward) {
        d.subject = withPrefix(QStringLiteral("Fwd:"), subject);
        d.text = QStringLiteral("\n\n-------- Forwarded message --------\nFrom: %1\nDate: %2\nSubject: %3\nTo: %4\n\n%5")
                     .arg(senderLabel(m), date, subject, m.value("to").toString(), m_shownText);
        compose(d);
        return;
    }

    d.replyToId = jsonId(m.value("id"));
    d.to = senderLabel(m);
    d.subject = withPrefix(QStringLiteral("Re:"), subject);
    d.text = QStringLiteral("\n\nOn %1, %2 wrote:\n%3").arg(date, senderLabel(m), quoted(m_shownText));

    if (kind == ReplyKind::ReplyAll) {
        // Everyone on To and Cc except ourselves.
        QString mine;
        const QJsonArray accounts = m_core->call("accounts.list").toArray();
        for (const QJsonValue &a : accounts) {
            if (jsonId(a.toObject().value("id")) == d.accountId)
                mine = a.toObject().value("email").toString();
        }
        QStringList others;
        const QString all = m.value("to").toString() + QLatin1Char(',') + m_shownCc;
        for (const QString &part : all.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
            const QString addr = part.trimmed();
            if (!addr.isEmpty() && !addr.contains(mine, Qt::CaseInsensitive))
                others << addr;
        }
        d.cc = others.join(QStringLiteral(", "));
    }
    compose(d);
}

// ---- accounts --------------------------------------------------------------

void MainWindow::addAccount() {
    AccountDialog dialog(m_core, this);
    dialog.exec();
}

void MainWindow::removeAccount() {
    const qint64 accountId = currentAccountId();
    if (accountId == 0)
        return;
    QString email;
    const QJsonArray accounts = m_core->call("accounts.list").toArray();
    for (const QJsonValue &a : accounts) {
        if (jsonId(a.toObject().value("id")) == accountId)
            email = a.toObject().value("email").toString();
    }
    const auto answer = QMessageBox::question(
        this, tr("Remove account"),
        tr("Remove %1 from Brick?\n\nMail stays on the server; only the local copy and saved sign-in are deleted.")
            .arg(email),
        QMessageBox::Cancel | QMessageBox::Yes, QMessageBox::Cancel);
    if (answer != QMessageBox::Yes)
        return;
    m_model->showFolder(0);
    clearReader();
    m_core->call("accounts.remove", {{"id", accountId}});
}

// ---- engine events ---------------------------------------------------------

void MainWindow::onEvent(const QJsonObject &event) {
    const QString type = event.value("type").toString();

    if (type == QLatin1String("messages_changed")) {
        if (m_model->isSearch() || m_model->isUnified() || jsonId(event.value("folder_id")) == m_model->folderId())
            m_refreshTimer->start();
    } else if (type == QLatin1String("folders_changed") || type == QLatin1String("accounts_changed")) {
        reloadFolders();
        updateActions();
    } else if (type == QLatin1String("body_ready")) {
        if (jsonId(event.value("id")) == m_pendingBodyId)
            showMessage(m_pendingBodyId, m_shownAllowRemote);
        if (m_model->isUnified() || jsonId(event.value("folder_id")) == m_model->folderId())
            m_refreshTimer->start(); // the row gained its preview text
    } else if (type == QLatin1String("body_error")) {
        if (jsonId(event.value("id")) == m_pendingBodyId) {
            m_pendingBodyId = 0;
            m_view->showHtml(QStringLiteral("<body style=\"font:14px sans-serif;color:#777;margin:16px\">%1</body>")
                                 .arg(tr("This message couldn't be downloaded. %1")
                                          .arg(event.value("message").toString().toHtmlEscaped())),
                             false);
        }
    } else if (type == QLatin1String("sync_started")) {
        ++m_syncing;
        updateActions();
    } else if (type == QLatin1String("sync_finished")) {
        m_syncing = qMax(0, m_syncing - 1);
        updateActions();
    } else if (type == QLatin1String("items_extracted")) {
        if (event.value("token").toString() != m_extractToken)
            return;
        m_extractingId = 0;
        statusBar()->clearMessage();
        updateActions();
        QList<Item> found;
        for (const QJsonValue &v : event.value("items").toArray())
            found.append(Item::fromJson(v.toObject()));
        const QString subject = found.isEmpty() ? m_subject->text() : found.first().sourceSubject;
        ExtractionDialog dialog(m_items, subject, event.value("summary").toString(), event.value("truncated").toBool(),
                                found, this);
        dialog.exec();
    } else if (type == QLatin1String("items_extract_error")) {
        if (event.value("token").toString() != m_extractToken)
            return;
        m_extractingId = 0;
        updateActions();
        statusBar()->showMessage(event.value("message").toString(), 10000);
    } else if (type == QLatin1String("error")) {
        statusBar()->showMessage(event.value("message").toString(), 8000);
    }
}

// ---- to-do and calendar ----------------------------------------------------

int MainWindow::view() const {
    return m_stack->currentIndex();
}

void MainWindow::setView(int index) {
    m_stack->setCurrentIndex(index);
    m_mailBar->setVisible(index == 0);
    updateActions();
    if (index == 0) {
        m_list->setFocus();
    } else {
        m_statusLabel->clear();
        (index == 1 ? static_cast<QWidget *>(m_todo) : static_cast<QWidget *>(m_calendar->grid()))->setFocus();
    }
}

void MainWindow::newItem(const QString &day, const QString &time) {
    Item item;
    item.date = day;
    item.startTime = day.isEmpty() ? QString() : time;
    editItem(item);
}

void MainWindow::editItem(const Item &item) {
    ItemEditor dialog(m_items, item, this);
    connect(&dialog, &ItemEditor::openEmailRequested, this, &MainWindow::openSource);
    dialog.exec();
}

// Shows the email an item was made from.
void MainWindow::openSource(const Item &item) {
    const QJsonObject source = m_core->call("items.source", {{"id", double(item.id)}}).toObject();
    if (source.isEmpty()) {
        statusBar()->showMessage(tr("The email this came from is no longer in Brick."), 6000);
        return;
    }
    const qint64 folderId = jsonId(source.value("folder_id"));
    const qint64 messageId = jsonId(source.value("message_id"));
    m_tabs->setCurrentIndex(0);
    if (!m_search->text().isEmpty()) {
        QSignalBlocker block(m_search);
        m_search->clear();
    }
    for (QTreeWidgetItemIterator it(m_folders); *it; ++it) {
        if ((*it)->data(0, FolderIdRole).isValid() && (*it)->data(0, FolderIdRole).toLongLong() == folderId) {
            m_folders->setCurrentItem(*it);
            break;
        }
    }
    if (m_model->isSearch())
        folderSelected();
    const int row = m_model->ensureRow(messageId);
    if (row >= 0) {
        const QModelIndex index = m_model->index(row, 0);
        m_list->setCurrentIndex(index);
        m_list->scrollTo(index);
    }
}

void MainWindow::showLinkedItems() {
    if (m_shownId == 0) {
        m_linked->hide();
        return;
    }
    const QList<Item> items = m_items->forMessage(m_shownId);
    if (items.isEmpty()) {
        m_linked->hide();
        return;
    }
    QStringList parts;
    for (const Item &i : items)
        parts << (i.whenLabel().isEmpty() ? i.title : QStringLiteral("%1 (%2)").arg(i.title, i.whenLabel()));
    m_linked->setText(tr("On your lists: %1").arg(parts.join(QStringLiteral("; "))));
    m_linked->show();
}

void MainWindow::accountOptions() {
    AccountOptionsDialog dialog(m_core, this);
    dialog.exec();
}

// J and K: step through the list without leaving the keyboard.
void MainWindow::moveCurrent(int delta) {
    const int rows = m_model->rowCount();
    if (rows == 0)
        return;
    const QModelIndexList selected = m_list->selectionModel()->selectedRows();
    const int current = selected.isEmpty() ? (delta > 0 ? -1 : rows) : selected.first().row();
    const int row = qBound(0, current + delta, rows - 1);
    const QModelIndex index = m_model->index(row, 0);
    m_list->setCurrentIndex(index);
    m_list->scrollTo(index);
}

void MainWindow::importCalendar() {
    const QString path = QFileDialog::getOpenFileName(this, tr("Import calendar"), QString(),
                                                      tr("Calendar files (*.ics);;All files (*)"));
    if (path.isEmpty())
        return;
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        statusBar()->showMessage(tr("Couldn't read %1").arg(path), 8000);
        return;
    }
    const ItemStore::ImportResult r = m_items->importIcs(QString::fromUtf8(file.readAll()));
    if (!r.error.isEmpty()) {
        statusBar()->showMessage(r.error, 8000);
        return;
    }
    QString text = tr("Imported %n item(s)", nullptr, r.added);
    if (r.skipped > 0)
        text += tr(", %1 already there").arg(r.skipped);
    if (r.repeatsDropped > 0)
        text += tr(". %n repeat(s) were too complex to keep", nullptr, r.repeatsDropped);
    statusBar()->showMessage(text, 10000);
}

void MainWindow::exportCalendar() {
    const QString path = QFileDialog::getSaveFileName(this, tr("Export calendar"), QStringLiteral("brick-calendar.ics"),
                                                      tr("Calendar files (*.ics)"));
    if (path.isEmpty())
        return;
    QString error;
    const QString text = m_items->exportIcs(&error);
    if (!error.isEmpty()) {
        statusBar()->showMessage(error, 8000);
        return;
    }
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly) || file.write(text.toUtf8()) < 0) {
        statusBar()->showMessage(tr("Couldn't write %1").arg(path), 8000);
        return;
    }
    statusBar()->showMessage(tr("Exported %1").arg(path), 8000);
}

// The invitation bar's button: adds what the email's invitation describes.
void MainWindow::addInvitation() {
    if (m_shownId == 0)
        return;
    const ItemStore::ImportResult r = m_items->importInvitation(m_shownId);
    if (!r.error.isEmpty())
        statusBar()->showMessage(r.error, 8000);
    else if (r.added == 0)
        statusBar()->showMessage(tr("Already on your calendar"), 6000);
    else
        statusBar()->showMessage(tr("Added %n item(s) to your calendar", nullptr, r.added), 6000);
}

void MainWindow::claudeSettings() {
    ClaudeSettingsDialog dialog(m_core, this);
    dialog.exec();
}

// Sends the open email to Claude. The proposals arrive as an event and are
// shown for review; nothing is saved until the user confirms.
void MainWindow::extractItems() {
    if (m_shownId == 0 || m_extractingId != 0)
        return;
    if (!ClaudeSettingsDialog::isConfigured(m_core)) {
        claudeSettings();
        if (!ClaudeSettingsDialog::isConfigured(m_core))
            return;
    }
    m_extractToken = QUuid::createUuid().toString(QUuid::WithoutBraces);
    QString error;
    m_core->call("items.extract", {{"id", double(m_shownId)}, {"token", m_extractToken}}, &error);
    if (!error.isEmpty()) {
        statusBar()->showMessage(error, 8000);
        return;
    }
    m_extractingId = m_shownId;
    m_extractAct->setEnabled(false);
    statusBar()->showMessage(tr("Asking Claude…"));
}
