#include "DebugServer.h"

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QAction>
#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QJsonArray>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QLocalServer>
#include <QLocalSocket>
#include <QMenuBar>
#include <QMouseEvent>
#include <QPlainTextEdit>
#include <QPointer>
#include <QTabBar>
#include <QTimer>
#include <QToolBar>

#include "Core.h"
#include "MainWindow.h"

namespace {

QJsonArray rect(const QRect &r) {
    return {r.x(), r.y(), r.width(), r.height()};
}

QString textOf(const QWidget *w) {
    if (auto *b = qobject_cast<const QAbstractButton *>(w))
        return b->text();
    if (auto *l = qobject_cast<const QLabel *>(w))
        return l->text();
    return {};
}

QJsonObject describe(QWidget *w, int depth) {
    QJsonObject o;
    o["class"] = QString::fromLatin1(w->metaObject()->className());
    if (!w->objectName().isEmpty())
        o["name"] = w->objectName();
    o["geometry"] = rect(w->geometry());
    o["global"] = QJsonArray{w->mapToGlobal(QPoint(0, 0)).x(), w->mapToGlobal(QPoint(0, 0)).y()};
    if (!w->isEnabled())
        o["enabled"] = false;
    if (w->hasFocus())
        o["focus"] = true;
    if (w->isWindow())
        o["title"] = w->windowTitle();
    if (!w->toolTip().isEmpty())
        o["tooltip"] = w->toolTip();

    const QString text = textOf(w);
    if (!text.isEmpty())
        o["text"] = text;
    if (auto *b = qobject_cast<QAbstractButton *>(w); b && b->isCheckable())
        o["checked"] = b->isChecked();
    if (auto *e = qobject_cast<QLineEdit *>(w)) {
        o["value"] = e->text();
        if (!e->placeholderText().isEmpty())
            o["placeholder"] = e->placeholderText();
    }
    if (auto *e = qobject_cast<QPlainTextEdit *>(w))
        o["value"] = e->toPlainText();
    if (auto *c = qobject_cast<QComboBox *>(w))
        o["value"] = c->currentText();
    if (auto *t = qobject_cast<QTabBar *>(w)) {
        QJsonArray tabs;
        for (int i = 0; i < t->count(); ++i)
            tabs.append(t->tabText(i));
        o["tabs"] = tabs;
        o["currentTab"] = t->currentIndex();
    }
    if (auto *tb = qobject_cast<QToolBar *>(w)) {
        QJsonArray actions;
        for (QAction *a : tb->actions()) {
            if (!a->isSeparator() && !a->text().isEmpty())
                actions.append(QJsonObject{{"text", a->text()}, {"enabled", a->isEnabled()}, {"visible", a->isVisible()}});
        }
        o["actions"] = actions;
    }
    if (auto *mb = qobject_cast<QMenuBar *>(w)) {
        QJsonArray menus;
        for (QAction *a : mb->actions())
            menus.append(a->text());
        o["menus"] = menus;
    }
    if (auto *v = qobject_cast<QAbstractItemView *>(w); v && v->model()) {
        o["rows"] = v->model()->rowCount();
        QJsonArray first;
        for (int r = 0; r < qMin(25, v->model()->rowCount()); ++r)
            first.append(v->model()->index(r, 0).data(Qt::DisplayRole).toString());
        o["firstRows"] = first;
    }

    if (depth > 0) {
        QJsonArray kids;
        for (QObject *c : w->children()) {
            auto *cw = qobject_cast<QWidget *>(c);
            // Skip hidden widgets and the plumbing inside scroll areas.
            if (!cw || cw->isWindow() || !cw->isVisible())
                continue;
            kids.append(describe(cw, depth - 1));
        }
        if (!kids.isEmpty())
            o["children"] = kids;
    }
    return o;
}

QWidget *deepestAt(QWidget *root, QPoint *pos) {
    QWidget *w = root;
    for (;;) {
        QWidget *c = w->childAt(*pos);
        if (!c)
            return w;
        *pos = c->mapFrom(w, *pos);
        w = c;
    }
}

void sendMouse(QWidget *w, QPoint pos, Qt::MouseButton button, bool dbl) {
    QWidget *t = deepestAt(w, &pos);
    auto send = [&](QEvent::Type type) {
        const QPoint global = t->mapToGlobal(pos);
        QMouseEvent e(type, QPointF(pos), QPointF(global), button,
                      type == QEvent::MouseButtonRelease ? Qt::NoButton : button, Qt::NoModifier);
        QApplication::sendEvent(t, &e);
    };
    send(QEvent::MouseButtonPress);
    send(QEvent::MouseButtonRelease);
    if (dbl) {
        send(QEvent::MouseButtonDblClick);
        send(QEvent::MouseButtonRelease);
    }
}

Qt::KeyboardModifiers modifiersFrom(const QString &s) {
    Qt::KeyboardModifiers m;
    for (const QString &part : s.toLower().split(QLatin1Char('+'), Qt::SkipEmptyParts)) {
        if (part == QLatin1String("ctrl"))
            m |= Qt::ControlModifier;
        else if (part == QLatin1String("shift"))
            m |= Qt::ShiftModifier;
        else if (part == QLatin1String("alt"))
            m |= Qt::AltModifier;
        else if (part == QLatin1String("meta"))
            m |= Qt::MetaModifier;
    }
    return m;
}

void sendKey(QWidget *w, int key, Qt::KeyboardModifiers mods, const QString &text) {
    QKeyEvent press(QEvent::KeyPress, key, mods, text);
    QApplication::sendEvent(w, &press);
    QKeyEvent release(QEvent::KeyRelease, key, mods, text);
    QApplication::sendEvent(w, &release);
}

QJsonObject err(const QString &message) {
    return {{"ok", false}, {"error", message}};
}

QJsonObject ok(const QJsonObject &extra = {}) {
    QJsonObject o = extra;
    o["ok"] = true;
    return o;
}

} // namespace

