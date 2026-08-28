#include "ReplaceColorDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"
#include "tools/ToolIcons.h"

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
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>

#include <cmath>

namespace {

/// Where the eyedropper reads from. One per application — see `setSampler`.
ReplaceColorDialog::Sampler g_sampler;

/// The size of the mask thumbnail, matching CS6's proportions.
constexpr int kMaskBox = 200;

const QCursor &eyedropperCursor()
{
    static const QCursor cursor = [] {
        const QPixmap pale = ToolIcons::icon(ToolId::Eyedropper, Qt::white).pixmap(22, 22);
        const QPixmap dark =
            ToolIcons::icon(ToolId::Eyedropper, QColor(0, 0, 0, 190)).pixmap(22, 22);

        QPixmap art(pale.size());
        art.setDevicePixelRatio(pale.devicePixelRatio());
        art.fill(Qt::transparent);
        QPainter painter(&art);
        painter.drawPixmap(QPointF(1, 1), dark);
        painter.drawPixmap(QPointF(0, 0), pale);
        painter.end();
        // The tip of the dropper is the bottom-left of the glyph.
        return QCursor(art, 0, 21);
    }();
    return cursor;
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

void ReplaceColorDialog::setSampler(Sampler sampler)
{
    g_sampler = std::move(sampler);
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
            [this] { m_pickMode = PickMode::Replace; });
    connect(m_addButton, &QToolButton::clicked, this,
            [this] { m_pickMode = PickMode::Add; });
    connect(m_subButton, &QToolButton::clicked, this,
            [this] { m_pickMode = PickMode::Subtract; });

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
    connect(m_showSelection, &QRadioButton::toggled, this, [this] { refreshMask(); });
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
        connect(slider, &QSlider::valueChanged, this, &ReplaceColorDialog::onValueChanged);
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
    if (m_loading) {
        return;
    }
    refreshMask();
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

    // The thumbnail has to show the document as it was before the preview
    // was applied, so the mask is taken with the preview rolled back.
    const bool wasApplied = m_previewApplied;
    if (wasApplied) {
        revertPreview();
    }

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

    if (wasApplied && m_preview->isChecked()) {
        applyPreview();
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

void ReplaceColorDialog::takeSampleAt(const QPoint &globalPos)
{
    QColor color;
    QPoint doc;
    if (!g_sampler || !g_sampler(globalPos, &color, &doc)) {
        return;
    }

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

void ReplaceColorDialog::updateHoverSampling()
{
    if (!g_sampler) {
        return;
    }

    const QPoint pos = QCursor::pos();
    const bool outside = !frameGeometry().contains(pos);

    if (outside == m_sampling) {
        if (m_sampling) {
            showCursorFor(g_sampler(pos, nullptr, nullptr));
        }
        return;
    }

    if (outside) {
        // A drag that began inside the dialog — sliding off the end of a
        // slider, say — belongs to the control it started on. Taking the mouse
        // mid-drag would cut that gesture off, so sampling waits for the
        // button to come up.
        if (QGuiApplication::mouseButtons() != Qt::NoButton) {
            return;
        }
        m_sampling = true;
        // The dialog is modal, so the canvas cannot see the pointer at all:
        // holding the mouse is what lets the dialog catch a click on the
        // image. The cursor is not taken with the grab, because it has to
        // change as the pointer crosses on and off the canvas.
        grabMouse();
        QGuiApplication::setOverrideCursor(Qt::ArrowCursor);
        m_cursorOverridden = true;
        showCursorFor(g_sampler(pos, nullptr, nullptr));
    } else {
        m_sampling = false;
        releaseMouse();
        clearCursorOverride();
    }
}

void ReplaceColorDialog::showCursorFor(bool overImage)
{
    if (!m_cursorOverridden) {
        return;
    }
    // The eyedropper belongs to the image and stops at its edge: over the
    // panels or another window there is nothing to sample.
    QGuiApplication::changeOverrideCursor(overImage ? eyedropperCursor()
                                                    : QCursor(Qt::ArrowCursor));
}

void ReplaceColorDialog::clearCursorOverride()
{
    if (m_cursorOverridden) {
        QGuiApplication::restoreOverrideCursor();
        m_cursorOverridden = false;
    }
}

void ReplaceColorDialog::mouseMoveEvent(QMouseEvent *event)
{
    updateHoverSampling();
    if (m_sampling) {
        event->accept();
        return;
    }
    QDialog::mouseMoveEvent(event);
}

void ReplaceColorDialog::mouseReleaseEvent(QMouseEvent *event)
{
    if (m_sampling) {
        // Unlike the Color Picker, which follows the pointer, a sample is only
        // taken on a click: the list is cumulative, so hovering must not keep
        // adding to it.
        takeSampleAt(event->globalPosition().toPoint());
        event->accept();
        return;
    }
    QDialog::mouseReleaseEvent(event);
}

void ReplaceColorDialog::showEvent(QShowEvent *event)
{
    QDialog::showEvent(event);
    if (!g_sampler) {
        return;
    }
    if (!m_hoverTimer) {
        m_hoverTimer = new QTimer(this);
        m_hoverTimer->setInterval(30);
        connect(m_hoverTimer, &QTimer::timeout, this,
                &ReplaceColorDialog::updateHoverSampling);
    }
    m_hoverTimer->start();
}

void ReplaceColorDialog::hideEvent(QHideEvent *event)
{
    if (m_hoverTimer) {
        m_hoverTimer->stop();
    }
    if (m_sampling) {
        m_sampling = false;
        // Neither the grab nor the cursor may outlive the dialog, or the
        // application is left with a mouse it cannot use.
        releaseMouse();
    }
    clearCursorOverride();
    QDialog::hideEvent(event);
}
