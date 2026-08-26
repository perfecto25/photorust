#include "BlackWhiteDialog.h"
#include "ColorPickerDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QEvent>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

struct BWPreset {
    const char *name;
    int vals[6]; // reds, yellows, greens, cyans, blues, magentas
};

static const BWPreset kPresets[] = {
    {"Default",          {40,  60,  40,  60,  20,  80}},
    {"Blue Filter",      {25,  35,  25,  10, 300,  25}},
    {"Custom",           {40,  60,  40,  60,  20,  80}},
    {"Darker",           {20,  40,  20,  40,   0,  60}},
    {"Green Filter",     {25,  35, 300,  10,  20,  25}},
    {"High Contrast Blue Filter", {10,  0,  10,   0, 300,  10}},
    {"High Contrast Red Filter",  {300, 0,  10,   0,  10,  10}},
    {"Infrared",         {-70, 200, -50, -200, -150, 100}},
    {"Lighter",          {60,  80,  60,  80,  40, 100}},
    {"Maximum Black",    {  0,   0,   0,   0,   0,   0}},
    {"Maximum White",    {100, 100, 100, 100, 100, 100}},
    {"Neutral Density",  {40,  60,  40,  60,  20,  80}},
    {"Red Filter",       {300, 35,  25,  10,  20,  25}},
    {"Yellow Filter",    {25, 300,  25,  10,  20,  25}},
};

static constexpr int kPresetCount = static_cast<int>(std::size(kPresets));

struct ChannelEntry {
    const char *label;
    QColor swatch;
};

static const ChannelEntry kChannels[] = {
    {"Reds:",     QColor(Qt::red)},
    {"Yellows:",  QColor(Qt::yellow)},
    {"Greens:",   QColor(Qt::green)},
    {"Cyans:",    QColor(Qt::cyan)},
    {"Blues:",     QColor(Qt::blue)},
    {"Magentas:", QColor(Qt::magenta)},
};

BlackWhiteDialog::BlackWhiteDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Black and White"));
    setFixedSize(480, 480);

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
    m_presetCombo->setMinimumWidth(180);
    presetRow->addWidget(m_presetCombo, 1);
    left->addLayout(presetRow);

    left->addSpacing(4);

    // Six colour sliders
    auto *grid = new QGridLayout;
    grid->setHorizontalSpacing(6);
    grid->setVerticalSpacing(2);

    for (int i = 0; i < 6; ++i) {
        grid->addWidget(new QLabel(tr(kChannels[i].label)), i * 2, 0, Qt::AlignRight);

        auto *swatch = new QLabel;
        swatch->setFixedSize(14, 14);
        swatch->setStyleSheet(
            QStringLiteral("background-color: %1; border: 1px solid #555;")
                .arg(kChannels[i].swatch.name()));
        grid->addWidget(swatch, i * 2, 1);

        m_spins[i] = new QSpinBox;
        m_spins[i]->setRange(-200, 300);
        m_spins[i]->setValue(kPresets[0].vals[i]);
        m_spins[i]->setSuffix(QStringLiteral(" %"));
        m_spins[i]->setFixedWidth(70);
        grid->addWidget(m_spins[i], i * 2, 2);

        m_sliders[i] = new QSlider(Qt::Horizontal);
        m_sliders[i]->setRange(-200, 300);
        m_sliders[i]->setValue(kPresets[0].vals[i]);
        grid->addWidget(m_sliders[i], i * 2 + 1, 0, 1, 3);

        connect(m_sliders[i], &QSlider::valueChanged, m_spins[i], &QSpinBox::setValue);
        connect(m_spins[i], QOverload<int>::of(&QSpinBox::valueChanged),
                m_sliders[i], &QSlider::setValue);
        connect(m_sliders[i], &QSlider::valueChanged,
                this, &BlackWhiteDialog::onValueChanged);
    }

    left->addLayout(grid);

    left->addSpacing(8);

    // Tint
    auto *tintRow = new QHBoxLayout;
    m_tint = new QCheckBox(tr("Tint"));
    tintRow->addWidget(m_tint);

    m_tintSwatch = new QLabel;
    m_tintSwatch->setFixedSize(28, 18);
    m_tintSwatch->setCursor(Qt::PointingHandCursor);
    m_tintSwatch->installEventFilter(this);
    tintRow->addWidget(m_tintSwatch);

    tintRow->addStretch();
    left->addLayout(tintRow);

    // Hue slider
    auto *hueRow = new QHBoxLayout;
    hueRow->addWidget(new QLabel(tr("Hue")));
    m_hueSlider = new QSlider(Qt::Horizontal);
    m_hueSlider->setRange(0, 360);
    m_hueSlider->setValue(42);
    hueRow->addWidget(m_hueSlider, 1);
    m_hueSpin = new QSpinBox;
    m_hueSpin->setRange(0, 360);
    m_hueSpin->setValue(42);
    m_hueSpin->setSuffix(QStringLiteral("°"));
    m_hueSpin->setFixedWidth(60);
    hueRow->addWidget(m_hueSpin);
    left->addLayout(hueRow);

    // Saturation slider
    auto *satRow = new QHBoxLayout;
    satRow->addWidget(new QLabel(tr("Saturation")));
    m_satSlider = new QSlider(Qt::Horizontal);
    m_satSlider->setRange(0, 100);
    m_satSlider->setValue(20);
    satRow->addWidget(m_satSlider, 1);
    m_satSpin = new QSpinBox;
    m_satSpin->setRange(0, 100);
    m_satSpin->setValue(20);
    m_satSpin->setSuffix(QStringLiteral(" %"));
    m_satSpin->setFixedWidth(60);
    satRow->addWidget(m_satSpin);
    left->addLayout(satRow);

    updateTintSwatch();

    outer->addLayout(left, 1);

    // -- right column: buttons -------------------------------------------------
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(70);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(70);
    auto *autoBtn = new QPushButton(tr("Auto"));
    autoBtn->setFixedWidth(70);
    autoBtn->setEnabled(false);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addWidget(autoBtn);
    btnCol->addSpacing(10);
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    btnCol->addWidget(m_preview);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    // -- connections -----------------------------------------------------------
    connect(m_hueSlider, &QSlider::valueChanged, m_hueSpin, &QSpinBox::setValue);
    connect(m_hueSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_hueSlider, &QSlider::setValue);
    connect(m_satSlider, &QSlider::valueChanged, m_satSpin, &QSpinBox::setValue);
    connect(m_satSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_satSlider, &QSlider::setValue);

    connect(m_hueSlider, &QSlider::valueChanged, this, &BlackWhiteDialog::onValueChanged);
    connect(m_satSlider, &QSlider::valueChanged, this, &BlackWhiteDialog::onValueChanged);
    connect(m_hueSlider, &QSlider::valueChanged, this, &BlackWhiteDialog::updateTintSwatch);
    connect(m_satSlider, &QSlider::valueChanged, this, &BlackWhiteDialog::updateTintSwatch);
    connect(m_tint, &QCheckBox::toggled, this, [this] {
        updateTintSwatch();
        onValueChanged();
    });

    connect(m_preview, &QCheckBox::toggled, this, [this](bool checked) {
        if (checked)
            applyPreview();
        else
            revertPreview();
    });

    connect(m_presetCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &BlackWhiteDialog::applyPreset);

    connect(okBtn, &QPushButton::clicked, this, [this] {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);

    // Initial preview
    applyPreview();
}

