#include "LevelsDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLinearGradient>
#include <QPainter>
#include <QPainterPath>
#include <QPushButton>
#include <QVBoxLayout>

// ---------------------------------------------------------------------------
// HistogramWidget
// ---------------------------------------------------------------------------

HistogramWidget::HistogramWidget(QWidget *parent)
    : QWidget(parent)
{
    setFixedSize(256, 100);
    std::fill(std::begin(m_histogram), std::end(m_histogram), 0);
}

void HistogramWidget::setImage(const QImage &img, int channel)
{
    std::fill(std::begin(m_histogram), std::end(m_histogram), 0);

    const QImage src = img.convertToFormat(QImage::Format_ARGB32);
    for (int y = 0; y < src.height(); ++y) {
        const auto *line = reinterpret_cast<const QRgb *>(src.constScanLine(y));
        for (int x = 0; x < src.width(); ++x) {
            const QRgb px = line[x];
            int val = 0;
            switch (channel) {
            case 0: val = qGray(px); break;
            case 1: val = qRed(px); break;
            case 2: val = qGreen(px); break;
            case 3: val = qBlue(px); break;
            }
            m_histogram[val]++;
        }
    }
    m_peak = 1;
    for (int i = 0; i < 256; ++i)
        m_peak = qMax(m_peak, m_histogram[i]);
    update();
}

void HistogramWidget::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    p.fillRect(rect(), Qt::white);

    p.setPen(Qt::black);
    const int h = height();
    for (int i = 0; i < 256; ++i) {
        const int barH = static_cast<int>(
            static_cast<double>(m_histogram[i]) / m_peak * h);
        if (barH > 0)
            p.drawLine(i, h, i, h - barH);
    }

    p.setPen(QColor(180, 180, 180));
    p.drawRect(rect().adjusted(0, 0, -1, -1));
}

// ---------------------------------------------------------------------------
// TriangleSlider — draggable triangle thumbs below a bar
// ---------------------------------------------------------------------------

TriangleSlider::TriangleSlider(int count, int globalMin, int globalMax, QWidget *parent)
    : QWidget(parent)
    , m_globalMin(globalMin)
    , m_globalMax(globalMax)
{
    setFixedHeight(kThumbH + 2);
    for (int i = 0; i < count; ++i)
        m_thumbs.append({globalMin, globalMax, globalMin, Qt::black});
}

void TriangleSlider::setRange(int index, int min, int max)
{
    if (index < 0 || index >= m_thumbs.size()) return;
    if (max < min) max = min;
    m_thumbs[index].min = min;
    m_thumbs[index].max = max;
    m_thumbs[index].val = qBound(min, m_thumbs[index].val, max);
    update();
}

void TriangleSlider::setValue(int index, int val)
{
    if (index < 0 || index >= m_thumbs.size()) return;
    int lo = m_thumbs[index].min;
    int hi = m_thumbs[index].max;
    if (hi < lo) hi = lo;
    val = qBound(lo, val, hi);
    if (m_thumbs[index].val != val) {
        m_thumbs[index].val = val;
        update();
    }
}

void TriangleSlider::setColor(int index, const QColor &c)
{
    if (index < 0 || index >= m_thumbs.size()) return;
    m_thumbs[index].color = c;
    update();
}

int TriangleSlider::value(int index) const
{
    if (index < 0 || index >= m_thumbs.size()) return 0;
    return m_thumbs[index].val;
}

int TriangleSlider::xForValue(int index) const
{
    const auto &t = m_thumbs[index];
    const int span = m_globalMax - m_globalMin;
    const int usable = width() - 2 * kMargin;
    if (span <= 0 || usable <= 0) return kMargin;
    return kMargin + (t.val - m_globalMin) * usable / span;
}

int TriangleSlider::valueForX(int index, int x) const
{
    const auto &t = m_thumbs[index];
    const int span = m_globalMax - m_globalMin;
    const int usable = width() - 2 * kMargin;
    if (usable <= 0 || span <= 0) return t.min;
    int raw = m_globalMin + (x - kMargin) * span / usable;
    int lo = t.min, hi = t.max;
    if (hi < lo) hi = lo;
    return qBound(lo, raw, hi);
}

