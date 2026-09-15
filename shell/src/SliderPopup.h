#pragma once

#include <QHBoxLayout>
#include <QMenu>
#include <QSlider>
#include <QSpinBox>
#include <QToolButton>
#include <QWidgetAction>

/// CS6's little drop-down sliders — the arrow beside Opacity, Fill, Strength
/// and the rest of the percentage fields, which opens a horizontal slider.
///
/// Built as a `QMenu` rather than as a bare popup window so that closing on a
/// click elsewhere, keyboard dismissal and placement all come for free; a
/// hand-rolled frame would have to reimplement each of them.
namespace SliderPopup {

/// A menu holding a slider tied to `field`, in both directions.
inline QMenu *menuFor(QSpinBox *field)
{
    auto *menu = new QMenu(field);
    auto *slider = new QSlider(Qt::Horizontal, menu);
    slider->setRange(field->minimum(), field->maximum());
    slider->setFixedWidth(120);
    auto *action = new QWidgetAction(menu);
    action->setDefaultWidget(slider);
    menu->addAction(action);

    // Taken from the field each time it opens rather than kept in step
    // continuously: the field is the value, and the slider is a way of setting
    // it that only exists while it is on screen.
    QObject::connect(menu, &QMenu::aboutToShow, slider,
                     [slider, field] { slider->setValue(field->value()); });
    QObject::connect(slider, &QSlider::valueChanged, field, &QSpinBox::setValue);
    return menu;
}

/// The arrow button itself, ready to be put beside the field.
///
/// The field's own up and down arrows are taken away at the same time: CS6
/// shows the number alone with one arrow beside it, and leaving the stock pair
/// on gives two controls for one value.
///
/// `setButtonSymbols(NoButtons)` is not enough on its own. The theme styles
/// `QSpinBox::down-button` with a width and a background, and a styled
/// sub-control is drawn whether or not the widget wanted buttons — so the
/// field is marked with a property the stylesheet can see and the buttons are
/// given no width there. Without that there are two arrows side by side and
/// only one of them does anything.
inline QToolButton *arrowFor(QSpinBox *field, const QString &tip)
{
    field->setButtonSymbols(QAbstractSpinBox::NoButtons);
    field->setProperty("sliderPopup", true);

    auto *arrow = new QToolButton(field->parentWidget());
    // Named so the theme can draw it as CS6 does: a small boxed button hard
    // against the field, rather than a bare triangle floating beside it.
    arrow->setObjectName(QStringLiteral("sliderArrow"));
    arrow->setArrowType(Qt::DownArrow);
    arrow->setPopupMode(QToolButton::InstantPopup);
    arrow->setMenu(menuFor(field));
    arrow->setToolTip(tip);
    return arrow;
}

/// The field and its arrow as one widget, ready to go into a toolbar.
///
/// They have to be built into a container rather than added to the bar one
/// after the other: a toolbar puts spacing between the widgets it is given,
/// and CS6 draws these two hard against each other as a single control.
inline QWidget *fieldWithArrow(QSpinBox *field, const QString &tip)
{
    auto *holder = new QWidget(field->parentWidget());
    auto *row = new QHBoxLayout(holder);
    row->setContentsMargins(0, 0, 0, 0);
    row->setSpacing(0);

    QToolButton *arrow = arrowFor(field, tip);
    field->setParent(holder);
    arrow->setParent(holder);
    row->addWidget(field);
    row->addWidget(arrow);
    return holder;
}

} // namespace SliderPopup
