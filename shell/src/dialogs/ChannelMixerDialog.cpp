#include "ChannelMixerDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGridLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QSignalBlocker>
#include <QVBoxLayout>

struct CMPreset {
    const char *name;
    int matrix[3][3]; // [output][source]
    int constants[3];
    bool monochrome;
};

static const CMPreset kPresets[] = {
    {"Default",
     {{100, 0, 0}, {0, 100, 0}, {0, 0, 100}},
     {0, 0, 0}, false},
    {"Black & White Infrared (RGB)",
     {{-70, 200, -30}, {-70, 200, -30}, {-70, 200, -30}},
     {0, 0, 0}, true},
    {"Black & White with Blue Filter (RGB)",
     {{0, 0, 100}, {0, 0, 100}, {0, 0, 100}},
     {0, 0, 0}, true},
    {"Black & White with Green Filter (RGB)",
     {{0, 100, 0}, {0, 100, 0}, {0, 100, 0}},
     {0, 0, 0}, true},
    {"Black & White with Orange Filter (RGB)",
     {{50, 50, 0}, {50, 50, 0}, {50, 50, 0}},
     {0, 0, 0}, true},
    {"Black & White with Red Filter (RGB)",
     {{100, 0, 0}, {100, 0, 0}, {100, 0, 0}},
     {0, 0, 0}, true},
    {"Black & White with Yellow Filter (RGB)",
     {{34, 66, 0}, {34, 66, 0}, {34, 66, 0}},
     {0, 0, 0}, true},
    {"Custom",
     {{100, 0, 0}, {0, 100, 0}, {0, 0, 100}},
     {0, 0, 0}, false},
};

static constexpr int kPresetCount = static_cast<int>(std::size(kPresets));

static const char *kSourceLabels[] = {"Red:", "Green:", "Blue:"};
static const QColor kSourceColors[] = {
    QColor(220, 50, 50), QColor(50, 180, 50), QColor(80, 80, 220)
};

