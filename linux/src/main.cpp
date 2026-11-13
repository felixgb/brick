#include <QApplication>
#include <QDir>
#include <QMessageBox>
#include <QStandardPaths>

#include "Core.h"
#include "MainWindow.h"

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("brick"));
    QApplication::setApplicationDisplayName(QStringLiteral("Brick"));
    QApplication::setOrganizationName(QStringLiteral("brick"));
    QApplication::setDesktopFileName(QStringLiteral("brick"));

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
    return app.exec();
}
