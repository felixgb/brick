#include "Core.h"

#include <QJsonDocument>
#include <QMetaObject>

#include "brick.h"

Core::Core(const QString &dataDir, QObject *parent) : QObject(parent) {
    char *error = nullptr;
    m_engine = brick_open(dataDir.toUtf8().constData(), &Core::onEvent, this, &error);
    if (!m_engine) {
        m_openError = error ? QString::fromUtf8(error) : QStringLiteral("unknown error");
        brick_string_free(error);
    }
}

Core::~Core() {
    // After this returns the engine makes no further callbacks.
    brick_close(m_engine);
}

void Core::onEvent(const char *json, void *self) {
    // Runs on an engine thread: copy the payload and hop to the GUI thread.
    auto *core = static_cast<Core *>(self);
    const QByteArray payload(json);
    QMetaObject::invokeMethod(
        core,
        [core, payload] {
            const QJsonObject obj = QJsonDocument::fromJson(payload).object();
            if (!obj.isEmpty())
                emit core->event(obj);
        },
        Qt::QueuedConnection);
}

QJsonValue Core::call(const QString &method, const QJsonObject &params, QString *error) {
    if (!m_engine) {
        if (error)
            *error = m_openError;
        return QJsonValue(QJsonValue::Undefined);
    }
    QJsonObject request{{"method", method}, {"params", params}};
    const QByteArray bytes = QJsonDocument(request).toJson(QJsonDocument::Compact);
    char *raw = brick_call(m_engine, bytes.constData());
    const QJsonObject response = QJsonDocument::fromJson(QByteArray(raw)).object();
    brick_string_free(raw);

    if (response.contains("error")) {
        if (error)
            *error = response.value("error").toString();
        return QJsonValue(QJsonValue::Undefined);
    }
    return response.value("ok");
}