BlackWhiteDialog::~BlackWhiteDialog()
{
    revertPreview();
}

void BlackWhiteDialog::onValueChanged()
{
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

void BlackWhiteDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    m_engine->applyBlackAndWhite(
        static_cast<float>(m_spins[0]->value()),
        static_cast<float>(m_spins[1]->value()),
        static_cast<float>(m_spins[2]->value()),
        static_cast<float>(m_spins[3]->value()),
        static_cast<float>(m_spins[4]->value()),
        static_cast<float>(m_spins[5]->value()),
        m_tint->isChecked(),
        static_cast<float>(m_hueSpin->value()),
        static_cast<float>(m_satSpin->value()));
    m_previewApplied = true;
}

void BlackWhiteDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

void BlackWhiteDialog::applyPreset(int index)
{
    const QString text = m_presetCombo->itemText(index);
    if (text.isEmpty() || text == tr("Custom"))
        return;

    for (int i = 0; i < kPresetCount; ++i) {
        if (text == QString::fromUtf8(kPresets[i].name)) {
            m_applyingPreset = true;
            for (int j = 0; j < 6; ++j)
                m_spins[j]->setValue(kPresets[i].vals[j]);
            m_applyingPreset = false;
            applyPreview();
            return;
        }
    }
}

void BlackWhiteDialog::updateTintSwatch()
{
    QColor c = QColor::fromHsv(m_hueSpin->value() % 360,
                                qBound(0, m_satSpin->value() * 255 / 100, 255),
                                220);
    if (!m_tint->isChecked())
        c = c.toRgb();

    m_tintSwatch->setStyleSheet(
        QStringLiteral("background-color: %1; border: 1px solid #555;")
            .arg(c.name()));
    m_tintSwatch->setEnabled(m_tint->isChecked());
}

void BlackWhiteDialog::openTintColorPicker()
{
    QColor initial = QColor::fromHsv(m_hueSpin->value() % 360,
                                      qBound(0, m_satSpin->value() * 255 / 100, 255),
                                      220);
    QColor picked = ColorPickerDialog::getColor(initial, this, tr("Tint Color"));
    if (!picked.isValid())
        return;

    int h = picked.hsvHue();
    if (h < 0) h = 0;
    int s = qRound(picked.hsvSaturationF() * 100.0);

    m_hueSpin->setValue(h);
    m_satSpin->setValue(s);
}

bool BlackWhiteDialog::eventFilter(QObject *obj, QEvent *event)
{
    if (obj == m_tintSwatch && event->type() == QEvent::MouseButtonRelease
        && m_tint->isChecked()) {
        openTintColorPicker();
        return true;
    }
    return QDialog::eventFilter(obj, event);
}
