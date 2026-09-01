#include "TrimDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QCheckBox>
#include <QGridLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QPushButton>
#include <QRadioButton>
#include <QVBoxLayout>

TrimDialog::TrimDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle(tr("Trim"));

    auto *outer = new QHBoxLayout(this);
    auto *left = new QVBoxLayout;

    // --- Based On ---------------------------------------------------------
    auto *basisBox = new QGroupBox(tr("Based On"));
    auto *basisLayout = new QVBoxLayout(basisBox);
    m_transparent = new QRadioButton(tr("Transparent Pixels"));
    m_topLeft = new QRadioButton(tr("Top Left Pixel Color"));
    m_bottomRight = new QRadioButton(tr("Bottom Right Pixel Color"));
    basisLayout->addWidget(m_transparent);
    basisLayout->addWidget(m_topLeft);
    basisLayout->addWidget(m_bottomRight);
    left->addWidget(basisBox);

    // Nothing to trim on transparency in a flat image, so CS6 greys the option
    // out rather than offering a choice that would do nothing.
    const bool transparency = engine && engine->imageHasTransparency();
    m_transparent->setEnabled(transparency);
    if (transparency) {
        m_transparent->setChecked(true);
    } else {
        m_topLeft->setChecked(true);
    }

    // --- Trim Away --------------------------------------------------------
    auto *edgeBox = new QGroupBox(tr("Trim Away"));
    auto *edgeGrid = new QGridLayout(edgeBox);
    m_top = new QCheckBox(tr("Top"));
    m_bottom = new QCheckBox(tr("Bottom"));
    m_left = new QCheckBox(tr("Left"));
    m_right = new QCheckBox(tr("Right"));
    for (QCheckBox *edge : {m_top, m_bottom, m_left, m_right}) {
        edge->setChecked(true);
    }
    edgeGrid->addWidget(m_top, 0, 0);
    edgeGrid->addWidget(m_left, 0, 1);
    edgeGrid->addWidget(m_bottom, 1, 0);
    edgeGrid->addWidget(m_right, 1, 1);
    left->addWidget(edgeBox);

    left->addStretch();
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
}

int TrimDialog::basis() const
{
    if (m_topLeft->isChecked()) {
        return 1;
    }
    if (m_bottomRight->isChecked()) {
        return 2;
    }
    return 0;
}

bool TrimDialog::trimTop() const
{
    return m_top->isChecked();
}

bool TrimDialog::trimBottom() const
{
    return m_bottom->isChecked();
}

bool TrimDialog::trimLeft() const
{
    return m_left->isChecked();
}

bool TrimDialog::trimRight() const
{
    return m_right->isChecked();
}
