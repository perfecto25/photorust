#include "IndexedColorDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QBoxLayout>
#include <QGridLayout>
#include <QGroupBox>
#include <QLabel>
#include <QPushButton>

IndexedColorDialog::IndexedColorDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Indexed Color"));
    setFixedSize(420, 320);

    auto *outer = new QHBoxLayout(this);

    // -- left: form ---------------------------------------------------------
    auto *leftCol = new QVBoxLayout;

    auto *formGrid = new QGridLayout;
    formGrid->setColumnStretch(1, 1);

    // Palette
    formGrid->addWidget(new QLabel(tr("Palette:")), 0, 0, Qt::AlignRight);
    m_palette = new QComboBox;
    m_palette->addItem(tr("Exact"));
    m_palette->addItem(tr("System (Mac OS)"));
    m_palette->addItem(tr("System (Windows)"));
    m_palette->addItem(tr("Web"));
    m_palette->addItem(tr("Uniform"));
    m_palette->insertSeparator(5);
    m_palette->addItem(tr("Local (Perceptual)"));
    m_palette->addItem(tr("Local (Selective)"));
    m_palette->addItem(tr("Local (Adaptive)"));
    m_palette->insertSeparator(9);
    m_palette->addItem(tr("Master (Perceptual)"));
    m_palette->addItem(tr("Master (Selective)"));
    m_palette->addItem(tr("Master (Adaptive)"));
    m_palette->insertSeparator(13);
    m_palette->addItem(tr("Custom..."));
    m_palette->addItem(tr("Previous"));
    m_palette->setCurrentIndex(7); // Local (Selective)
    formGrid->addWidget(m_palette, 0, 1);

    // Colors
    formGrid->addWidget(new QLabel(tr("Colors:")), 1, 0, Qt::AlignRight);
    m_colors = new QSpinBox;
    m_colors->setRange(2, 256);
    m_colors->setValue(256);
    formGrid->addWidget(m_colors, 1, 1);

    // Forced
    formGrid->addWidget(new QLabel(tr("Forced:")), 2, 0, Qt::AlignRight);
    m_forced = new QComboBox;
    m_forced->addItem(tr("None"));
    m_forced->insertSeparator(1);
    m_forced->addItem(tr("Black and White"));
    m_forced->addItem(tr("Primaries"));
    m_forced->addItem(tr("Web"));
    m_forced->insertSeparator(5);
    m_forced->addItem(tr("Custom..."));
    m_forced->setCurrentIndex(2); // Black and White
    formGrid->addWidget(m_forced, 2, 1);

    // Transparency
    m_transparency = new QCheckBox(tr("Transparency"));
    m_transparency->setChecked(true);
    formGrid->addWidget(m_transparency, 3, 1);

    leftCol->addLayout(formGrid);

    // -- Options group ------------------------------------------------------
    auto *optGroup = new QGroupBox(tr("Options"));
    auto *optGrid = new QGridLayout(optGroup);
    optGrid->setColumnStretch(1, 1);

    // Matte
    optGrid->addWidget(new QLabel(tr("Matte:")), 0, 0, Qt::AlignRight);
    m_matte = new QComboBox;
    m_matte->addItem(tr("None"));
    m_matte->addItem(tr("Foreground Color"));
    m_matte->addItem(tr("Background Color"));
    m_matte->addItem(tr("White"));
    m_matte->addItem(tr("Black"));
    m_matte->addItem(tr("50% Gray"));
    m_matte->addItem(tr("Custom..."));
    m_matte->setEnabled(false);
    optGrid->addWidget(m_matte, 0, 1);

    // Dither
    optGrid->addWidget(new QLabel(tr("Dither:")), 1, 0, Qt::AlignRight);
    m_dither = new QComboBox;
    m_dither->addItem(tr("None"));
    m_dither->addItem(tr("Diffusion"));
    m_dither->addItem(tr("Pattern"));
    m_dither->addItem(tr("Noise"));
    m_dither->setCurrentIndex(1); // Diffusion
    optGrid->addWidget(m_dither, 1, 1);

    // Amount
    auto *amountRow = new QHBoxLayout;
    optGrid->addWidget(new QLabel(tr("Amount:")), 2, 0, Qt::AlignRight);
    m_amount = new QSpinBox;
    m_amount->setRange(1, 100);
    m_amount->setValue(75);
    m_amount->setSuffix(tr(" %"));
    amountRow->addWidget(m_amount);
    amountRow->addStretch();
    optGrid->addLayout(amountRow, 2, 1);

    // Preserve Exact Colors
    m_preserveExact = new QCheckBox(tr("Preserve Exact Colors"));
    optGrid->addWidget(m_preserveExact, 3, 0, 1, 2);

    leftCol->addWidget(optGroup);
    leftCol->addStretch();

    outer->addLayout(leftCol, 1);

    // -- right: buttons -----------------------------------------------------
    auto *btnCol = new QVBoxLayout;
    btnCol->setSpacing(6);

    auto *okBtn = new QPushButton(tr("OK"));
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);

    okBtn->setDefault(true);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addSpacing(8);
    btnCol->addWidget(m_preview);
    btnCol->addStretch();

    outer->addLayout(btnCol);

    // -- wiring -------------------------------------------------------------
    connect(m_palette, &QComboBox::currentIndexChanged,
            this, &IndexedColorDialog::onPaletteChanged);
    connect(m_dither, &QComboBox::currentIndexChanged,
            this, &IndexedColorDialog::onDitherChanged);
    connect(m_transparency, &QCheckBox::toggled, this, [this](bool on) {
        m_matte->setEnabled(!on);
    });

    // Preview wiring: any setting change re-applies when Preview is checked
    auto settingChanged = [this] { applyPreview(); };
    connect(m_colors, &QSpinBox::valueChanged, this, settingChanged);
    connect(m_dither, &QComboBox::currentIndexChanged, this, settingChanged);
    connect(m_amount, &QSpinBox::valueChanged, this, settingChanged);
    connect(m_preview, &QCheckBox::toggled, this, [this](bool on) {
        if (on)
            applyPreview();
        else
            revertPreview();
    });

    connect(okBtn, &QPushButton::clicked, this, &QDialog::accept);
    connect(cancelBtn, &QPushButton::clicked, this, [this] {
        revertPreview();
        reject();
    });

    // Apply initial preview
    applyPreview();
}

