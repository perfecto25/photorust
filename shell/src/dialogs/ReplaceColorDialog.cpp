#include "ReplaceColorDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"
#include "tools/ToolIcons.h"

#include <QApplication>
#include <QDateTime>
#include <QDebug>
#include <QButtonGroup>
#include <QGridLayout>
#include <QGuiApplication>
#include <QHBoxLayout>
#include <QImage>
#include <QLabel>
#include <QMouseEvent>
#include <QPainter>
#include <QPixmap>
#include <QPushButton>
#include <QRadioButton>
#include <QSlider>
#include <QSpinBox>
#include <QToolButton>
#include <QVBoxLayout>

#include <cmath>

namespace {

/// Where the eyedropper reads from. One per application — see `setSampler`.
ReplaceColorDialog::CursorHook g_cursorHook;

/// The size of the mask thumbnail, matching CS6's proportions.
constexpr int kMaskBox = 200;

/// The dropper cursor, badged like CS6's: bare for Sample, "+" for Add to
/// Sample and "-" for Subtract, so the active mode is visible on the image
/// rather than only in the dialog.
QCursor eyedropperCursor(char badge)
{
    const QPixmap pale = ToolIcons::icon(ToolId::Eyedropper, Qt::white).pixmap(22, 22);
    const QPixmap dark =
        ToolIcons::icon(ToolId::Eyedropper, QColor(0, 0, 0, 190)).pixmap(22, 22);

    QPixmap art(pale.size());
    art.setDevicePixelRatio(pale.devicePixelRatio());
    art.fill(Qt::transparent);
    QPainter painter(&art);
    painter.drawPixmap(QPointF(1, 1), dark);
    painter.drawPixmap(QPointF(0, 0), pale);
    if (badge != '\0') {
        // Bottom-right of the glyph, drawn dark-on-light so it reads over
        // both a bright and a dark image.
        QFont font = painter.font();
        font.setPixelSize(11);
        font.setBold(true);
        painter.setFont(font);
        const QRect box(art.width() - 10, art.height() - 12, 10, 12);
        painter.setPen(QColor(0, 0, 0, 200));
        painter.drawText(box.translated(1, 1), Qt::AlignCenter, QChar(badge));
        painter.setPen(Qt::white);
        painter.drawText(box, Qt::AlignCenter, QChar(badge));
    }
    painter.end();
    // The tip of the dropper is the bottom-left of the glyph.
    //
    // Derived from the rendered pixmap rather than hard-coded: `pixmap()`
    // returns the icon's own size (20px), not the 22 asked for, and a hotspot
    // outside the bitmap produces an invalid cursor — which is why the
    // eyedropper silently failed to appear at all.
    // x=3 matches the Color Picker's dropper, whose hotspot has always been
    // correct; y is derived so it can never fall outside the bitmap again.
    return QCursor(art, 3, art.height() - 1);
}

/// A flat swatch showing a colour, framed so white reads against the dialog.
QLabel *makeSwatch()
{
    auto *label = new QLabel;
    label->setFixedSize(34, 26);
    label->setFrameShape(QFrame::Box);
    label->setFrameShadow(QFrame::Plain);
    label->setAutoFillBackground(true);
    return label;
}

void setSwatchColor(QLabel *label, const QColor &color)
{
    if (!label) {
        return;
    }
    if (!color.isValid()) {
        label->setStyleSheet(QStringLiteral("border: 1px solid #000;"));
        return;
    }
    label->setStyleSheet(QStringLiteral("background-color: %1; border: 1px solid #000;")
                             .arg(color.name()));
}

} // namespace

ReplaceColorDialog::ReplaceColorDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Replace Color"));
    buildUi();
    refreshMask();
    refreshSwatches();
}

ReplaceColorDialog::~ReplaceColorDialog()
{
    revertPreview();
}

void ReplaceColorDialog::setCursorHook(CursorHook hook)
{
    g_cursorHook = std::move(hook);
}

