#include "FillDialog.h"
#include "ColorPickerDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGridLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QStandardItemModel>
#include <QToolButton>
#include <QVBoxLayout>

static const char *kBlendModes[] = {
    "Normal", "Dissolve",
    "Darken", "Multiply", "Color Burn", "Linear Burn", "Darker Color",
    "Lighten", "Screen", "Color Dodge", "Linear Dodge (Add)", "Lighter Color",
    "Overlay", "Soft Light", "Hard Light", "Vivid Light", "Linear Light",
    "Pin Light", "Hard Mix",
    "Difference", "Exclusion", "Subtract", "Divide",
    "Hue", "Saturation", "Color", "Luminosity",
};

static constexpr int kModeCount = static_cast<int>(std::size(kBlendModes));

// Contents indices (accounting for separators):
// 0=Foreground, 1=Background, 2=Color...,
// 3=separator,
// 4=Content-Aware, 5=Pattern, 6=History,
// 7=separator,
// 8=Black, 9=50% Gray, 10=White

FillDialog::FillDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Fill"));
    setFixedSize(380, 280);

    auto *outer = new QHBoxLayout(this);

    auto *left = new QVBoxLayout;

    // Contents row
    auto *contentsRow = new QHBoxLayout;
    auto *contentsLabel = new QLabel(tr("Contents:"));
    contentsLabel->setFixedWidth(65);
    contentsLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    contentsRow->addWidget(contentsLabel);
    m_contents = new QComboBox;
    m_contents->addItem(tr("Foreground Color"));
    m_contents->addItem(tr("Background Color"));
    m_contents->addItem(tr("Color..."));
    m_contents->insertSeparator(3);
    m_contents->addItem(tr("Content-Aware"));
    m_contents->addItem(tr("Pattern"));
    m_contents->addItem(tr("History"));
    m_contents->insertSeparator(7);
    m_contents->addItem(tr("Black"));
    m_contents->addItem(tr("50% Gray"));
    m_contents->addItem(tr("White"));
    m_contents->setMinimumWidth(160);

    // Disable unimplemented options (Content-Aware and History)
    auto *model = qobject_cast<QStandardItemModel *>(m_contents->model());
    if (model) {
        for (int i : {4, 6}) {
            if (auto *item = model->item(i))
                item->setEnabled(false);
        }
    }

    contentsRow->addWidget(m_contents, 1);
    left->addLayout(contentsRow);

    left->addSpacing(6);

    // Pattern group (shown only when Pattern is selected)
    m_patternGroup = new QGroupBox(tr("Options"));
    auto *patternLayout = new QHBoxLayout(m_patternGroup);
    auto *customLabel = new QLabel(tr("Custom Pattern:"));
    patternLayout->addWidget(customLabel);
    m_patternSwatch = new QToolButton;
    m_patternSwatch->setIconSize(QSize(32, 32));
    m_patternSwatch->setFixedSize(40, 40);
    m_patternSwatch->setPopupMode(QToolButton::InstantPopup);
    patternLayout->addWidget(m_patternSwatch);
    patternLayout->addStretch();
    left->addWidget(m_patternGroup);
    m_patternGroup->setVisible(false);

    buildPatternGrid();

    // Blending group
    auto *blendGroup = new QGroupBox(tr("Blending"));
    auto *blendLayout = new QVBoxLayout(blendGroup);

    auto *modeRow = new QHBoxLayout;
    auto *modeLabel = new QLabel(tr("Mode:"));
    modeLabel->setFixedWidth(55);
    modeLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    modeRow->addWidget(modeLabel);
    m_mode = new QComboBox;
    for (int i = 0; i < kModeCount; ++i)
        m_mode->addItem(QString::fromUtf8(kBlendModes[i]));
    m_mode->setMinimumWidth(140);
    modeRow->addWidget(m_mode, 1);
    blendLayout->addLayout(modeRow);

    auto *opacityRow = new QHBoxLayout;
    auto *opacityLabel = new QLabel(tr("Opacity:"));
    opacityLabel->setFixedWidth(55);
    opacityLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    opacityRow->addWidget(opacityLabel);
    m_opacity = new QSpinBox;
    m_opacity->setRange(1, 100);
    m_opacity->setValue(100);
    m_opacity->setSuffix(QStringLiteral(" %"));
    m_opacity->setFixedWidth(65);
    opacityRow->addWidget(m_opacity);
    opacityRow->addStretch();
    blendLayout->addLayout(opacityRow);

    left->addWidget(blendGroup);

    m_preserveTransp = new QCheckBox(tr("Preserve Transparency"));
    left->addWidget(m_preserveTransp);

    left->addStretch();

    outer->addLayout(left, 1);

    // Buttons
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(70);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(70);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    connect(m_contents, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &FillDialog::onContentsChanged);

    connect(okBtn, &QPushButton::clicked, this, &QDialog::accept);
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);
}

