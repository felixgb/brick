#pragma once

#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QString>

struct BrickEngine;

// Thin wrapper over the engine's C interface. call() is synchronous and only
// touches the local store; network results arrive through event().
class Core : public QObject {
    Q_OBJECT
public:
    explicit Core(const QString &dataDir, QObject *parent = nullptr);
    ~Core() override;

    bool isOpen() const { return m_engine != nullptr; }
    QString openError() const { return m_openError; }

    // Returns the "ok" value. On failure returns Undefined and sets *error.
    QJsonValue call(const QString &method, const QJsonObject &params = {}, QString *error = nullptr);

signals:
    void event(const QJsonObject &event);

private:
    static void onEvent(const char *json, void *self);

    BrickEngine *m_engine = nullptr;
    QString m_openError;
};
