// The chrome on a panel's own title bar.
//
// Everything asserted here is a Qt internal the shell leans on and cannot see
// fail: the two buttons are found by object names Qt sets privately, and
// their icons come from pixmap roles Qt picks privately. Get any of it wrong
// and the buttons silently keep Fusion's unlabelled squares with no tooltip —
// which looks exactly like the code never ran.
//
// The theme is applied here for the same reason. A stylesheet makes Qt wrap
// the application style in a QStyleSheetStyle that answers for the title
// bar's subcontrols itself, and theme.qss has rules for both buttons. Testing
// without it would exercise a style chain the application never runs.

#include "panels/DockTitleStyle.h"

#include <QAbstractButton>
#include <QApplication>
#include <QDockWidget>
#include <QFile>
#include <QMainWindow>
#include <QStyleFactory>
#include <QTest>

#include <memory>

class TestDockTitle : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void theTitleButtonsAreFindableByName();
    void theChevronReachesTheFloatButton();
    void theCrossReachesTheCloseButton();

private:
    /// A dock in a window, both on the style under test, with the buttons
    /// Qt built for its title bar.
    struct Fixture {
        std::unique_ptr<QMainWindow> window;
        QDockWidget *dock = nullptr;
        QAbstractButton *floatButton = nullptr;
        QAbstractButton *closeButton = nullptr;
    };
    Fixture makeDock();

    DockTitleStyle m_style{QStringLiteral("Fusion")};
};

void TestDockTitle::initTestCase()
{
    // The real theme, found the way main.cpp finds it.
    QFile theme(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/theme.qss"));
    QVERIFY2(theme.open(QIODevice::ReadOnly | QIODevice::Text), "theme.qss must be readable");
    qApp->setStyleSheet(QString::fromUtf8(theme.readAll()));
}

TestDockTitle::Fixture TestDockTitle::makeDock()
{
    Fixture fixture;
    fixture.window = std::make_unique<QMainWindow>();
    fixture.dock = new QDockWidget(QStringLiteral("Channels"), fixture.window.get());
    fixture.window->addDockWidget(Qt::RightDockWidgetArea, fixture.dock);
    fixture.window->setStyle(&m_style);
    fixture.dock->setStyle(&m_style);

    fixture.floatButton =
        fixture.dock->findChild<QAbstractButton *>(QStringLiteral("qt_dockwidget_floatbutton"));
    fixture.closeButton =
        fixture.dock->findChild<QAbstractButton *>(QStringLiteral("qt_dockwidget_closebutton"));
    return fixture;
}

void TestDockTitle::theTitleButtonsAreFindableByName()
{
    // `MainWindow::createDocks` finds both by name to give them tooltips —
    // the float button's follows the panel between docked and floating.
    const Fixture fixture = makeDock();
    QVERIFY(fixture.floatButton);
    QVERIFY(fixture.closeButton);
}

void TestDockTitle::theChevronReachesTheFloatButton()
{
    const Fixture fixture = makeDock();
    QVERIFY(fixture.floatButton);

    const QSize size(16, 16);
    const QImage onTheButton = fixture.floatButton->icon().pixmap(size).toImage();
    QVERIFY(!onTheButton.isNull());

    // It has to be the chevron the style offers, and not what Fusion would
    // have put there — SP_TitleBarNormalButton being the wrong role would
    // leave the second of those in place.
    const QImage fromTheStyle =
        m_style.standardIcon(QStyle::SP_TitleBarNormalButton, nullptr, fixture.dock)
            .pixmap(size)
            .toImage();
    QCOMPARE(onTheButton, fromTheStyle);

    std::unique_ptr<QStyle> fusion(QStyleFactory::create(QStringLiteral("Fusion")));
    QVERIFY(fusion);
    QVERIFY(onTheButton
            != fusion->standardIcon(QStyle::SP_TitleBarNormalButton, nullptr, fixture.dock)
                   .pixmap(size)
                   .toImage());
}

void TestDockTitle::theCrossReachesTheCloseButton()
{
    const Fixture fixture = makeDock();
    QVERIFY(fixture.closeButton);

    const QSize size(16, 16);
    const QImage onTheButton = fixture.closeButton->icon().pixmap(size).toImage();
    QVERIFY(!onTheButton.isNull());

    // Whichever of the two close roles the dock asked for, the style answers
    // both with the same cross, so either stands in for what it handed over.
    const QImage fromTheStyle =
        m_style.standardIcon(QStyle::SP_DockWidgetCloseButton, nullptr, fixture.dock)
            .pixmap(size)
            .toImage();
    QCOMPARE(onTheButton, fromTheStyle);

    std::unique_ptr<QStyle> fusion(QStyleFactory::create(QStringLiteral("Fusion")));
    QVERIFY(fusion);
    QVERIFY(onTheButton
            != fusion->standardIcon(QStyle::SP_DockWidgetCloseButton, nullptr, fixture.dock)
                   .pixmap(size)
                   .toImage());
}

QTEST_MAIN(TestDockTitle)
#include "tst_docktitle.moc"
