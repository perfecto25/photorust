#include "ImageSizeDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"
#include "tools/ToolIcons.h"

#include <QCheckBox>
#include <QComboBox>
#include <QStandardItemModel>
#include <QDoubleSpinBox>
#include <QFrame>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QPen>
#include <QPixmap>
#include <QPushButton>
#include <QToolButton>
#include <QVBoxLayout>

#include <algorithm>
#include <cmath>

namespace {

/// Width/Height units, in CS6's order. Pixels and Percent are handled apart
/// from the physical units, which all convert through the resolution.
enum Unit { UnitPixels = 0, UnitInches, UnitCm, UnitMm, UnitPoints, UnitPercent };

constexpr int kPreviewBox = 220;

/// One entry in the Fit To menu.
///
/// Stored as the physical size and resolution CS6 labels it with, rather than
/// precomputed pixels, so the arithmetic is visible and a preset cannot drift
/// from its own label.
struct FitPreset {
    const char *label;
    double width;
    double height;
    int unit;      ///< UnitPixels, UnitMm or UnitInches.
    double dpi;
};

const FitPreset kFitPresets[] = {
    {"960 x 640 px 144 ppi",     960,  640, UnitPixels, 144},
    {"1024 x 768 px 72 ppi",    1024,  768, UnitPixels,  72},
    {"1136 x 640 px 144 ppi",   1136,  640, UnitPixels, 144},
    {"1366 x 768 px 72 ppi",    1366,  768, UnitPixels,  72},

    {"A4 210 x 297 mm 300 dpi",  210,  297, UnitMm,     300},
    {"A6 105 x 148 mm 300 dpi",  105,  148, UnitMm,     300},
    {"Legal 8.5 x 14 in 300 dpi", 8.5,   14, UnitInches, 300},
    {"Letter 8.5 x 11 in 300 dpi", 8.5,  11, UnitInches, 300},

    {"4 x 6 in 300 dpi",           4,    6, UnitInches, 300},
    {"5 x 7 in 300 dpi",           5,    7, UnitInches, 300},
    {"8 x 10 in 300 dpi",          8,   10, UnitInches, 300},
    {"11 x 14 in 300 dpi",        11,   14, UnitInches, 300},
};

/// Index groups that get a separator drawn after them, matching CS6.
constexpr int kFitGroupBreaks[] = {3, 7};

/// Item data marking the two entries that are not presets.
constexpr int kFitOriginal = -1;
constexpr int kFitAutoResolution = -2;
constexpr int kFitCustom = -3;

/// Pixels a preset's dimension comes to at its own resolution.
int presetPixels(double size, int unit, double dpi)
{
    switch (unit) {
    case UnitMm:     return qRound(size / 25.4 * dpi);
    case UnitInches: return qRound(size * dpi);
    default:         return qRound(size);
    }
}

/// The chain glyph: two interlocking links when constrained, pulled apart and
/// broken when not. Drawn as line art through `ToolIcons` so it matches the
/// weight and colour of the rest of the interface rather than depending on a
/// Unicode glyph the user's font may not have.
QIcon chainIcon(bool linked, const QColor &color)
{
    if (linked) {
        return ToolIcons::fromSvgBody(
            QStringLiteral(R"SVG(<rect x="7.1" y="2.4" width="5.8" height="8.6" rx="2.9"/>)SVG"
                           R"SVG(<rect x="7.1" y="9" width="5.8" height="8.6" rx="2.9"/>)SVG"),
            color);
    }
    return ToolIcons::fromSvgBody(
        QStringLiteral(R"SVG(<rect x="7.1" y="1.8" width="5.8" height="7" rx="2.9"/>)SVG"
                       R"SVG(<rect x="7.1" y="11.2" width="5.8" height="7" rx="2.9"/>)SVG"),
        color);
}

/// The chain toggle, with the bracket CS6 draws linking the two rows it binds.
///
/// Painted whole rather than assembled from a QToolButton icon plus decoration:
/// the glyph has to sit *on* the bracket's spine, interrupting it, and letting
/// the button centre its own icon puts it beside the bracket instead.
class ChainToggle : public QToolButton
{
public:
    explicit ChainToggle(QWidget *parent = nullptr)
        : QToolButton(parent)
    {
        setCheckable(true);
        setChecked(true);
        setAutoRaise(true);
        setFocusPolicy(Qt::NoFocus);
        setFixedWidth(30);
        setToolTip(QToolButton::tr("Constrain aspect ratio"));
    }

protected:
    void paintEvent(QPaintEvent *) override
    {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing, true);

