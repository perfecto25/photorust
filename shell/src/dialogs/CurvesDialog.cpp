#include "CurvesDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGridLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QPushButton>
#include <QVBoxLayout>
#include <cmath>

// ---------------------------------------------------------------------------
// Monotone cubic spline (Fritsch-Carlson) for the curve
// ---------------------------------------------------------------------------

static void splineInterpolate(
    const QVector<QPointF> &pts, float out[256])
{
    const int n = pts.size();
    if (n == 0) {
        for (int i = 0; i < 256; ++i) out[i] = i / 255.0f;
        return;
    }
    if (n == 1) {
        for (int i = 0; i < 256; ++i) out[i] = qBound(0.0f, (float)pts[0].y(), 1.0f);
        return;
    }

    // Compute deltas and slopes
    QVector<double> dx(n - 1), dy(n - 1), m(n);
    for (int i = 0; i < n - 1; ++i) {
        dx[i] = pts[i + 1].x() - pts[i].x();
        dy[i] = pts[i + 1].y() - pts[i].y();
    }
    QVector<double> slopes(n - 1);
    for (int i = 0; i < n - 1; ++i)
        slopes[i] = dx[i] > 1e-9 ? dy[i] / dx[i] : 0.0;

    m[0] = slopes[0];
    m[n - 1] = slopes[n - 2];
    for (int i = 1; i < n - 1; ++i)
        m[i] = (slopes[i - 1] + slopes[i]) * 0.5;

    // Fritsch-Carlson monotonicity
    for (int i = 0; i < n - 1; ++i) {
        if (std::abs(slopes[i]) < 1e-9) {
            m[i] = 0;
            m[i + 1] = 0;
        } else {
            double a = m[i] / slopes[i];
            double b = m[i + 1] / slopes[i];
            double h = std::hypot(a, b);
            if (h > 3.0) {
                double t = 3.0 / h;
                m[i] = t * a * slopes[i];
                m[i + 1] = t * b * slopes[i];
            }
        }
    }

    // Evaluate at each integer input 0..255
    for (int ix = 0; ix < 256; ++ix) {
        double x = ix / 255.0;
        // Clamp to endpoints
        if (x <= pts[0].x()) {
            out[ix] = qBound(0.0f, (float)pts[0].y(), 1.0f);
            continue;
        }
        if (x >= pts[n - 1].x()) {
            out[ix] = qBound(0.0f, (float)pts[n - 1].y(), 1.0f);
            continue;
        }
        // Find segment
        int seg = 0;
        for (int i = n - 2; i >= 0; --i) {
            if (x >= pts[i].x()) { seg = i; break; }
        }
        double h = dx[seg];
        if (h < 1e-12) { out[ix] = qBound(0.0f, (float)pts[seg].y(), 1.0f); continue; }
        double t = (x - pts[seg].x()) / h;
        double t2 = t * t, t3 = t2 * t;
        double h00 = 2 * t3 - 3 * t2 + 1;
        double h10 = t3 - 2 * t2 + t;
        double h01 = -2 * t3 + 3 * t2;
        double h11 = t3 - t2;
        double val = h00 * pts[seg].y() + h10 * h * m[seg]
                   + h01 * pts[seg + 1].y() + h11 * h * m[seg + 1];
        out[ix] = qBound(0.0f, (float)val, 1.0f);
    }
}

// ---------------------------------------------------------------------------
// CurveWidget
// ---------------------------------------------------------------------------

CurveWidget::CurveWidget(QWidget *parent)
    : QWidget(parent)
{
    setFixedSize(kSize + 2, kSize + 2);
    resetCurve();
}

void CurveWidget::resetCurve()
{
    m_points.clear();
    m_points.append(QPointF(0.0, 0.0));
    m_points.append(QPointF(1.0, 1.0));
    interpolate();
    update();
}

void CurveWidget::setPoints(const QVector<QPointF> &pts)
{
    m_points = pts;
    interpolate();
    update();
    emit curveChanged();
}

void CurveWidget::setHistogram(const QImage &img, int channel)
{
    std::fill(std::begin(m_histo), std::end(m_histo), 0);
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
            m_histo[val]++;
        }
    }
    m_histoPeak = 1;
    for (int i = 0; i < 256; ++i)
        m_histoPeak = qMax(m_histoPeak, m_histo[i]);
    update();
}

