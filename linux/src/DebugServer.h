#pragma once

#include <QJsonObject>
#include <QObject>

class Core;
class MainWindow;
class QLocalServer;
class QLocalSocket;
class QWidget;

// Lets a script (or Claude) look inside and drive the running app. Only
// started when BRICK_DEBUG_SOCKET names a socket path; see linux/tools/brickctl.py.
//
// Protocol: one JSON object per line in, one JSON object per line out.
//   {"cmd": "tree"}                       the visible widget tree
//   {"cmd": "windows"}                    top-level windows
//   {"cmd": "screenshot", "path": "x.png", "window": "objectName?"}
//   {"cmd": "click", "target": "name" | "text:Label", "pos": [x, y]?, "double": false, "button": "left|right"}
//   {"cmd": "invoke", "target": "name", "method": "click"}
//   {"cmd": "action", "text": "Next message"}   trigger a menu or toolbar action
//   {"cmd": "key", "key": "Return", "mod": "ctrl+shift"?, "target": "name"?}
//   {"cmd": "type", "text": "hello", "target": "name"?}
//   {"cmd": "set", "target": "name", "index": 2 | "text": "..." | "checked": true}
//   {"cmd": "call", "method": "items.list", "params": {}}   engine request
//   {"cmd": "wait", "ms": 100}            replies after a delay
//   {"cmd": "quit"}
// Anything that sends input is queued and the reply says so, because a click
// can open a modal dialog and the reply must not wait for it to close.
class DebugServer : public QObject {
    Q_OBJECT
public:
    DebugServer(Core *core, MainWindow *window, const QString &socketPath, QObject *parent = nullptr);

    bool isListening() const;

private:
    void onConnection();
    void onLine(QLocalSocket *socket, const QByteArray &line);
    QJsonObject run(const QJsonObject &cmd, QLocalSocket *socket, bool *deferred);

    QWidget *find(const QJsonValue &target) const;

    Core *m_core;
    MainWindow *m_window;
    QLocalServer *m_server;
};