void TriangleSlider::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing);
    for (int i = 0; i < m_thumbs.size(); ++i) {
        const int cx = xForValue(i);
        QPainterPath path;
        path.moveTo(cx, 0);
        path.lineTo(cx - 5, kThumbH);
        path.lineTo(cx + 5, kThumbH);
        path.closeSubpath();
        p.setBrush(m_thumbs[i].color);
        p.setPen(QPen(QColor(80, 80, 80), 1));
        p.drawPath(path);
    }
}

void TriangleSlider::mousePressEvent(QMouseEvent *e)
{
    m_dragging = -1;
    int bestDist = 999;
    for (int i = 0; i < m_thumbs.size(); ++i) {
        int d = qAbs(e->pos().x() - xForValue(i));
        if (d < bestDist && d < 12) {
            bestDist = d;
            m_dragging = i;
        }
    }
    if (m_dragging >= 0)
        mouseMoveEvent(e);
}

void TriangleSlider::mouseMoveEvent(QMouseEvent *e)
{
    if (m_dragging < 0) return;
    int v = valueForX(m_dragging, e->pos().x());
    if (v != m_thumbs[m_dragging].val) {
        m_thumbs[m_dragging].val = v;
        update();
        emit valueChanged(m_dragging, v);
    }
}

void TriangleSlider::mouseReleaseEvent(QMouseEvent *)
{
    m_dragging = -1;
}

// ---------------------------------------------------------------------------
// LevelsDialog
// ---------------------------------------------------------------------------

struct LevelsPreset {
    const char *name;
    int inBlack;
    int inWhite;
    double gamma;
    int outBlack;
    int outWhite;
};

static const LevelsPreset kPresets[] = {
    {"Default",              0, 255, 1.00,   0, 255},
    {"Darker",               0, 255, 0.75,   0, 255},
    {"Increase Contrast 1",  5, 250, 1.00,   0, 255},
    {"Increase Contrast 2", 10, 245, 1.00,   0, 255},
    {"Increase Contrast 3", 15, 240, 1.00,   0, 255},
    {"Lighten Shadows",      0, 255, 1.50,   0, 255},
    {"Lighter",              0, 255, 1.50,   0, 255},
    {"Midtones Brighter",    0, 255, 1.25,   0, 255},
    {"Midtones Darker",      0, 255, 0.75,   0, 255},
    {"Custom",               0, 255, 1.00,   0, 255},
};
static constexpr int kPresetCount = sizeof(kPresets) / sizeof(kPresets[0]);
static constexpr int kCustomIndex = kPresetCount - 1;