        const bool linked = isChecked();
        QColor ink(0xE8, 0xE8, 0xE8);
        // Dimmed when unconstrained, so the state reads before the glyph is
        // examined closely.
        ink.setAlpha(linked ? 235 : 130);

        const QPixmap glyph = chainIcon(linked, ink).pixmap(18, 18);
        const int spine = width() - 9;
        const int cy = height() / 2;
        painter.drawPixmap(spine - glyph.width() / 2, cy - glyph.height() / 2, glyph);

        QColor line = ink;
        line.setAlpha(linked ? 200 : 90);
        painter.setPen(QPen(line, 1.0));

        // Arms reach right toward the two fields; the spine joins them and is
        // broken where the glyph sits.
        const int right = width() - 1;
        const int top = height() / 4;
        const int bottom = height() - height() / 4;
        const int gap = glyph.height() / 2 + 1;

        painter.drawLine(spine, top, right, top);
        painter.drawLine(spine, bottom, right, bottom);
        painter.drawLine(spine, top, spine, cy - gap);
        painter.drawLine(spine, cy + gap, spine, bottom);
    }
};

/// Format a byte count the way CS6 does: M above a megabyte, K below.
QString sizeSummary(double bytes)
{
    if (bytes >= 1024.0 * 1024.0) {
        return QStringLiteral("%1M").arg(bytes / (1024.0 * 1024.0), 0, 'f', 2);
    }
    return QStringLiteral("%1K").arg(bytes / 1024.0, 0, 'f', 1);
}

} // namespace

ImageSizeDialog::ImageSizeDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Image Size"));
    if (m_engine) {
        m_pixelWidth = m_engine->property("canvasWidth").toInt();
        m_pixelHeight = m_engine->property("canvasHeight").toInt();
        m_resolution = m_engine->imageResolution();
    }
    m_pixelWidth = qMax(1, m_pixelWidth);
    m_pixelHeight = qMax(1, m_pixelHeight);
    m_originalWidth = m_pixelWidth;
    m_originalHeight = m_pixelHeight;
    m_originalResolution = m_resolution;

    buildUi();
    syncFields();
}

double ImageSizeDialog::resultResolution() const
{
    return m_resolution;
}

int ImageSizeDialog::resampleMode() const
{
    return m_resample->isChecked() ? m_resampleMode->currentIndex() : -1;
}

double ImageSizeDialog::unitScale(int unitIndex) const
{
    // Pixels per unit. The physical units all go through the resolution,
    // which is exactly why changing Resolution moves Width and Height when
    // resampling is off.
    switch (unitIndex) {
    case UnitInches: return m_resolution;
    case UnitCm:     return m_resolution / 2.54;
    case UnitMm:     return m_resolution / 25.4;
    case UnitPoints: return m_resolution / 72.0;
    default:         return 1.0;   // Pixels
    }
}