void ReplaceColorDialog::buildUi()
{
    auto *outer = new QHBoxLayout(this);
    auto *left = new QVBoxLayout;

    // --- Eyedroppers and the sampled colour -------------------------------
    auto *topRow = new QHBoxLayout;
    auto *group = new QButtonGroup(this);
    const auto makeDropper = [&](int variant, const QString &tip) {
        auto *button = new QToolButton;
        button->setCheckable(true);
        button->setIcon(ToolIcons::icon(ToolId::Eyedropper, variant, QColor(0xE8, 0xE8, 0xE8)));
        button->setToolTip(tip);
        group->addButton(button);
        topRow->addWidget(button);
        return button;
    };
    m_pickButton = makeDropper(0, tr("Sample the colour to replace"));
    m_addButton = makeDropper(0, tr("Add to sample"));
    m_subButton = makeDropper(0, tr("Subtract from sample"));
    m_pickButton->setChecked(true);

    // The three share one glyph, so a badge is what tells them apart.
    m_addButton->setText(QStringLiteral("+"));
    m_subButton->setText(QStringLiteral("-"));
    m_addButton->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    m_subButton->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);

    connect(m_pickButton, &QToolButton::clicked, this,
            [this] { m_pickMode = PickMode::Replace; refreshSamplingCursor(); });
    connect(m_addButton, &QToolButton::clicked, this,
            [this] { m_pickMode = PickMode::Add; refreshSamplingCursor(); });
    connect(m_subButton, &QToolButton::clicked, this,
            [this] { m_pickMode = PickMode::Subtract; refreshSamplingCursor(); });

    topRow->addStretch();
    topRow->addWidget(new QLabel(tr("Color:")));
    m_colorSwatch = makeSwatch();
    topRow->addWidget(m_colorSwatch);
    left->addLayout(topRow);

    m_localized = new QCheckBox(tr("Localized Color Clusters"));
    connect(m_localized, &QCheckBox::toggled, this, &ReplaceColorDialog::onValueChanged);
    left->addWidget(m_localized);

    // --- Fuzziness ---------------------------------------------------------
    auto *fuzzRow = new QHBoxLayout;
    fuzzRow->addWidget(new QLabel(tr("Fuzziness:")));
    m_fuzzinessSlider = new QSlider(Qt::Horizontal);
    m_fuzzinessSlider->setRange(0, 200);
    m_fuzzinessSlider->setValue(40);
    fuzzRow->addWidget(m_fuzzinessSlider, 1);
    m_fuzzinessSpin = new QSpinBox;
    m_fuzzinessSpin->setRange(0, 200);
    m_fuzzinessSpin->setValue(40);
    m_fuzzinessSpin->setFixedWidth(60);
    fuzzRow->addWidget(m_fuzzinessSpin);
    connect(m_fuzzinessSlider, &QSlider::valueChanged, m_fuzzinessSpin, &QSpinBox::setValue);
    connect(m_fuzzinessSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_fuzzinessSlider, &QSlider::setValue);
    connect(m_fuzzinessSlider, &QSlider::valueChanged, this,
            &ReplaceColorDialog::onValueChanged);
    left->addLayout(fuzzRow);

    // --- Mask thumbnail ----------------------------------------------------
    m_maskLabel = new QLabel;
    m_maskLabel->setFixedSize(kMaskBox, kMaskBox * 3 / 5);
    m_maskLabel->setAlignment(Qt::AlignCenter);
    m_maskLabel->setStyleSheet(QStringLiteral("background-color: #000; border: 1px solid #555;"));
    left->addWidget(m_maskLabel, 0, Qt::AlignHCenter);

    auto *modeRow = new QHBoxLayout;
    modeRow->addStretch();
    m_showSelection = new QRadioButton(tr("Selection"));
    m_showSelection->setChecked(true);
    m_showImage = new QRadioButton(tr("Image"));
    modeRow->addWidget(m_showSelection);
    modeRow->addWidget(m_showImage);
    modeRow->addStretch();
    connect(m_showSelection, &QRadioButton::toggled, this, [this] { applyChange(true); });
    left->addLayout(modeRow);

    // --- Replacement ------------------------------------------------------
    auto *grid = new QGridLayout;
    const auto makeRow = [&](int row, const QString &label, int min, int max,
                             QSlider *&slider, QSpinBox *&spin) {
        grid->addWidget(new QLabel(label), row, 0);
        slider = new QSlider(Qt::Horizontal);
        slider->setRange(min, max);
        slider->setValue(0);
        grid->addWidget(slider, row, 1);
        spin = new QSpinBox;
        spin->setRange(min, max);
        spin->setValue(0);
        spin->setFixedWidth(60);
        grid->addWidget(spin, row, 2);
        connect(slider, &QSlider::valueChanged, spin, &QSpinBox::setValue);
        connect(spin, QOverload<int>::of(&QSpinBox::valueChanged), slider, &QSlider::setValue);
        // The mask depends only on the samples and Fuzziness, so moving these
        // must not pay to recompute it.
        connect(slider, &QSlider::valueChanged, this, [this] { applyChange(false); });
    };
    makeRow(0, tr("Hue:"), -180, 180, m_hueSlider, m_hueSpin);
    makeRow(1, tr("Saturation:"), -100, 100, m_satSlider, m_satSpin);
    makeRow(2, tr("Lightness:"), -100, 100, m_lightSlider, m_lightSpin);

    auto *resultCol = new QVBoxLayout;
    m_resultSwatch = makeSwatch();
    resultCol->addWidget(m_resultSwatch);
    resultCol->addWidget(new QLabel(tr("Result")));
    grid->addLayout(resultCol, 0, 3, 3, 1, Qt::AlignCenter);
    left->addLayout(grid);

    left->addStretch();
    outer->addLayout(left, 1);

    // --- Buttons ----------------------------------------------------------
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(80);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(80);
    // Load/Save of .axt settings files is not implemented yet; shown for
    // layout fidelity but disabled rather than silently doing nothing.
    auto *loadBtn = new QPushButton(tr("Load..."));
    loadBtn->setFixedWidth(80);
    loadBtn->setEnabled(false);
    auto *saveBtn = new QPushButton(tr("Save..."));
    saveBtn->setFixedWidth(80);
    saveBtn->setEnabled(false);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addWidget(loadBtn);
    btnCol->addWidget(saveBtn);
    btnCol->addSpacing(10);
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    btnCol->addWidget(m_preview);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    connect(m_preview, &QCheckBox::toggled, this, [this](bool on) {
        if (on) applyPreview(); else revertPreview();
    });
    connect(okBtn, &QPushButton::clicked, this, [this] {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);
}