LevelsDialog::LevelsDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Levels"));
    setFixedSize(470, 370);

    if (m_engine)
        m_originalImage = m_engine->compositeImage();

    auto *outer = new QHBoxLayout(this);

    // -- left column -----------------------------------------------------------
    auto *left = new QVBoxLayout;

    // Preset row
    auto *presetRow = new QHBoxLayout;
    presetRow->addWidget(new QLabel(tr("Preset:")));
    m_presetCombo = new QComboBox;
    for (int i = 0; i < kPresetCount; ++i) {
        m_presetCombo->addItem(QString::fromUtf8(kPresets[i].name));
        if (i == 0) m_presetCombo->insertSeparator(1);
        if (i == kPresetCount - 2) m_presetCombo->insertSeparator(m_presetCombo->count());
    }
    m_presetCombo->setMinimumWidth(180);
    presetRow->addWidget(m_presetCombo, 1);
    left->addLayout(presetRow);

    // Channel selector (indented like CS6)
    auto *channelRow = new QHBoxLayout;
    channelRow->addSpacing(20);
    channelRow->addWidget(new QLabel(tr("Channel:")));
    m_channelCombo = new QComboBox;
    m_channelCombo->addItem(tr("RGB"));
    m_channelCombo->addItem(tr("Red"));
    m_channelCombo->addItem(tr("Green"));
    m_channelCombo->addItem(tr("Blue"));
    m_channelCombo->setMinimumWidth(120);
    channelRow->addWidget(m_channelCombo, 1);
    left->addLayout(channelRow);

    // Input Levels label
    left->addWidget(new QLabel(tr("Input Levels:")));

    // Histogram
    m_histogram = new HistogramWidget;
    left->addWidget(m_histogram);

    // Input triangle slider (black=0, gray=1, white=2)
    m_inputSlider = new TriangleSlider(3, 0, 255);
    m_inputSlider->setFixedWidth(256);
    m_inputSlider->setRange(0, 0, 253);
    m_inputSlider->setValue(0, 0);
    m_inputSlider->setColor(0, Qt::black);
    m_inputSlider->setRange(1, 0, 255);
    m_inputSlider->setValue(1, 128);
    m_inputSlider->setColor(1, QColor(128, 128, 128));
    m_inputSlider->setRange(2, 2, 255);
    m_inputSlider->setValue(2, 255);
    m_inputSlider->setColor(2, Qt::white);
    left->addWidget(m_inputSlider);

    // Input level spinboxes
    auto *inputRow = new QHBoxLayout;
    m_inBlack = new QSpinBox;
    m_inBlack->setRange(0, 253);
    m_inBlack->setValue(0);
    m_inBlack->setFixedWidth(50);
    inputRow->addWidget(m_inBlack);

    inputRow->addStretch();
    m_gamma = new QDoubleSpinBox;
    m_gamma->setRange(0.01, 9.99);
    m_gamma->setSingleStep(0.01);
    m_gamma->setDecimals(2);
    m_gamma->setValue(1.00);
    m_gamma->setFixedWidth(60);
    inputRow->addWidget(m_gamma);

    inputRow->addStretch();
    m_inWhite = new QSpinBox;
    m_inWhite->setRange(2, 255);
    m_inWhite->setValue(255);
    m_inWhite->setFixedWidth(50);
    inputRow->addWidget(m_inWhite);
    left->addLayout(inputRow);

    // Output Levels label
    left->addWidget(new QLabel(tr("Output Levels:")));

    // Output gradient bar
    auto *gradientBar = new QWidget;
    gradientBar->setFixedHeight(16);
    gradientBar->setFixedWidth(256);
    gradientBar->setStyleSheet(QStringLiteral(
        "background: qlineargradient(x1:0,y1:0,x2:1,y2:0,"
        "stop:0 black, stop:1 white);"
        "border: 1px solid #999;"));
    left->addWidget(gradientBar);

    // Output triangle slider (black=0, white=1)
    m_outputSlider = new TriangleSlider(2, 0, 255);
    m_outputSlider->setFixedWidth(256);
    m_outputSlider->setRange(0, 0, 255);
    m_outputSlider->setValue(0, 0);
    m_outputSlider->setColor(0, Qt::black);
    m_outputSlider->setRange(1, 0, 255);
    m_outputSlider->setValue(1, 255);
    m_outputSlider->setColor(1, Qt::white);
    left->addWidget(m_outputSlider);

    // Output level spinboxes
    auto *outputRow = new QHBoxLayout;
    m_outBlack = new QSpinBox;
    m_outBlack->setRange(0, 255);
    m_outBlack->setValue(0);
    m_outBlack->setFixedWidth(50);
    outputRow->addWidget(m_outBlack);
    outputRow->addStretch();
    m_outWhite = new QSpinBox;
    m_outWhite->setRange(0, 255);
    m_outWhite->setValue(255);
    m_outWhite->setFixedWidth(50);
    outputRow->addWidget(m_outWhite);
    left->addLayout(outputRow);

    outer->addLayout(left, 1);

    // -- right column: buttons -------------------------------------------------
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(80);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(80);
    auto *autoBtn = new QPushButton(tr("Auto"));
    autoBtn->setFixedWidth(80);
    auto *optionsBtn = new QPushButton(tr("Options..."));
    optionsBtn->setFixedWidth(80);
    optionsBtn->setEnabled(false);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addWidget(autoBtn);
    btnCol->addWidget(optionsBtn);
    btnCol->addSpacing(10);

    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    btnCol->addWidget(m_preview);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    // -- connections -----------------------------------------------------------

    // Spinbox <-> triangle slider sync (input)
    //
    // Photoshop's gamma slider: the displayed gamma IS the value used as
    // the exponent's denominator (the engine computes n^(1/gamma)).
    // The slider position p (normalised between black and white) is where
    // p^(1/gamma) = 0.5, i.e. the input that produces 50% output.
    // Solving: 1/gamma = log(0.5)/log(p)  =>  gamma = log(p)/log(0.5).
    // Inversely: p = 0.5^gamma  (= pow(0.5, gamma)).
    auto gammaToSlider = [this](double gamma) -> int {
        const int lo = m_inBlack->value();
        const int hi = m_inWhite->value();
        if (hi <= lo) return lo;
        double mid = std::pow(0.5, qBound(0.01, gamma, 9.99));
        return qBound(lo, lo + static_cast<int>(mid * (hi - lo) + 0.5), hi);
    };

    auto sliderToGamma = [this](int pos) -> double {
        const int lo = m_inBlack->value();
        const int hi = m_inWhite->value();
        if (hi <= lo) return 1.0;
        double norm = static_cast<double>(pos - lo) / (hi - lo);
        norm = qBound(0.001, norm, 0.999);
        return qBound(0.01, std::log(norm) / std::log(0.5), 9.99);
    };

    auto updateAllRanges = [this, gammaToSlider] {
        const int lo = m_inBlack->value();
        const int hi = m_inWhite->value();
        // Black can't pass gray (which is between lo and hi)
        m_inputSlider->setRange(0, 0, qMax(0, hi - 1));
        // Gray stays between black and white
        m_inputSlider->setRange(1, lo + 1, qMax(lo + 1, hi - 1));
        // White can't go below gray
        m_inputSlider->setRange(2, qMin(255, lo + 1), 255);
        // Reposition gray thumb to match current gamma
        m_inputSlider->setValue(1, gammaToSlider(m_gamma->value()));
    };

    connect(m_inBlack, QOverload<int>::of(&QSpinBox::valueChanged), this,
            [this, updateAllRanges](int v) {
        m_inputSlider->setValue(0, v);
        // Enforce: white must be >= black + 2
        if (m_inWhite->value() < v + 2) {
            m_inWhite->blockSignals(true);
            m_inWhite->setValue(qMin(255, v + 2));
            m_inWhite->blockSignals(false);
            m_inputSlider->setValue(2, m_inWhite->value());
        }
        updateAllRanges();
        markCustom();
        onValueChanged();
    });
    connect(m_inWhite, QOverload<int>::of(&QSpinBox::valueChanged), this,
            [this, updateAllRanges](int v) {
        m_inputSlider->setValue(2, v);
        // Enforce: black must be <= white - 2
        if (m_inBlack->value() > v - 2) {
            m_inBlack->blockSignals(true);
            m_inBlack->setValue(qMax(0, v - 2));
            m_inBlack->blockSignals(false);
            m_inputSlider->setValue(0, m_inBlack->value());
        }
        updateAllRanges();
        markCustom();
        onValueChanged();
    });
    connect(m_gamma, QOverload<double>::of(&QDoubleSpinBox::valueChanged), this,
            [this, gammaToSlider](double v) {
        m_inputSlider->setValue(1, gammaToSlider(v));
        markCustom();
        onValueChanged();
    });

    connect(m_inputSlider, &TriangleSlider::valueChanged, this,
            [this, updateAllRanges, gammaToSlider, sliderToGamma](int idx, int val) {
        if (idx == 0) {
            m_inBlack->blockSignals(true);
            m_inBlack->setValue(val);
            m_inBlack->blockSignals(false);
            if (m_inWhite->value() < val + 2) {
                m_inWhite->blockSignals(true);
                m_inWhite->setValue(qMin(255, val + 2));
                m_inWhite->blockSignals(false);
                m_inputSlider->setValue(2, m_inWhite->value());
            }
            updateAllRanges();
        } else if (idx == 2) {
            m_inWhite->blockSignals(true);
            m_inWhite->setValue(val);
            m_inWhite->blockSignals(false);
            if (m_inBlack->value() > val - 2) {
                m_inBlack->blockSignals(true);
                m_inBlack->setValue(qMax(0, val - 2));
                m_inBlack->blockSignals(false);
                m_inputSlider->setValue(0, m_inBlack->value());
            }
            updateAllRanges();
        } else if (idx == 1) {
            double g = sliderToGamma(val);
            m_gamma->blockSignals(true);
            m_gamma->setValue(g);
            m_gamma->blockSignals(false);
        }
        markCustom();
        onValueChanged();
    });

    // Spinbox <-> triangle slider sync (output)
    connect(m_outBlack, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int v) {
        m_outputSlider->setValue(0, v);
        markCustom();
        onValueChanged();
    });
    connect(m_outWhite, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int v) {
        m_outputSlider->setValue(1, v);
        markCustom();
        onValueChanged();
    });
    connect(m_outputSlider, &TriangleSlider::valueChanged, this, [this](int idx, int val) {
        if (idx == 0) {
            m_outBlack->blockSignals(true);
            m_outBlack->setValue(val);
            m_outBlack->blockSignals(false);
        } else {
            m_outWhite->blockSignals(true);
            m_outWhite->setValue(val);
            m_outWhite->blockSignals(false);
        }
        markCustom();
        onValueChanged();
    });

    connect(m_channelCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &LevelsDialog::rebuildHistogram);

    connect(m_presetCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &LevelsDialog::applyPreset);

    connect(m_preview, &QCheckBox::toggled, this, [this](bool checked) {
        if (checked)
            applyPreview();
        else
            revertPreview();
    });

    connect(autoBtn, &QPushButton::clicked, this, [this] {
        if (m_originalImage.isNull())
            return;
        const QImage src = m_originalImage.convertToFormat(QImage::Format_ARGB32);
        int minVal = 255, maxVal = 0;
        for (int y = 0; y < src.height(); ++y) {
            const auto *line = reinterpret_cast<const QRgb *>(src.constScanLine(y));
            for (int x = 0; x < src.width(); ++x) {
                const int g = qGray(line[x]);
                if (g < minVal) minVal = g;
                if (g > maxVal) maxVal = g;
            }
        }
        m_inBlack->setValue(minVal);
        m_inWhite->setValue(qMax(minVal + 1, maxVal));
    });

    connect(okBtn, &QPushButton::clicked, this, [this] {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);

    rebuildHistogram();
}