void ImageSizeDialog::buildUi()
{
    auto *outer = new QHBoxLayout(this);

    // --- Preview ----------------------------------------------------------
    m_preview = new QLabel;
    m_preview->setFixedSize(kPreviewBox, kPreviewBox);
    m_preview->setAlignment(Qt::AlignCenter);
    m_preview->setFrameShape(QFrame::Box);
    m_preview->setStyleSheet(QStringLiteral("background-color: #2b2b2b; border: 1px solid #555;"));
    if (m_engine) {
        const QImage thumb =
            m_engine->layerThumbnail(m_engine->property("activeLayerIndex").toInt(), kPreviewBox);
        if (!thumb.isNull()) {
            m_preview->setPixmap(QPixmap::fromImage(thumb).scaled(
                m_preview->size(), Qt::KeepAspectRatio, Qt::SmoothTransformation));
        }
    }
    outer->addWidget(m_preview, 0, Qt::AlignTop);

    auto *right = new QVBoxLayout;

    auto *summaryRow = new QHBoxLayout;
    summaryRow->addWidget(new QLabel(tr("Image Size:")));
    m_summary = new QLabel;
    summaryRow->addWidget(m_summary);
    summaryRow->addStretch();
    right->addLayout(summaryRow);

    auto *dimRow = new QHBoxLayout;
    dimRow->addWidget(new QLabel(tr("Dimensions:")));
    m_dimensions = new QLabel;
    dimRow->addWidget(m_dimensions);
    dimRow->addStretch();
    right->addLayout(dimRow);

    auto *fitRow = new QHBoxLayout;
    fitRow->addWidget(new QLabel(tr("Fit To:")));
    m_fitTo = new QComboBox;
    m_fitTo->addItem(tr("Original Size"), kFitOriginal);
    m_fitTo->addItem(tr("Custom"), kFitCustom);
    m_fitTo->addItem(tr("Auto Resolution..."), kFitAutoResolution);
    m_fitTo->insertSeparator(m_fitTo->count());
    for (int i = 0; i < int(std::size(kFitPresets)); ++i) {
        m_fitTo->addItem(QString::fromUtf8(kFitPresets[i].label), i);
        for (int brk : kFitGroupBreaks) {
            if (i == brk) {
                m_fitTo->insertSeparator(m_fitTo->count());
            }
        }
    }
    // Auto Resolution opens a further dialog in CS6 (screen/print target and
    // halftone screen), which is not implemented — shown for completeness but
    // not selectable, rather than silently doing nothing.
    if (auto *model = qobject_cast<QStandardItemModel *>(m_fitTo->model())) {
        if (auto *item = model->item(m_fitTo->findData(kFitAutoResolution))) {
            item->setEnabled(false);
        }
    }
    fitRow->addWidget(m_fitTo, 1);
    right->addLayout(fitRow);

    // --- Width / Height / Resolution --------------------------------------
    auto *grid = new QGridLayout;

    const auto addUnits = [](QComboBox *box, bool physicalOnly) {
        box->addItem(tr("Pixels"));
        box->addItem(tr("Inches"));
        box->addItem(tr("Centimeters"));
        box->addItem(tr("Millimeters"));
        box->addItem(tr("Points"));
        if (!physicalOnly) {
            box->addItem(tr("Percent"));
        }
    };

    grid->addWidget(new QLabel(tr("Width:")), 0, 1);
    m_width = new QDoubleSpinBox;
    m_width->setRange(0.001, 300000.0);
    m_width->setDecimals(3);
    grid->addWidget(m_width, 0, 2);
    m_widthUnit = new QComboBox;
    addUnits(m_widthUnit, false);
    grid->addWidget(m_widthUnit, 0, 3);

    grid->addWidget(new QLabel(tr("Height:")), 1, 1);
    m_height = new QDoubleSpinBox;
    m_height->setRange(0.001, 300000.0);
    m_height->setDecimals(3);
    grid->addWidget(m_height, 1, 2);
    m_heightUnit = new QComboBox;
    addUnits(m_heightUnit, false);
    grid->addWidget(m_heightUnit, 1, 3);

    // The chain, spanning the two rows it links, exactly as CS6 draws it.
    m_chain = new ChainToggle;
    grid->addWidget(m_chain, 0, 0, 2, 1);

    grid->addWidget(new QLabel(tr("Resolution:")), 2, 1);
    m_resolution_field = new QDoubleSpinBox;
    m_resolution_field->setRange(1.0, 30000.0);
    // Whole ppi, matching CS6 — a fractional print resolution is not a thing
    // anyone types, and 72.000 just looks wrong.
    m_resolution_field->setDecimals(0);
    grid->addWidget(m_resolution_field, 2, 2);
    m_resolutionUnit = new QComboBox;
    m_resolutionUnit->addItem(tr("Pixels/Inch"));
    m_resolutionUnit->addItem(tr("Pixels/Centimeter"));
    grid->addWidget(m_resolutionUnit, 2, 3);
    right->addLayout(grid);

    // --- Resample ---------------------------------------------------------
    auto *resampleRow = new QHBoxLayout;
    m_resample = new QCheckBox(tr("Resample:"));
    m_resample->setChecked(true);
    resampleRow->addWidget(m_resample);
    m_resampleMode = new QComboBox;
    // CS6's order; several map onto the same interpolator in the engine.
    m_resampleMode->addItem(tr("Automatic"));
    m_resampleMode->addItem(tr("Preserve Details"));
    m_resampleMode->addItem(tr("Bicubic Smoother (enlargement)"));
    m_resampleMode->addItem(tr("Bicubic Sharper (reduction)"));
    m_resampleMode->addItem(tr("Bicubic (smooth gradients)"));
    m_resampleMode->addItem(tr("Nearest Neighbor (hard edges)"));
    m_resampleMode->addItem(tr("Bilinear"));
    resampleRow->addWidget(m_resampleMode, 1);
    right->addLayout(resampleRow);

    right->addStretch();
    outer->addLayout(right, 1);

    // --- Buttons ----------------------------------------------------------
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

    connect(m_width, &QDoubleSpinBox::editingFinished, this, &ImageSizeDialog::onWidthEdited);
    connect(m_height, &QDoubleSpinBox::editingFinished, this, &ImageSizeDialog::onHeightEdited);
    connect(m_resolution_field, &QDoubleSpinBox::editingFinished,
            this, &ImageSizeDialog::onResolutionEdited);
    connect(m_widthUnit, &QComboBox::currentIndexChanged, this, &ImageSizeDialog::onUnitsChanged);
    connect(m_heightUnit, &QComboBox::currentIndexChanged, this, &ImageSizeDialog::onUnitsChanged);
    connect(m_resolutionUnit, &QComboBox::currentIndexChanged,
            this, &ImageSizeDialog::onUnitsChanged);
    connect(m_resample, &QCheckBox::toggled, this, &ImageSizeDialog::onResampleToggled);
    connect(m_fitTo, &QComboBox::currentIndexChanged, this, &ImageSizeDialog::onFitToChanged);

    m_widthUnit->setCurrentIndex(UnitPixels);
    m_heightUnit->setCurrentIndex(UnitPixels);
}