void CurveWidget::buildLut(uint8_t lut[256]) const
{
    for (int i = 0; i < 256; ++i)
        lut[i] = static_cast<uint8_t>(qBound(0.0f, m_curve[i] * 255.0f + 0.5f, 255.0f));
}

void CurveWidget::interpolate()
{
    std::sort(m_points.begin(), m_points.end(),
              [](const QPointF &a, const QPointF &b) { return a.x() < b.x(); });
    splineInterpolate(m_points, m_curve);
}

QPointF CurveWidget::toWidget(QPointF p) const
{
    return QPointF(1 + p.x() * kSize, 1 + (1.0 - p.y()) * kSize);
}

QPointF CurveWidget::fromWidget(QPointF p) const
{
    return QPointF((p.x() - 1) / kSize, 1.0 - (p.y() - 1) / kSize);
}

void CurveWidget::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing);

    // Background
    p.fillRect(rect(), Qt::white);

    // Histogram
    if (m_showHisto) {
        p.setPen(Qt::NoPen);
        p.setBrush(QColor(220, 220, 220));
        for (int i = 0; i < 256; ++i) {
            int barH = static_cast<int>(
                static_cast<double>(m_histo[i]) / m_histoPeak * kSize);
            if (barH > 0)
                p.drawRect(1 + i, 1 + kSize - barH, 1, barH);
        }
    }

    // Grid lines (4x4)
    p.setPen(QPen(QColor(200, 200, 200), 1));
    for (int i = 1; i < 4; ++i) {
        int pos = 1 + i * kSize / 4;
        p.drawLine(pos, 1, pos, 1 + kSize);
        p.drawLine(1, pos, 1 + kSize, pos);
    }

    // Baseline diagonal
    if (m_showBaseline) {
        p.setPen(QPen(QColor(180, 180, 180), 1, Qt::DashLine));
        p.drawLine(1, 1 + kSize, 1 + kSize, 1);
    }

    // Curve
    p.setPen(QPen(Qt::black, 1.5));
    for (int i = 0; i < 255; ++i) {
        QPointF a = toWidget(QPointF(i / 255.0, m_curve[i]));
        QPointF b = toWidget(QPointF((i + 1) / 255.0, m_curve[i + 1]));
        p.drawLine(a, b);
    }

    // Control points
    p.setPen(QPen(Qt::black, 1));
    for (const auto &pt : m_points) {
        QPointF w = toWidget(pt);
        p.setBrush(Qt::white);
        p.drawEllipse(w, 4, 4);
    }

    // Border
    p.setPen(QPen(QColor(150, 150, 150), 1));
    p.setBrush(Qt::NoBrush);
    p.drawRect(QRectF(0.5, 0.5, kSize + 1, kSize + 1));
}

void CurveWidget::mousePressEvent(QMouseEvent *e)
{
    QPointF pos = fromWidget(e->pos());
    m_dragging = -1;

    // Check if clicking near an existing point
    for (int i = 0; i < m_points.size(); ++i) {
        QPointF w = toWidget(m_points[i]);
        if ((e->pos() - w).manhattanLength() < 10) {
            m_dragging = i;
            return;
        }
    }

    // Add new point
    pos.setX(qBound(0.0, pos.x(), 1.0));
    pos.setY(qBound(0.0, pos.y(), 1.0));
    m_points.append(pos);
    std::sort(m_points.begin(), m_points.end(),
              [](const QPointF &a, const QPointF &b) { return a.x() < b.x(); });
    for (int i = 0; i < m_points.size(); ++i) {
        if (m_points[i] == pos) { m_dragging = i; break; }
    }
    interpolate();
    update();
    emit curveChanged();
}

void CurveWidget::mouseMoveEvent(QMouseEvent *e)
{
    if (m_dragging < 0) return;
    QPointF pos = fromWidget(e->pos());
    pos.setX(qBound(0.0, pos.x(), 1.0));
    pos.setY(qBound(0.0, pos.y(), 1.0));

    // Don't let endpoints move horizontally
    if (m_dragging == 0)
        pos.setX(0.0);
    else if (m_dragging == m_points.size() - 1)
        pos.setX(1.0);

    m_points[m_dragging] = pos;
    interpolate();
    // Re-find index after sort
    for (int i = 0; i < m_points.size(); ++i) {
        if (m_points[i] == pos) { m_dragging = i; break; }
    }
    update();
    emit curveChanged();
}