ChannelMixerDialog::ChannelMixerDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Channel Mixer"));
    setFixedSize(460, 420);

    // Initialize to identity
    for (int i = 0; i < 3; ++i) {
        for (int j = 0; j < 3; ++j)
            m_matrix[i][j] = (i == j) ? 100 : 0;
        m_constants[i] = 0;
    }

    auto *outer = new QHBoxLayout(this);

    // -- left column -----------------------------------------------------------
    auto *left = new QVBoxLayout;

    // Preset row
    auto *presetRow = new QHBoxLayout;
    presetRow->addWidget(new QLabel(tr("Preset:")));
    m_presetCombo = new QComboBox;
    m_presetCombo->addItem(tr("Default"));
    m_presetCombo->insertSeparator(1);
    for (int i = 1; i < kPresetCount; ++i)
        m_presetCombo->addItem(QString::fromUtf8(kPresets[i].name));
    m_presetCombo->setMinimumWidth(200);
    presetRow->addWidget(m_presetCombo, 1);
    left->addLayout(presetRow);

    // Output Channel row
    auto *outputRow = new QHBoxLayout;
    auto *outputLabel = new QLabel(tr("Output Channel:"));
    outputRow->addWidget(outputLabel);
    m_outputCombo = new QComboBox;
    m_outputCombo->addItem(tr("Red"));
    m_outputCombo->addItem(tr("Green"));
    m_outputCombo->addItem(tr("Blue"));
    m_outputCombo->setMinimumWidth(100);
    outputRow->addWidget(m_outputCombo);
    outputRow->addStretch();
    left->addLayout(outputRow);

    left->addSpacing(2);

    // Source Channels group
    auto *srcGroup = new QGroupBox(tr("Source Channels"));
    auto *srcLayout = new QVBoxLayout(srcGroup);

    for (int i = 0; i < 3; ++i) {
        auto *row = new QHBoxLayout;
        auto *label = new QLabel(tr(kSourceLabels[i]));
        label->setFixedWidth(50);
        label->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
        row->addWidget(label);

        m_spins[i] = new QSpinBox;
        m_spins[i]->setRange(-200, 200);
        m_spins[i]->setValue(0);
        m_spins[i]->setPrefix(QStringLiteral("+"));
        m_spins[i]->setSuffix(QStringLiteral(" %"));
        m_spins[i]->setFixedWidth(70);
        row->addWidget(m_spins[i]);

        srcLayout->addLayout(row);

        m_sliders[i] = new QSlider(Qt::Horizontal);
        m_sliders[i]->setRange(-200, 200);
        m_sliders[i]->setValue(0);

        // Coloured groove
        QString groove = QStringLiteral(
            "QSlider::groove:horizontal { height: 4px; background: "
            "qlineargradient(x1:0, y1:0, x2:1, y2:0, stop:0 #333, stop:1 %1); }")
            .arg(kSourceColors[i].name());
        m_sliders[i]->setStyleSheet(groove);

        srcLayout->addWidget(m_sliders[i]);

        connect(m_sliders[i], &QSlider::valueChanged, this, [this, i](int v) {
            if (m_updatingUi) return;
            QSignalBlocker b(m_spins[i]);
            m_spins[i]->setValue(v);
            saveUiToChannel();
            updateTotal();
            onValueChanged();
        });
        connect(m_spins[i], QOverload<int>::of(&QSpinBox::valueChanged), this, [this, i](int v) {
            if (m_updatingUi) return;
            QSignalBlocker b(m_sliders[i]);
            m_sliders[i]->setValue(v);
            saveUiToChannel();
            updateTotal();
            onValueChanged();
        });
    }

    // Total row
    auto *totalRow = new QHBoxLayout;
    totalRow->addStretch();
    auto *totalLabelFixed = new QLabel(tr("Total:"));
    totalLabelFixed->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    totalRow->addWidget(totalLabelFixed);
    m_totalLabel = new QLabel(QStringLiteral("+100 %"));
    m_totalLabel->setFixedWidth(60);
    m_totalLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    totalRow->addWidget(m_totalLabel);
    srcLayout->addLayout(totalRow);

    left->addWidget(srcGroup);

    left->addSpacing(4);

    // Constant row
    auto *constRow = new QHBoxLayout;
    auto *constLabel = new QLabel(tr("Constant:"));
    constLabel->setFixedWidth(60);
    constLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    constRow->addWidget(constLabel);
    m_constSpin = new QSpinBox;
    m_constSpin->setRange(-200, 200);
    m_constSpin->setValue(0);
    m_constSpin->setSuffix(QStringLiteral(" %"));
    m_constSpin->setFixedWidth(70);
    constRow->addWidget(m_constSpin);
    constRow->addStretch();
    left->addLayout(constRow);

    m_constSlider = new QSlider(Qt::Horizontal);
    m_constSlider->setRange(-200, 200);
    m_constSlider->setValue(0);
    left->addWidget(m_constSlider);

    connect(m_constSlider, &QSlider::valueChanged, this, [this](int v) {
        if (m_updatingUi) return;
        QSignalBlocker b(m_constSpin);
        m_constSpin->setValue(v);
        m_constants[m_currentOutput] = v;
        onValueChanged();
    });
    connect(m_constSpin, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int v) {
        if (m_updatingUi) return;
        QSignalBlocker b(m_constSlider);
        m_constSlider->setValue(v);
        m_constants[m_currentOutput] = v;
        onValueChanged();
    });

    left->addSpacing(4);

    // Monochrome
    m_monochrome = new QCheckBox(tr("Monochrome"));
    left->addWidget(m_monochrome);

    left->addStretch();

    outer->addLayout(left, 1);

    // -- right column: buttons -------------------------------------------------
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(70);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(70);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addSpacing(10);
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    btnCol->addWidget(m_preview);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    // -- connections -----------------------------------------------------------
    connect(m_outputCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &ChannelMixerDialog::onOutputChannelChanged);
    connect(m_monochrome, &QCheckBox::toggled,
            this, &ChannelMixerDialog::onMonochromeToggled);
    connect(m_presetCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &ChannelMixerDialog::applyPreset);

    connect(m_preview, &QCheckBox::toggled, this, [this](bool checked) {
        if (checked)
            applyPreview();
        else
            revertPreview();
    });

    connect(okBtn, &QPushButton::clicked, this, [this] {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);

    // Initialize UI from matrix
    loadChannelToUi();
    applyPreview();
}

ChannelMixerDialog::~ChannelMixerDialog()
{
    revertPreview();
}

void ChannelMixerDialog::loadChannelToUi()
{
    m_updatingUi = true;
    int ch = m_currentOutput;
    for (int i = 0; i < 3; ++i) {
        m_spins[i]->setValue(m_matrix[ch][i]);
        m_sliders[i]->setValue(m_matrix[ch][i]);
    }
    m_constSpin->setValue(m_constants[ch]);
    m_constSlider->setValue(m_constants[ch]);
    m_updatingUi = false;
    updateTotal();
}

void ChannelMixerDialog::saveUiToChannel()
{
    int ch = m_currentOutput;
    for (int i = 0; i < 3; ++i)
        m_matrix[ch][i] = m_spins[i]->value();
}

