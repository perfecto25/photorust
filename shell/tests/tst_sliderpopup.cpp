// The drop-down sliders beside CS6's percentage fields.
//
// A slider and a field showing the same number is a pair that can disagree,
// and the ways it can are not visible from looking at either: the slider can
// open showing a stale value, or moving it can leave the field behind, or the
// field's own arrows can survive and give two controls for one value. Each of
// those is a small wrong number rather than anything that looks broken.

#include "SliderPopup.h"

#include <QMenu>
#include <QSlider>
#include <QSpinBox>
#include <QTest>
#include <QToolButton>
#include <QLayout>
#include <QWidget>

class TestSliderPopup : public QObject
{
    Q_OBJECT

private slots:
    void theSliderOpensOnWhateverTheFieldSays();
    void movingTheSliderMovesTheField();
    void theSliderInheritsTheFieldsRange();
    void theFieldsOwnArrowsAreTurnedOff();
    void theFieldIsMarkedSoTheThemeCanHideItsArrows();
    void theFieldAndItsArrowGoInAsOneWidget();
};

void TestSliderPopup::theSliderOpensOnWhateverTheFieldSays()
{
    QWidget parent;
    auto *field = new QSpinBox(&parent);
    field->setRange(0, 100);
    field->setValue(55);
    QMenu *menu = SliderPopup::menuFor(field);
    auto *slider = menu->findChild<QSlider *>();
    QVERIFY(slider);

    // Typed into after the popup was built: it has to catch up when it opens,
    // not when it was made.
    field->setValue(83);
    emit menu->aboutToShow();
    QCOMPARE(slider->value(), 83);
}

void TestSliderPopup::movingTheSliderMovesTheField()
{
    QWidget parent;
    auto *field = new QSpinBox(&parent);
    field->setRange(0, 100);
    field->setValue(20);
    QMenu *menu = SliderPopup::menuFor(field);
    auto *slider = menu->findChild<QSlider *>();
    QVERIFY(slider);

    slider->setValue(64);
    QCOMPARE(field->value(), 64);
}

void TestSliderPopup::theSliderInheritsTheFieldsRange()
{
    // An angle runs from -180, not from zero. A slider stuck on 0..100 would
    // quietly clamp half of it away.
    QWidget parent;
    auto *field = new QSpinBox(&parent);
    field->setRange(-180, 180);
    QMenu *menu = SliderPopup::menuFor(field);
    auto *slider = menu->findChild<QSlider *>();
    QVERIFY(slider);
    QCOMPARE(slider->minimum(), -180);
    QCOMPARE(slider->maximum(), 180);
}

void TestSliderPopup::theFieldsOwnArrowsAreTurnedOff()
{
    // CS6 shows the number alone with one arrow beside it. Leaving the stock
    // pair on would be a second control for the same value, in a bar where
    // every pixel of width is spoken for.
    QWidget parent;
    auto *field = new QSpinBox(&parent);
    field->setRange(0, 100);
    QToolButton *arrow = SliderPopup::arrowFor(field, QStringLiteral("Strength slider"));
    QVERIFY(arrow);
    QCOMPARE(field->buttonSymbols(), QAbstractSpinBox::NoButtons);
    QCOMPARE(arrow->arrowType(), Qt::DownArrow);
    QVERIFY(arrow->menu());
}

void TestSliderPopup::theFieldIsMarkedSoTheThemeCanHideItsArrows()
{
    // `NoButtons` is not enough on its own: the theme gives
    // `QSpinBox::down-button` a width and a background, and a styled
    // sub-control is drawn whether or not the widget wanted buttons. The
    // result is two arrows side by side with only one of them live. The
    // property is what the stylesheet keys off to take the stock pair away,
    // so the two have to stay in step — see theme.qss.
    QWidget parent;
    auto *field = new QSpinBox(&parent);
    field->setRange(0, 100);
    SliderPopup::arrowFor(field, QStringLiteral("Strength slider"));
    QVERIFY2(field->property("sliderPopup").toBool(),
             "the field is not marked, so the theme will go on drawing its own arrows");
}

void TestSliderPopup::theFieldAndItsArrowGoInAsOneWidget()
{
    // A toolbar puts spacing between the widgets it is given, and CS6 draws
    // these two hard against each other as a single control — so they have to
    // arrive as one.
    QWidget parent;
    auto *field = new QSpinBox(&parent);
    field->setRange(0, 100);
    QWidget *holder = SliderPopup::fieldWithArrow(field, QStringLiteral("Strength slider"));
    QVERIFY(holder);
    QCOMPARE(field->parentWidget(), holder);
    QCOMPARE(holder->findChildren<QToolButton *>().size(), 1);
    QVERIFY(holder->layout());
    QCOMPARE(holder->layout()->spacing(), 0);
    QCOMPARE(holder->layout()->contentsMargins(), QMargins(0, 0, 0, 0));
}

QTEST_MAIN(TestSliderPopup)
#include "tst_sliderpopup.moc"