void CurveWidget::mouseReleaseEvent(QMouseEvent *e)
{
    if (m_dragging >= 0) {
        // If dragged off the widget area and it's not an endpoint, remove it
        QPointF pos = fromWidget(e->pos());
        if (m_dragging > 0 && m_dragging < m_points.size() - 1) {
            if (pos.x() < -0.05 || pos.x() > 1.05 || pos.y() < -0.05 || pos.y() > 1.05) {
                m_points.removeAt(m_dragging);
                interpolate();
                update();
                emit curveChanged();
            }
        }
    }
    m_dragging = -1;
}

// ---------------------------------------------------------------------------
// Curves presets
// ---------------------------------------------------------------------------

struct CurvesPreset {
    const char *name;
    QVector<QPointF> points;
};

static QVector<CurvesPreset> buildPresets()
{
    QVector<CurvesPreset> p;
    p.append({"Default", {{0,0},{1,1}}});
    p.append({"Color Negative (RGB)", {{0,1},{1,0}}});
    p.append({"Cross Process (RGB)", {{0,0},{0.25,0.06},{0.5,0.55},{0.75,0.9},{1,1}}});
    p.append({"Darker (RGB)", {{0,0},{0.5,0.35},{1,1}}});
    p.append({"Increase Contrast (RGB)", {{0,0},{0.25,0.15},{0.75,0.85},{1,1}}});
    p.append({"Lighter (RGB)", {{0,0},{0.5,0.65},{1,1}}});
    p.append({"Linear Contrast (RGB)", {{0,0.05},{0.25,0.20},{0.75,0.80},{1,0.95}}});
    p.append({"Medium Contrast (RGB)", {{0,0},{0.3,0.2},{0.7,0.8},{1,1}}});
    p.append({"Negative (RGB)", {{0,1},{1,0}}});
    p.append({"Strong Contrast (RGB)", {{0,0},{0.25,0.1},{0.75,0.9},{1,1}}});
    p.append({"Custom", {{0,0},{1,1}}});
    return p;
}

// ---------------------------------------------------------------------------
// CurvesDialog
// ---------------------------------------------------------------------------

CurvesDialog::CurvesDialog(Engine *engine, int layerIndex, QWidget *parent)
    : CurvesDialog(engine, parent)
{
    // Editing a layer, not the pixels: the curve is stored on the layer and
    // the canvas shows it through that layer's mask.
    m_adjustmentLayer = layerIndex;
    if (m_engine) {
        m_engine->beginAdjustmentEdit(layerIndex);
    }
    applyPreview();
}

