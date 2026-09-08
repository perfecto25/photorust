#include "WarpTextDialog.h"

#include <QButtonGroup>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QRadioButton>
#include <QSlider>
#include <QSpinBox>
#include <QVBoxLayout>

WarpTextDialog::WarpTextDialog(const TypeWarp &warp, QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle(tr("Warp Text"));

    auto *root = new QVBoxLayout(this);

    auto *styleRow = new QHBoxLayout();
    styleRow->addWidget(new QLabel(tr("Style:"), this));
    m_style = new QComboBox(this);
    // Each entry carries its style as data rather than relying on its row,
    // since the separators CS6 groups the menu with occupy rows of their own.
    const QStringList names = TypeWarp::styleNames();
    for (int style = 0; style < names.size(); ++style) {
        if (TypeWarp::startsGroup(style)) {
            m_style->insertSeparator(m_style->count());
        }
        m_style->addItem(names.at(style), style);
    }
    styleRow->addWidget(m_style, 1);
    root->addLayout(styleRow);

    auto *orientation = new QHBoxLayout();
    m_horizontal = new QRadioButton(tr("Horizontal"), this);
    m_vertical = new QRadioButton(tr("Vertical"), this);
    auto *group = new QButtonGroup(this);
    group->addButton(m_horizontal);
    group->addButton(m_vertical);
    orientation->addWidget(m_horizontal);
    orientation->addWidget(m_vertical);
    orientation->addStretch(1);
    root->addLayout(orientation);

    auto *grid = new QGridLayout();
    auto addAmount = [&](int row, const QString &label, QSpinBox *&box, QSlider *&slider) {
        grid->addWidget(new QLabel(label, this), row, 0);
        box = new QSpinBox(this);
        box->setRange(-100, 100);
        box->setSuffix(QStringLiteral(" %"));
        grid->addWidget(box, row, 1);
        slider = new QSlider(Qt::Horizontal, this);
        slider->setRange(-100, 100);
        grid->addWidget(slider, row + 1, 0, 1, 2);
    };
    addAmount(0, tr("Bend:"), m_bend, m_bendSlider);
    addAmount(2, tr("Horizontal Distortion:"), m_hDistort, m_hSlider);
    addAmount(4, tr("Vertical Distortion:"), m_vDistort, m_vSlider);
    root->addLayout(grid);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    root->addWidget(buttons);

    // -- initial state --------------------------------------------------------
    m_updating = true;
    const int styleRow_ = m_style->findData(warp.style());
    m_style->setCurrentIndex(styleRow_ >= 0 ? styleRow_ : 0);
    (warp.horizontal() ? m_horizontal : m_vertical)->setChecked(true);
    m_bend->setValue(qRound(warp.bend() * 100.0));
    m_hDistort->setValue(qRound(warp.horizontalDistortion() * 100.0));
    m_vDistort->setValue(qRound(warp.verticalDistortion() * 100.0));
    m_bendSlider->setValue(m_bend->value());
    m_hSlider->setValue(m_hDistort->value());
    m_vSlider->setValue(m_vDistort->value());
    m_updating = false;
    refreshOrientationEnabled();

    // -- wiring ---------------------------------------------------------------
    for (auto [box, slider] : {std::pair{m_bend, m_bendSlider},
                               std::pair{m_hDistort, m_hSlider},
                               std::pair{m_vDistort, m_vSlider}}) {
        connect(slider, &QSlider::valueChanged, box, &QSpinBox::setValue);
        connect(box, &QSpinBox::valueChanged, slider, &QSlider::setValue);
        connect(box, &QSpinBox::valueChanged, this, [this] { announce(); });
    }
    connect(m_style, &QComboBox::currentIndexChanged, this, [this] {
        refreshOrientationEnabled();
        announce();
    });
    connect(m_horizontal, &QRadioButton::toggled, this, [this] { announce(); });
}

void WarpTextDialog::refreshOrientationEnabled()
{
    const bool usable = TypeWarp::hasOrientation(m_style->currentData().toInt());
    m_horizontal->setEnabled(usable);
    m_vertical->setEnabled(usable);
}

void WarpTextDialog::announce()
{
    if (!m_updating) {
        emit warpChanged(warp());
    }
}

TypeWarp WarpTextDialog::warp() const
{
    return TypeWarp(m_style->currentData().toInt(), m_horizontal->isChecked(),
                    m_bend->value() / 100.0, m_hDistort->value() / 100.0,
                    m_vDistort->value() / 100.0);
}
