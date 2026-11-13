#pragma once

#include <QTemporaryDir>
#include <QUrl>
#include <QWebEngineUrlRequestInterceptor>
#include <QWebEngineView>

class QWebEngineProfile;

// Blocks every network request a message tries to make. Images are allowed
// only after the user opts in for the message on screen.
class MailRequestFilter : public QWebEngineUrlRequestInterceptor {
    Q_OBJECT
public:
    using QWebEngineUrlRequestInterceptor::QWebEngineUrlRequestInterceptor;
    void interceptRequest(QWebEngineUrlRequestInfo &info) override;

    bool allowRemoteImages = false;
    QUrl localDocument;
};

// Shows one message body. JavaScript is off, nothing is persisted, and links
// open in the system browser instead of navigating the view.
class MessageView : public QWebEngineView {
    Q_OBJECT
public:
    explicit MessageView(QWidget *parent = nullptr);

    void showHtml(const QString &html, bool allowRemoteImages);
    void clear();

private:
    QWebEngineProfile *m_profile;
    MailRequestFilter *m_filter;
    QTemporaryDir m_scratch;
};