IndexedColorDialog::~IndexedColorDialog()
{
    // If the dialog is destroyed without accept (e.g. window close button),
    // revert any outstanding preview.
    revertPreview();
}

int IndexedColorDialog::colors() const
{
    return m_colors->value();
}

int IndexedColorDialog::ditherIndex() const
{
    return m_dither->currentIndex();
}

int IndexedColorDialog::ditherAmount() const
{
    return m_amount->value();
}

void IndexedColorDialog::onPaletteChanged(int index)
{
    Q_UNUSED(index)
    const QString text = m_palette->currentText();
    const bool isExact = text == tr("Exact");
    const bool isFixed = text == tr("Web") ||
                         text.startsWith(tr("System"));
    m_colors->setEnabled(!isExact && !isFixed);
    if (isFixed || isExact)
        m_colors->setValue(256);
}

void IndexedColorDialog::onDitherChanged(int index)
{
    const bool isDiffusion = (index == 1);
    m_amount->setEnabled(isDiffusion);
    m_preserveExact->setEnabled(isDiffusion);
}

void IndexedColorDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    // Undo any previous preview first so we always convert from the original
    revertPreview();

    const int ditherIdx = m_dither->currentIndex();
    const int amount = (ditherIdx == 1) ? m_amount->value() : 0;
    m_engine->convertToIndexed(m_colors->value(), amount);
    m_previewApplied = true;
}

void IndexedColorDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}