void ChannelMixerDialog::onOutputChannelChanged(int index)
{
    if (index < 0) return;
    saveUiToChannel();
    m_constants[m_currentOutput] = m_constSpin->value();
    m_currentOutput = index;
    loadChannelToUi();
}

void ChannelMixerDialog::onMonochromeToggled(bool checked)
{
    if (checked) {
        // Save current channel, switch output to "Gray"
        saveUiToChannel();
        m_constants[m_currentOutput] = m_constSpin->value();

        m_outputCombo->blockSignals(true);
        m_outputCombo->clear();
        m_outputCombo->addItem(tr("Gray"));
        m_outputCombo->blockSignals(false);
        m_currentOutput = 0;

        // In mono mode, all three output rows use the same weights
        // Copy current Red row as the mono row if it's identity
        loadChannelToUi();
    } else {
        m_outputCombo->blockSignals(true);
        m_outputCombo->clear();
        m_outputCombo->addItem(tr("Red"));
        m_outputCombo->addItem(tr("Green"));
        m_outputCombo->addItem(tr("Blue"));
        m_outputCombo->blockSignals(false);
        m_currentOutput = 0;
        loadChannelToUi();
    }
    onValueChanged();
}

void ChannelMixerDialog::onValueChanged()
{
    if (m_updatingUi) return;
    if (!m_applyingPreset) {
        for (int i = 0; i < m_presetCombo->count(); ++i) {
            if (m_presetCombo->itemText(i) == tr("Custom")) {
                m_presetCombo->blockSignals(true);
                m_presetCombo->setCurrentIndex(i);
                m_presetCombo->blockSignals(false);
                break;
            }
        }
    }
    applyPreview();
}

void ChannelMixerDialog::updateTotal()
{
    int ch = m_currentOutput;
    int total = m_matrix[ch][0] + m_matrix[ch][1] + m_matrix[ch][2];
    QString sign = total >= 0 ? QStringLiteral("+") : QString();
    m_totalLabel->setText(QStringLiteral("%1%2 %").arg(sign).arg(total));
}

void ChannelMixerDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    // Save current UI to matrix
    saveUiToChannel();
    m_constants[m_currentOutput] = m_constSpin->value();

    bool mono = m_monochrome->isChecked();

    if (mono) {
        // In monochrome mode, all output channels use the first row
        m_engine->applyChannelMixer(
            static_cast<float>(m_matrix[0][0]),
            static_cast<float>(m_matrix[0][1]),
            static_cast<float>(m_matrix[0][2]),
            static_cast<float>(m_matrix[0][0]),
            static_cast<float>(m_matrix[0][1]),
            static_cast<float>(m_matrix[0][2]),
            static_cast<float>(m_matrix[0][0]),
            static_cast<float>(m_matrix[0][1]),
            static_cast<float>(m_matrix[0][2]),
            static_cast<float>(m_constants[0]),
            static_cast<float>(m_constants[0]),
            static_cast<float>(m_constants[0]),
            true);
    } else {
        m_engine->applyChannelMixer(
            static_cast<float>(m_matrix[0][0]),
            static_cast<float>(m_matrix[0][1]),
            static_cast<float>(m_matrix[0][2]),
            static_cast<float>(m_matrix[1][0]),
            static_cast<float>(m_matrix[1][1]),
            static_cast<float>(m_matrix[1][2]),
            static_cast<float>(m_matrix[2][0]),
            static_cast<float>(m_matrix[2][1]),
            static_cast<float>(m_matrix[2][2]),
            static_cast<float>(m_constants[0]),
            static_cast<float>(m_constants[1]),
            static_cast<float>(m_constants[2]),
            false);
    }
    m_previewApplied = true;
}

void ChannelMixerDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

void ChannelMixerDialog::applyPreset(int index)
{
    const QString text = m_presetCombo->itemText(index);
    if (text.isEmpty() || text == tr("Custom"))
        return;

    for (int p = 0; p < kPresetCount; ++p) {
        if (text == QString::fromUtf8(kPresets[p].name)) {
            m_applyingPreset = true;

            // Set monochrome first (it rebuilds the output combo)
            if (m_monochrome->isChecked() != kPresets[p].monochrome)
                m_monochrome->setChecked(kPresets[p].monochrome);

            for (int i = 0; i < 3; ++i) {
                for (int j = 0; j < 3; ++j)
                    m_matrix[i][j] = kPresets[p].matrix[i][j];
                m_constants[i] = kPresets[p].constants[i];
            }

            m_currentOutput = 0;
            m_outputCombo->blockSignals(true);
            m_outputCombo->setCurrentIndex(0);
            m_outputCombo->blockSignals(false);
            loadChannelToUi();

            m_applyingPreset = false;
            applyPreview();
            return;
        }
    }
}
