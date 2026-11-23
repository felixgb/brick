#include <QApplication>
#include <QDir>
#include <QIcon>
#include <QMessageBox>
#include <QStandardPaths>

#include "Core.h"
#include "DebugServer.h"
#include "MainWindow.h"

int main(int argc, char *argv[]) {
    // QtWebEngine needs this set before the application object exists.
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("brick"));
    QApplication::setApplicationDisplayName(QStringLiteral("Brick"));
    QApplication::setOrganizationName(QStringLiteral("brick"));
    QApplication::setDesktopFileName(QStringLiteral("brick"));

    // The installed theme icon if there is one, else the copy built into the binary.
    QIcon builtIn;
    for (int size : {16, 32, 48, 64, 128, 256})
        builtIn.addFile(QStringLiteral(":/icon/brick-%1.png").arg(size), QSize(size, size));
    QApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("brick"), builtIn));

    // ~/.local/share/brick
    const QString dataDir = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    QDir().mkpath(dataDir);

    Core core(dataDir);
    if (!core.isOpen()) {
        QMessageBox::critical(nullptr, QStringLiteral("Brick"),
                              QObject::tr("Brick couldn't open its mail store.\n\n%1").arg(core.openError()));
        return 1;
    }

    MainWindow window(&core);
    window.show();

    // Debug hook: lets a script inspect and drive the running app.
    // See linux/tools/brickctl.py.
    const QString debugSocket = qEnvironmentVariable("BRICK_DEBUG_SOCKET");
    if (!debugSocket.isEmpty())
        new DebugServer(&core, &window, debugSocket, &app);

    return app.exec();
}