void ImageSizeDialog::syncFields()
{
    m_updating = true;

    const int wUnit = m_widthUnit->currentIndex();
    const int hUnit = m_heightUnit->currentIndex();

    // Pixels are whole; inches and the rest need decimals to be usable.
    m_width->setDecimals(wUnit == UnitPixels ? 0 : 3);
    m_height->setDecimals(hUnit == UnitPixels ? 0 : 3);

    m_width->setValue(wUnit == UnitPercent
                          ? 100.0 * m_pixelWidth / m_originalWidth
                          : m_pixelWidth / unitScale(wUnit));
    m_height->setValue(hUnit == UnitPercent
                           ? 100.0 * m_pixelHeight / m_originalHeight
                           : m_pixelHeight / unitScale(hUnit));

    // Pixels/cm is the same number expressed per 2.54cm.
    m_resolution_field->setValue(m_resolutionUnit->currentIndex() == 1 ? m_resolution / 2.54
                                                                      : m_resolution);
    m_updating = false;
    updateSummary();
}

void ImageSizeDialog::updateSummary()
{
    // Scale the document's real byte count by the pixel-count change, so the
    // figure tracks the pending resize while still counting channels the way
    // the engine's colour mode says — three for RGB, not the four we store.
    double bytes = double(m_pixelWidth) * double(m_pixelHeight) * 3.0;
    if (m_engine && m_originalWidth > 0 && m_originalHeight > 0) {
        const double actual = double(m_engine->imageDataBytes());
        const double ratio = (double(m_pixelWidth) * double(m_pixelHeight))
            / (double(m_originalWidth) * double(m_originalHeight));
        bytes = actual * ratio;
    }
    m_summary->setText(sizeSummary(bytes));
    m_dimensions->setText(tr("%1 px × %2 px").arg(m_pixelWidth).arg(m_pixelHeight));
}