void FillDialog::buildPatternGrid()
{
    m_patternPopup = new QWidget(this, Qt::Popup);
    auto *grid = new QGridLayout(m_patternPopup);
    grid->setSpacing(2);
    grid->setContentsMargins(4, 4, 4, 4);

    if (!m_engine)
        return;

    QString names = m_engine->patternNames();
    QStringList nameList = names.split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    const int cols = 7;

    for (int i = 0; i < nameList.size(); ++i) {
        QImage img = m_engine->patternPreview(i, 32);
        auto *btn = new QToolButton;
        btn->setIcon(QPixmap::fromImage(img));
        btn->setIconSize(QSize(32, 32));
        btn->setFixedSize(36, 36);
        btn->setToolTip(nameList[i]);
        connect(btn, &QToolButton::clicked, this, [this, i, img]() {
            m_selectedPattern = i;
            m_patternSwatch->setIcon(QPixmap::fromImage(img));
            m_patternPopup->hide();
        });
        grid->addWidget(btn, i / cols, i % cols);
    }

    // Set the initial swatch to the first pattern
    if (!nameList.isEmpty()) {
        QImage first = m_engine->patternPreview(0, 32);
        m_patternSwatch->setIcon(QPixmap::fromImage(first));
    }

    connect(m_patternSwatch, &QToolButton::clicked, this, [this]() {
        QPoint pos = m_patternSwatch->mapToGlobal(
            QPoint(0, m_patternSwatch->height()));
        m_patternPopup->move(pos);
        m_patternPopup->show();
    });
}

void FillDialog::onContentsChanged(int index)
{
    const QString text = m_contents->itemText(index);
    bool showPattern = (text == tr("Pattern"));
    m_patternGroup->setVisible(showPattern);

    if (text == tr("Color...")) {
        QColor picked = ColorPickerDialog::getColor(
            m_customColor.isValid() ? m_customColor : Qt::black,
            this, tr("Fill Color"));
        if (picked.isValid()) {
            m_customColor = picked;
        } else {
            m_contents->setCurrentIndex(0);
        }
    }
}

bool FillDialog::isPatternFill() const
{
    return m_contents->currentText() == tr("Pattern");
}

int FillDialog::selectedPatternIndex() const
{
    return m_selectedPattern;
}

QColor FillDialog::fillColor() const
{
    const QString text = m_contents->currentText();
    if (text == tr("Foreground Color"))
        return m_engine ? m_engine->foregroundColor() : Qt::black;
    if (text == tr("Background Color"))
        return m_engine ? m_engine->backgroundColor() : Qt::white;
    if (text == tr("Color..."))
        return m_customColor.isValid() ? m_customColor : Qt::black;
    if (text == tr("Black"))
        return Qt::black;
    if (text == tr("50% Gray"))
        return QColor(128, 128, 128);
    if (text == tr("White"))
        return Qt::white;
    return Qt::black;
}

int FillDialog::blendModeIndex() const
{
    return m_mode->currentIndex();
}

int FillDialog::opacity() const
{
    return m_opacity->value();
}

bool FillDialog::preserveTransparency() const
{
    return m_preserveTransp->isChecked();
}