CurvesDialog::CurvesDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Curves"));
    setFixedSize(580, 400);

    if (m_engine)
        m_originalImage = m_engine->compositeImage();

    auto *outer = new QHBoxLayout(this);

    // -- left column -----------------------------------------------------------
    auto *left = new QVBoxLayout;

    // Preset row
    auto *presetRow = new QHBoxLayout;
    presetRow->addWidget(new QLabel(tr("Preset:")));
    m_presetCombo = new QComboBox;
    auto presets = buildPresets();
    for (int i = 0; i < presets.size(); ++i) {
        m_presetCombo->addItem(QString::fromUtf8(presets[i].name));
        if (i == 0)
            m_presetCombo->insertSeparator(1);
        if (i == presets.size() - 2)
            m_presetCombo->insertSeparator(m_presetCombo->count());
    }
    m_presetCombo->setMinimumWidth(200);
    presetRow->addWidget(m_presetCombo, 1);
    left->addLayout(presetRow);

    // Channel selector
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

    // Curve widget with Input/Output labels
    auto *curveArea = new QHBoxLayout;
    auto *outputLabel = new QLabel(tr("Output:"));
    outputLabel->setAlignment(Qt::AlignBottom | Qt::AlignHCenter);
    curveArea->addWidget(outputLabel);
    m_curveWidget = new CurveWidget;
    curveArea->addWidget(m_curveWidget);
    left->addLayout(curveArea);

    auto *inputLabel = new QLabel(tr("Input:"));
    inputLabel->setAlignment(Qt::AlignCenter);
    left->addWidget(inputLabel);

    outer->addLayout(left, 1);

    // -- right column ----------------------------------------------------------
    auto *right = new QVBoxLayout;

    // Buttons
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(80);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(80);
    auto *autoBtn = new QPushButton(tr("Auto"));
    autoBtn->setFixedWidth(80);
    autoBtn->setEnabled(false);
    auto *optionsBtn = new QPushButton(tr("Options..."));
    optionsBtn->setFixedWidth(80);
    optionsBtn->setEnabled(false);
    right->addWidget(okBtn);
    right->addWidget(cancelBtn);
    right->addSpacing(5);
    right->addWidget(autoBtn);
    right->addWidget(optionsBtn);
    right->addSpacing(10);

    // Preview
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    right->addWidget(m_preview);
    right->addSpacing(10);

    // Show group
    auto *showGroup = new QGroupBox(tr("Show:"));
    auto *showLayout = new QVBoxLayout(showGroup);
    m_histoCheck = new QCheckBox(tr("Histogram"));
    m_histoCheck->setChecked(true);
    showLayout->addWidget(m_histoCheck);
    m_baselineCheck = new QCheckBox(tr("Baseline"));
    m_baselineCheck->setChecked(true);
    showLayout->addWidget(m_baselineCheck);
    right->addWidget(showGroup);

    right->addStretch();
    outer->addLayout(right);

    // -- connections -----------------------------------------------------------
    connect(m_curveWidget, &CurveWidget::curveChanged, this, [this] {
        if (!m_applyingPreset) {
            for (int i = 0; i < m_presetCombo->count(); ++i) {
                if (m_presetCombo->itemText(i) == QStringLiteral("Custom")) {
                    m_presetCombo->blockSignals(true);
                    m_presetCombo->setCurrentIndex(i);
                    m_presetCombo->blockSignals(false);
                    break;
                }
            }
        }
        applyPreview();
    });

    connect(m_channelCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &CurvesDialog::rebuildHistogram);

    connect(m_presetCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &CurvesDialog::applyPreset);

    connect(m_preview, &QCheckBox::toggled, this, [this](bool checked) {
        if (checked)
            applyPreview();
        else
            revertPreview();
    });

    connect(m_histoCheck, &QCheckBox::toggled,
            m_curveWidget, &CurveWidget::setShowHistogram);
    connect(m_baselineCheck, &QCheckBox::toggled,
            m_curveWidget, &CurveWidget::setShowBaseline);

    connect(okBtn, &QPushButton::clicked, this, [this] {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);

    rebuildHistogram();
}

CurvesDialog::~CurvesDialog()
{
    // Only the destructive path leaves a preview to undo; a layer edit has
    // already been settled by `accept` or `reject`.
    if (m_adjustmentLayer < 0) {
        revertPreview();
    }
}

void CurvesDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    uint8_t lut[256];
    m_curveWidget->buildLut(lut);
    const int channel = m_channelCombo->currentIndex();
    const rust::Slice<const uint8_t> table(lut, 256);

    if (m_adjustmentLayer >= 0) {
        // The layer carries the curve, so there is nothing to undo between
        // previews — setting it again simply replaces what it was.
        m_engine->setLayerCurves(table, channel);
        return;
    }

    revertPreview();
    m_engine->applyCurvesLut(table, channel);
    m_previewApplied = true;
}

void CurvesDialog::accept()
{
    if (m_adjustmentLayer >= 0 && m_engine) {
        m_engine->endAdjustmentEdit(true);
    }
    QDialog::accept();
}

void CurvesDialog::reject()
{
    if (m_adjustmentLayer >= 0 && m_engine) {
        // Nothing was committed, so the curve itself is what goes back.
        m_engine->endAdjustmentEdit(false);
    }
    QDialog::reject();
}

void CurvesDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

void CurvesDialog::rebuildHistogram()
{
    if (m_originalImage.isNull())
        return;
    m_curveWidget->setHistogram(m_originalImage, m_channelCombo->currentIndex());
}

void CurvesDialog::applyPreset(int index)
{
    const QString text = m_presetCombo->itemText(index);
    if (text.isEmpty()) return;

    auto presets = buildPresets();
    for (const auto &p : presets) {
        if (text == QString::fromUtf8(p.name)) {
            if (text == QStringLiteral("Custom")) return;
            m_applyingPreset = true;
            m_curveWidget->setPoints(p.points);
            m_applyingPreset = false;
            return;
        }
    }
}