void ImageSizeDialog::onWidthEdited()
{
    if (m_updating) {
        return;
    }
    markCustom();
    const int unit = m_widthUnit->currentIndex();
    const double aspect = double(m_originalHeight) / double(m_originalWidth);

    if (!m_resample->isChecked()) {
        // Pixels are fixed, so a new printed width means a new resolution.
        if (unit != UnitPixels && unit != UnitPercent && m_width->value() > 0.0) {
            m_resolution = m_pixelWidth / m_width->value();
        }
        syncFields();
        return;
    }

    m_pixelWidth = unit == UnitPercent
                       ? qRound(m_originalWidth * m_width->value() / 100.0)
                       : qRound(m_width->value() * unitScale(unit));
    m_pixelWidth = qMax(1, m_pixelWidth);
    if (m_chain->isChecked()) {
        m_pixelHeight = qMax(1, int(std::lround(m_pixelWidth * aspect)));
    }
    syncFields();
}

void ImageSizeDialog::onHeightEdited()
{
    if (m_updating) {
        return;
    }
    markCustom();
    const int unit = m_heightUnit->currentIndex();
    const double aspect = double(m_originalWidth) / double(m_originalHeight);

    if (!m_resample->isChecked()) {
        if (unit != UnitPixels && unit != UnitPercent && m_height->value() > 0.0) {
            m_resolution = m_pixelHeight / m_height->value();
        }
        syncFields();
        return;
    }

    m_pixelHeight = unit == UnitPercent
                        ? qRound(m_originalHeight * m_height->value() / 100.0)
                        : qRound(m_height->value() * unitScale(unit));
    m_pixelHeight = qMax(1, m_pixelHeight);
    if (m_chain->isChecked()) {
        m_pixelWidth = qMax(1, int(std::lround(m_pixelHeight * aspect)));
    }
    syncFields();
}

void ImageSizeDialog::onResolutionEdited()
{
    if (m_updating) {
        return;
    }
    markCustom();
    const double entered = m_resolution_field->value();
    m_resolution = m_resolutionUnit->currentIndex() == 1 ? entered * 2.54 : entered;
    // With resampling on, resolution is print metadata and the pixels stay put;
    // with it off the printed size moves instead, which `syncFields` shows.
    syncFields();
}

void ImageSizeDialog::onUnitsChanged()
{
    syncFields();
}

void ImageSizeDialog::onFitToChanged(int index)
{
    if (m_updating) {
        return;
    }
    const int data = m_fitTo->itemData(index).toInt();
    if (data == kFitCustom || data == kFitAutoResolution) {
        return;
    }
    applyFitPreset(data);
}

void ImageSizeDialog::applyFitPreset(int presetIndex)
{
    if (presetIndex == kFitOriginal) {
        m_pixelWidth = m_originalWidth;
        m_pixelHeight = m_originalHeight;
        m_resolution = m_originalResolution;
        syncFields();
        return;
    }
    if (presetIndex < 0 || presetIndex >= int(std::size(kFitPresets))) {
        return;
    }

    const FitPreset &preset = kFitPresets[presetIndex];
    int w = presetPixels(preset.width, preset.unit, preset.dpi);
    int h = presetPixels(preset.height, preset.unit, preset.dpi);

    // Presets are listed portrait; a landscape image takes the preset turned
    // on its side, which is what stops "A4" from rotating a photograph.
    if ((m_pixelWidth > m_pixelHeight) != (w > h)) {
        std::swap(w, h);
    }

    m_pixelWidth = qMax(1, w);
    m_pixelHeight = qMax(1, h);
    m_resolution = preset.dpi;
    // A preset is a resize, so it needs resampling on to mean anything.
    if (!m_resample->isChecked()) {
        m_resample->setChecked(true);
    }
    syncFields();
}

void ImageSizeDialog::markCustom()
{
    const int custom = m_fitTo->findData(kFitCustom);
    if (custom < 0 || m_fitTo->currentIndex() == custom) {
        return;
    }
    const QSignalBlocker block(m_fitTo);
    m_fitTo->setCurrentIndex(custom);
}

void ImageSizeDialog::onResampleToggled(bool on)
{
    m_resampleMode->setEnabled(on);
    if (!on) {
        // Turning it off puts the pixels back: from here the dialog can only
        // change how large the image prints, never what it contains.
        m_pixelWidth = m_originalWidth;
        m_pixelHeight = m_originalHeight;
    }
    syncFields();
}