QString ReplaceColorDialog::samplesString() const
{
    QStringList parts;
    parts.reserve(m_samples.size());
    for (const Sample &s : m_samples) {
        parts << QStringLiteral("%1,%2,%3,%4,%5")
                     .arg(s.pos.x())
                     .arg(s.pos.y())
                     .arg(s.color.red())
                     .arg(s.color.green())
                     .arg(s.color.blue());
    }
    return parts.join(QLatin1Char(';'));
}

void ReplaceColorDialog::onValueChanged()
{
    applyChange(true);
}

void ReplaceColorDialog::applyChange(bool maskDirty)
{
    if (m_loading) {
        return;
    }
    // Roll the preview back once, up front, so both the mask and the fresh
    // preview see the original image. Doing it here rather than inside each
    // step is what keeps a slider drag to one undo and one apply — every
    // apply costs a full-image pass and a history snapshot, so doing it twice
    // per event was the whole reason this dialog felt slow.
    revertPreview();
    if (maskDirty) {
        refreshMask();
    }
    refreshSwatches();
    if (m_preview->isChecked()) {
        applyPreview();
    }
}

void ReplaceColorDialog::refreshMask()
{
    if (!m_engine || !m_maskLabel) {
        return;
    }

    // The caller has already rolled the preview back, so the engine is
    // holding the original image and the mask describes what the user
    // actually selected rather than the result of the last preview.
    QImage image;
    if (m_showImage && m_showImage->isChecked()) {
        image = m_engine->layerThumbnail(m_engine->property("activeLayerIndex").toInt(),
                                         kMaskBox);
    } else {
        image = m_engine->replaceColorMask(samplesString(),
                                           float(m_fuzzinessSpin->value()),
                                           m_localized->isChecked(),
                                           kMaskBox);
    }
    if (!image.isNull()) {
        m_maskLabel->setPixmap(QPixmap::fromImage(image).scaled(
            m_maskLabel->size(), Qt::KeepAspectRatio, Qt::SmoothTransformation));
    } else {
        m_maskLabel->clear();
    }
}