DebugServer::DebugServer(Core *core, MainWindow *window, const QString &socketPath, QObject *parent)
    : QObject(parent), m_core(core), m_window(window), m_server(new QLocalServer(this)) {
    QLocalServer::removeServer(socketPath);
    m_server->listen(socketPath);
    connect(m_server, &QLocalServer::newConnection, this, &DebugServer::onConnection);
}

bool DebugServer::isListening() const {
    return m_server->isListening();
}

void DebugServer::onConnection() {
    while (QLocalSocket *socket = m_server->nextPendingConnection()) {
        connect(socket, &QLocalSocket::disconnected, socket, &QObject::deleteLater);
        connect(socket, &QLocalSocket::readyRead, this, [this, socket] {
            while (socket->canReadLine())
                onLine(socket, socket->readLine());
        });
    }
}

void DebugServer::onLine(QLocalSocket *socket, const QByteArray &line) {
    QJsonParseError parse;
    const QJsonObject cmd = QJsonDocument::fromJson(line, &parse).object();
    QJsonObject reply;
    bool deferred = false;
    if (parse.error != QJsonParseError::NoError)
        reply = err(QStringLiteral("bad JSON: ") + parse.errorString());
    else
        reply = run(cmd, socket, &deferred);
    if (deferred)
        return; // the command answers for itself later
    socket->write(QJsonDocument(reply).toJson(QJsonDocument::Compact) + '\n');
    socket->flush();
}

QWidget *DebugServer::find(const QJsonValue &target) const {
    const QString t = target.toString();
    if (t.isEmpty())
        return nullptr;
    QWidget *fallback = nullptr;
    const QWidgetList all = QApplication::allWidgets();
    for (QWidget *w : all) {
        bool match = false;
        if (t.startsWith(QLatin1String("text:")))
            match = textOf(w) == t.mid(5);
        else if (t.startsWith(QLatin1String("class:")))
            match = QString::fromLatin1(w->metaObject()->className()) == t.mid(6);
        else
            match = w->objectName() == t;
        if (!match)
            continue;
        if (w->isVisible())
            return w;
        fallback = w;
    }
    return fallback;
}

