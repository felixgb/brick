#include "MessageView.h"

#include <QDesktopServices>
#include <QFile>
#include <QWebEnginePage>
#include <QWebEngineProfile>
#include <QWebEngineSettings>
#include <QWebEngineUrlRequestInfo>

void MailRequestFilter::interceptRequest(QWebEngineUrlRequestInfo &info) {
    const QUrl url = info.requestUrl();
    const QString scheme = url.scheme();
    if (scheme == QLatin1String("data") || scheme == QLatin1String("about"))
        return;
    if (scheme == QLatin1String("file") && url == localDocument)
        return;
    const bool image = info.resourceType() == QWebEngineUrlRequestInfo::ResourceTypeImage;
    if (allowRemoteImages && image && (scheme == QLatin1String("https") || scheme == QLatin1String("http")))
        return;
    info.block(true);
}

namespace {

class MailPage : public QWebEnginePage {
public:
    using QWebEnginePage::QWebEnginePage;

protected:
    bool acceptNavigationRequest(const QUrl &url, NavigationType type, bool isMainFrame) override {
        if (type == NavigationTypeLinkClicked) {
            const QString scheme = url.scheme();
            if (scheme == QLatin1String("https") || scheme == QLatin1String("http") ||
                scheme == QLatin1String("mailto"))
                QDesktopServices::openUrl(url);
            return false;
        }
        // Anything else is our own setHtml()/load(). Scripts are off and the
        // request filter blocks the network, so a message cannot navigate itself.
        Q_UNUSED(isMainFrame);
        return true;
    }

    QWebEnginePage *createWindow(WebWindowType) override { return nullptr; }
};

} // namespace

MessageView::MessageView(QWidget *parent) : QWebEngineView(parent) {
    // An off-the-record profile: no cookies, cache or history survive.
    m_profile = new QWebEngineProfile(this);
    m_filter = new MailRequestFilter(m_profile);
    m_profile->setUrlRequestInterceptor(m_filter);

    auto *page = new MailPage(m_profile, this);
    QWebEngineSettings *s = page->settings();
    s->setAttribute(QWebEngineSettings::JavascriptEnabled, false);
    s->setAttribute(QWebEngineSettings::PluginsEnabled, false);
    s->setAttribute(QWebEngineSettings::LocalStorageEnabled, false);
    s->setAttribute(QWebEngineSettings::LocalContentCanAccessRemoteUrls, false);
    s->setAttribute(QWebEngineSettings::LocalContentCanAccessFileUrls, false);
    s->setAttribute(QWebEngineSettings::AutoLoadImages, true);
    setPage(page);
    setContextMenuPolicy(Qt::NoContextMenu);
}

void MessageView::showHtml(const QString &html, bool allowRemoteImages) {
    m_filter->allowRemoteImages = allowRemoteImages;
    m_filter->localDocument = QUrl();

    // setHtml() goes through a data: URL, which Chromium caps at 2 MB.
    // Larger messages (usually ones with inline images) are loaded from a
    // private temporary file instead.
    const QByteArray utf8 = html.toUtf8();
    if (utf8.size() < 1500 * 1024 || !m_scratch.isValid()) {
        setHtml(html, QUrl(QStringLiteral("about:blank")));
        return;
    }
    QFile file(m_scratch.filePath(QStringLiteral("message.html")));
    if (file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        file.write(utf8);
        file.close();
        m_filter->localDocument = QUrl::fromLocalFile(file.fileName());
        load(m_filter->localDocument);
    }
}

void MessageView::clear() {
    m_filter->allowRemoteImages = false;
    setHtml(QString(), QUrl(QStringLiteral("about:blank")));
}