void ReplaceColorDialog::refreshSwatches()
{
    // "Color" is the most recently sampled colour; "Result" is that colour
    // put through the same HSL shift the image will get.
    const QColor sampled = m_samples.isEmpty() ? QColor() : m_samples.back().color;
    setSwatchColor(m_colorSwatch, sampled);

    if (!sampled.isValid()) {
        setSwatchColor(m_resultSwatch, QColor());
        return;
    }

    float h = 0.0f, s = 0.0f, l = 0.0f;
    sampled.getHslF(&h, &s, &l);
    if (h < 0.0f) {
        // Achromatic: Qt reports hue -1, which would wrap to nonsense.
        h = 0.0f;
    }

    const float hue = float(m_hueSpin->value()) / 360.0f;
    const float sat = float(m_satSpin->value()) / 100.0f;
    const float light = float(m_lightSpin->value()) / 100.0f;

    h = std::fmod(h + hue + 1.0f, 1.0f);
    s = sat >= 0.0f ? s + (1.0f - s) * sat : s * (1.0f + sat);
    l = light >= 0.0f ? l + (1.0f - l) * light : l * (1.0f + light);

    setSwatchColor(m_resultSwatch,
                   QColor::fromHslF(h, qBound(0.0f, s, 1.0f), qBound(0.0f, l, 1.0f)));
}

void ReplaceColorDialog::applyPreview()
{
    revertPreview();
    if (!m_engine || m_samples.isEmpty()) {
        return;
    }
    // Nothing to show until one of the sliders is off centre.
    if (m_hueSpin->value() == 0 && m_satSpin->value() == 0 && m_lightSpin->value() == 0) {
        return;
    }

    m_engine->applyReplaceColor(samplesString(),
                                float(m_fuzzinessSpin->value()),
                                m_localized->isChecked(),
                                float(m_hueSpin->value()),
                                float(m_satSpin->value()),
                                float(m_lightSpin->value()));
    m_previewApplied = true;
}

void ReplaceColorDialog::revertPreview()
{
    if (m_previewApplied && m_engine) {
        m_engine->undo();
        m_previewApplied = false;
    }
}

// ---------------------------------------------------------------- sampling ---

void ReplaceColorDialog::applySample(const QPoint &doc, const QColor &color)
{
    switch (m_pickMode) {
    case PickMode::Replace:
        m_samples.clear();
        m_samples.append({doc, color});
        break;
    case PickMode::Add:
        m_samples.append({doc, color});
        break;
    case PickMode::Subtract: {
        // Drop the samples this click falls within, so clicking a colour that
        // was pulled in by an earlier sample takes it back out again.
        const int tolerance = m_fuzzinessSpin->value();
        for (int i = m_samples.size() - 1; i >= 0; --i) {
            const QColor &c = m_samples[i].color;
            const int distance = qMax(qMax(qAbs(c.red() - color.red()),
                                           qAbs(c.green() - color.green())),
                                      qAbs(c.blue() - color.blue()));
            if (distance <= tolerance) {
                m_samples.removeAt(i);
            }
        }
        break;
    }
    }

    onValueChanged();
}

void ReplaceColorDialog::addSample(const QPoint &documentPos, const QColor &color)
{
    // Arrives from the canvas, which is in colour-sampling mode for as long
    // as this dialog is open.
    applySample(documentPos, color);
}

void ReplaceColorDialog::refreshSamplingCursor()
{
    if (!m_cursorOverridden || !g_cursorHook) {
        return;
    }
    char badge = '\0';
    if (m_pickMode == PickMode::Add) {
        badge = '+';
    } else if (m_pickMode == PickMode::Subtract) {
        badge = '-';
    }
    const QCursor cursor = eyedropperCursor(badge);
    g_cursorHook(&cursor);
}

void ReplaceColorDialog::clearCursorOverride()
{
    if (m_cursorOverridden) {
        m_cursorOverridden = false;
        if (g_cursorHook) {
            g_cursorHook(nullptr);
        }
    }
}

void ReplaceColorDialog::showEvent(QShowEvent *event)
{
    QDialog::showEvent(event);
    if (!g_cursorHook) {
        return;
    }
    // The eyedropper is shown for as long as the dialog is open, exactly as
    // Photoshop does — the pointer is over the canvas or it is over the
    // dialog, and the dialog's own widgets keep their cursors regardless.
    //
    // An earlier version polled the pointer and switched the cursor only when
    // it was over the image. That had three conditions to get right and, when
    // one of them silently failed, the eyedropper simply never appeared.
    // There is nothing here to get wrong.
    m_cursorOverridden = true;
    refreshSamplingCursor();
}

void ReplaceColorDialog::hideEvent(QHideEvent *event)
{
    // The cursor must not outlive the dialog, or the application is left
    // showing an eyedropper with nothing to sample.
    clearCursorOverride();
    QDialog::hideEvent(event);
}