QJsonObject DebugServer::run(const QJsonObject &cmd, QLocalSocket *socket, bool *deferred) {
    const QString name = cmd.value("cmd").toString();

    if (name == QLatin1String("tree")) {
        QJsonArray windows;
        const int depth = cmd.value("depth").toInt(40);
        for (QWidget *w : QApplication::topLevelWidgets()) {
            if (w->isVisible())
                windows.append(describe(w, depth));
        }
        return ok({{"windows", windows}});
    }

    if (name == QLatin1String("windows")) {
        QJsonArray out;
        for (QWidget *w : QApplication::topLevelWidgets()) {
            if (!w->isVisible())
                continue;
            out.append(QJsonObject{{"class", QString::fromLatin1(w->metaObject()->className())},
                                   {"name", w->objectName()},
                                   {"title", w->windowTitle()},
                                   {"geometry", rect(w->geometry())},
                                   {"modal", w->isModal()},
                                   {"active", w == QApplication::activeWindow()}});
        }
        return ok({{"windows", out}});
    }

    if (name == QLatin1String("screenshot")) {
        QWidget *w = nullptr;
        if (cmd.contains("window"))
            w = find(cmd.value("window"));
        if (!w)
            w = QApplication::activeModalWidget();
        if (!w)
            w = m_window;
        const QString path = cmd.value("path").toString();
        if (path.isEmpty())
            return err(QStringLiteral("screenshot needs a path"));
        const QPixmap pixmap = w->grab();
        if (!pixmap.save(path))
            return err(QStringLiteral("couldn't write ") + path);
        return ok({{"path", path}, {"width", pixmap.width()}, {"height", pixmap.height()},
                   {"window", w->windowTitle()}});
    }

    if (name == QLatin1String("call")) {
        QString error;
        const QJsonValue v = m_core->call(cmd.value("method").toString(), cmd.value("params").toObject(), &error);
        if (!error.isEmpty())
            return err(error);
        return ok({{"value", v}});
    }

    if (name == QLatin1String("wait")) {
        QPointer<QLocalSocket> guard(socket);
        QTimer::singleShot(cmd.value("ms").toInt(100), this, [guard] {
            if (guard) {
                guard->write(QJsonDocument(QJsonObject{{"ok", true}}).toJson(QJsonDocument::Compact) + '\n');
                guard->flush();
            }
        });
        *deferred = true;
        return {};
    }

    if (name == QLatin1String("quit")) {
        QTimer::singleShot(0, qApp, &QApplication::quit);
        return ok();
    }

    if (name == QLatin1String("set")) {
        QWidget *w = find(cmd.value("target"));
        if (!w)
            return err(QStringLiteral("no such widget"));
        if (auto *tabs = qobject_cast<QTabBar *>(w); tabs && cmd.contains("index"))
            tabs->setCurrentIndex(cmd.value("index").toInt());
        else if (auto *combo = qobject_cast<QComboBox *>(w); combo && cmd.contains("index"))
            combo->setCurrentIndex(cmd.value("index").toInt());
        else if (auto *line = qobject_cast<QLineEdit *>(w); line && cmd.contains("text"))
            line->setText(cmd.value("text").toString());
        else if (auto *plain = qobject_cast<QPlainTextEdit *>(w); plain && cmd.contains("text"))
            plain->setPlainText(cmd.value("text").toString());
        else if (auto *button = qobject_cast<QAbstractButton *>(w); button && cmd.contains("checked"))
            button->setChecked(cmd.value("checked").toBool());
        else
            return err(QStringLiteral("don't know how to set a ") + QString::fromLatin1(w->metaObject()->className()));
        return ok();
    }

    // Everything below sends input. Queue it so a modal dialog opened by the
    // input can't hold up the reply.
    if (name == QLatin1String("click")) {
        QWidget *w = find(cmd.value("target"));
        if (!w)
            return err(QStringLiteral("no such widget"));
        QPoint pos = w->rect().center();
        if (cmd.contains("pos")) {
            const QJsonArray p = cmd.value("pos").toArray();
            pos = QPoint(p.at(0).toInt(), p.at(1).toInt());
        }
        const bool right = cmd.value("button").toString() == QLatin1String("right");
        const bool dbl = cmd.value("double").toBool();
        QPointer<QWidget> guard(w);
        QTimer::singleShot(0, this, [guard, pos, right, dbl] {
            if (guard)
                sendMouse(guard, pos, right ? Qt::RightButton : Qt::LeftButton, dbl);
        });
        return ok({{"queued", true}});
    }

    if (name == QLatin1String("invoke")) {
        QWidget *w = find(cmd.value("target"));
        if (!w)
            return err(QStringLiteral("no such widget"));
        const QByteArray method = cmd.value("method").toString().toUtf8();
        if (w->metaObject()->indexOfMethod(method + "()") < 0)
            return err(QStringLiteral("no method ") + QString::fromUtf8(method) + QStringLiteral("() on ")
                       + QString::fromLatin1(w->metaObject()->className()));
        QPointer<QWidget> guard(w);
        QTimer::singleShot(0, this, [guard, method] {
            if (guard)
                QMetaObject::invokeMethod(guard, method.constData());
        });
        return ok({{"queued", true}});
    }

    if (name == QLatin1String("key") || name == QLatin1String("type")) {
        QWidget *w = cmd.contains("target") ? find(cmd.value("target")) : QApplication::focusWidget();
        if (!w)
            w = QApplication::activeWindow();
        if (!w)
            return err(QStringLiteral("nothing has focus"));
        QPointer<QWidget> guard(w);
        if (name == QLatin1String("key")) {
            const QKeySequence seq(cmd.value("key").toString());
            if (seq.isEmpty())
                return err(QStringLiteral("unknown key"));
            const int key = int(seq[0].key());
            const Qt::KeyboardModifiers mods = modifiersFrom(cmd.value("mod").toString());
            QTimer::singleShot(0, this, [guard, key, mods] {
                if (guard)
                    sendKey(guard, key, mods, QString());
            });
        } else {
            const QString text = cmd.value("text").toString();
            QTimer::singleShot(0, this, [guard, text] {
                if (!guard)
                    return;
                for (const QChar ch : text) {
                    const int key = ch.isLetterOrNumber() ? int(ch.toUpper().unicode()) : int(ch.unicode());
                    sendKey(guard, key, Qt::NoModifier, QString(ch));
                }
            });
        }
        return ok({{"queued", true}});
    }

    return err(QStringLiteral("unknown command: ") + name);
}
