#include "RotateCanvasDialog.h"

#include <QDoubleSpinBox>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QRadioButton>
#include <QVBoxLayout>

RotateCanvasDialog::RotateCanvasDialog(QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle(tr("Rotate Canvas"));

    auto *outer = new QHBoxLayout(this);
    auto *left = new QGridLayout;

    left->addWidget(new QLabel(tr("Angle:")), 0, 0, Qt::AlignRight);
    m_angle = new QDoubleSpinBox;
    // Past a full turn the image is only back where it started, and the sign
    // is what the radio buttons are for.
    m_angle->setRange(0.0, 359.99);
    m_angle->setDecimals(2);
    m_angle->setValue(0.0);
    m_angle->setFixedWidth(70);
    left->addWidget(m_angle, 0, 1);

    m_clockwise = new QRadioButton(tr("°CW"));
    m_clockwise->setChecked(true);
    auto *counter = new QRadioButton(tr("°CCW"));
    left->addWidget(m_clockwise, 0, 2);
    left->addWidget(counter, 1, 2);

    outer->addLayout(left, 1);

    auto *buttons = new QVBoxLayout;
    auto *ok = new QPushButton(tr("OK"));
    ok->setDefault(true);
    ok->setFixedWidth(90);
    auto *cancel = new QPushButton(tr("Cancel"));
    cancel->setFixedWidth(90);
    buttons->addWidget(ok);
    buttons->addWidget(cancel);
    buttons->addStretch();
    outer->addLayout(buttons);

    connect(ok, &QPushButton::clicked, this, &QDialog::accept);
    connect(cancel, &QPushButton::clicked, this, &QDialog::reject);

    m_angle->setFocus();
    m_angle->selectAll();
}

double RotateCanvasDialog::degreesClockwise() const
{
    const double angle = m_angle->value();
    return m_clockwise->isChecked() ? angle : -angle;
}
