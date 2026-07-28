#include "MainWindow.h"
#include "shortcuts/CommandRegistry.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QLoggingCategory>
#include <QStandardPaths>

Q_LOGGING_CATEGORY(lcApp, "photorust.app")

namespace {

/// Locate a bundled resource, trying the paths that matter on our two targets.
///
/// Order: next to the executable (the normal installed layout and the macOS
/// bundle's Resources dir), then the source tree, so a developer running
/// straight out of `build/` gets the theme without an install step.
QString findResource(const QString &name)
{
    const QString appDir = QCoreApplication::applicationDirPath();
    const QStringList candidates = {
        appDir + QStringLiteral("/resources/") + name,
        appDir + QStringLiteral("/../Resources/") + name, // macOS .app bundle
        appDir + QStringLiteral("/../shell/resources/") + name,
        QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/") + name,
    };

    for (const QString &path : candidates) {
        if (QFile::exists(path)) {
            return QFileInfo(path).absoluteFilePath();
        }
    }
    return {};
}

/// Load and apply the CS6 dark stylesheet.
void applyTheme(QApplication &app)
{
    const QString path = findResource(QStringLiteral("theme.qss"));
    if (path.isEmpty()) {
        qCWarning(lcApp) << "theme.qss not found; falling back to the Qt default style";
        return;
    }

    QFile file(path);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
        qCWarning(lcApp) << "could not read" << path;
        return;
    }
    app.setStyleSheet(QString::fromUtf8(file.readAll()));
}

} // namespace

int main(int argc, char *argv[])
{
    QApplication app(argc, argv);
    QCoreApplication::setApplicationName(QStringLiteral("PhotoRust"));
    QCoreApplication::setOrganizationName(QStringLiteral("PhotoRust"));
    QCoreApplication::setApplicationVersion(QStringLiteral("0.1.0"));

    // Fusion gives QSS a predictable base to style; the native styles on Linux
    // and macOS each override too much of it to match CS6 reliably.
    QApplication::setStyle(QStringLiteral("Fusion"));
    applyTheme(app);

    // The engine: one instance, owning the open document. Everything the UI
    // does goes through this object.
    Engine engine;

    CommandRegistry registry;
    const QString keymap = findResource(QStringLiteral("shortcuts.json"));
    if (keymap.isEmpty() || !registry.load(keymap)) {
        qCWarning(lcApp) << "no keymap loaded; keyboard shortcuts will be unavailable";
    }

    MainWindow window(&engine, &registry);
    window.show();

    return app.exec();
}