LevelsDialog::~LevelsDialog()
{
    revertPreview();
}

void LevelsDialog::onValueChanged()
{
    applyPreview();
}

void LevelsDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    const float inBlack = m_inBlack->value() / 255.0f;
    const float inWhite = m_inWhite->value() / 255.0f;
    const float gamma = static_cast<float>(m_gamma->value());
    const float outBlack = m_outBlack->value() / 255.0f;
    const float outWhite = m_outWhite->value() / 255.0f;

    const int channel = m_channelCombo->currentIndex(); // 0=RGB, 1=Red, 2=Green, 3=Blue
    m_engine->applyLevels(inBlack, inWhite, gamma, outBlack, outWhite, channel);
    m_previewApplied = true;
}

void LevelsDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

void LevelsDialog::rebuildHistogram()
{
    if (m_originalImage.isNull())
        return;
    m_histogram->setImage(m_originalImage, m_channelCombo->currentIndex());
}

void LevelsDialog::applyPreset(int index)
{
    // Skip separators
    const QString text = m_presetCombo->itemText(index);
    if (text.isEmpty()) return;

    // Find the preset by name
    for (int i = 0; i < kPresetCount; ++i) {
        if (text == QString::fromUtf8(kPresets[i].name)) {
            if (i == kCustomIndex) return;
            m_updatingFromPreset = true;
            m_inBlack->setValue(kPresets[i].inBlack);
            m_inWhite->setValue(kPresets[i].inWhite);
            m_gamma->setValue(kPresets[i].gamma);
            m_outBlack->setValue(kPresets[i].outBlack);
            m_outWhite->setValue(kPresets[i].outWhite);
            m_updatingFromPreset = false;
            onValueChanged();
            return;
        }
    }
}

void LevelsDialog::markCustom()
{
    if (m_updatingFromPreset) return;
    // Find "Custom" item and select it
    for (int i = 0; i < m_presetCombo->count(); ++i) {
        if (m_presetCombo->itemText(i) == QStringLiteral("Custom")) {
            m_presetCombo->blockSignals(true);
            m_presetCombo->setCurrentIndex(i);
            m_presetCombo->blockSignals(false);
            return;
        }
    }
}
