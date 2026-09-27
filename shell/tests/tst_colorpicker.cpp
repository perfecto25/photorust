// The Color Picker's eyedropper over the image.
//
// With the picker open the pointer becomes an eyedropper over the image, as in
// CS6, and a colour is chosen by clicking — not by hovering. A picker that
// followed the pointer changed the colour on its way back to the dialog, over
// whatever it happened to cross.

#include "dialogs/ColorPickerDialog.h"

#include <QCursor>
#include <QMainWindow>
#include <QTest>

class TestColorPicker : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void hoveringChoosesNothing();
    void aClickChoosesAndTheColourStays();
    void draggingFollowsUntilLetGo();

private:
    /// The window standing in for the image: every screen position over it
    /// has a colour of its own, so a sample taken in the wrong place shows.
    QMainWindow *m_image = nullptr;
    ColorPickerDialog *m_dialog = nullptr;

    QPoint onImage(int x, int y) const { return m_image->geometry().topLeft() + QPoint(x, y); }
    /// Let the dialog's pointer polling run.
    static void settle() { QTest::qWait(100); }
    void click(const QPoint &global)
    {
        QTest::mousePress(m_dialog, Qt::LeftButton, {}, m_dialog->mapFromGlobal(global));
        QTest::mouseRelease(m_dialog, Qt::LeftButton, {}, m_dialog->mapFromGlobal(global));
    }
    static QColor colourAt(const QPoint &global) { return QColor(global.x() % 256, global.y() % 256, 50); }
};

void TestColorPicker::init()
{
    delete m_dialog;
    delete m_image;
    m_image = new QMainWindow;
    m_image->resize(1200, 800);
    m_image->show();
    QVERIFY(QTest::qWaitForWindowExposed(m_image));
    QMainWindow *image = m_image;
    ColorPickerDialog::setSampler([image](const QPoint &global) {
        return image->frameGeometry().contains(global) ? colourAt(global) : QColor();
    });
    m_dialog = new ColorPickerDialog(Qt::black, m_image);
    m_dialog->move(m_image->geometry().topLeft() + QPoint(600, 100));
    m_dialog->show();
    QVERIFY(QTest::qWaitForWindowExposed(m_dialog));
}

void TestColorPicker::hoveringChoosesNothing()
{
    QCursor::setPos(onImage(100, 400));
    settle();
    QCursor::setPos(onImage(180, 450));
    settle();
    QCOMPARE(m_dialog->selectedColor(), QColor(Qt::black));
}

void TestColorPicker::aClickChoosesAndTheColourStays()
{
    const QPoint there = onImage(100, 400);
    QCursor::setPos(there);
    settle();
    click(there);
    QCOMPARE(m_dialog->selectedColor(), colourAt(there));

    // On the way back to the dialog, across the image: the colour stays.
    QCursor::setPos(onImage(300, 300));
    settle();
    QCursor::setPos(onImage(500, 200));
    settle();
    QCOMPARE(m_dialog->selectedColor(), colourAt(there));

    // Until the next click.
    const QPoint elsewhere = onImage(250, 600);
    QCursor::setPos(elsewhere);
    settle();
    click(elsewhere);
    QCOMPARE(m_dialog->selectedColor(), colourAt(elsewhere));
}

void TestColorPicker::draggingFollowsUntilLetGo()
{
    // Holding the button down and dragging keeps choosing, as CS6's does; the
    // colour is where it was let go, and stays there.
    const QPoint start = onImage(100, 400);
    const QPoint end = onImage(160, 430);
    QCursor::setPos(start);
    settle();
    QTest::mousePress(m_dialog, Qt::LeftButton, {}, m_dialog->mapFromGlobal(start));
    QMouseEvent move(QEvent::MouseMove, QPointF(m_dialog->mapFromGlobal(end)), QPointF(end),
                     Qt::NoButton, Qt::LeftButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_dialog, &move);
    QCOMPARE(m_dialog->selectedColor(), colourAt(end));
    QTest::mouseRelease(m_dialog, Qt::LeftButton, {}, m_dialog->mapFromGlobal(end));

    QCursor::setPos(onImage(400, 200));
    settle();
    QCOMPARE(m_dialog->selectedColor(), colourAt(end));
}

QTEST_MAIN(TestColorPicker)
#include "tst_colorpicker.moc"
