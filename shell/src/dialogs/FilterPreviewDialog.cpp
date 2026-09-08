#include "FilterPreviewDialog.h"

#include "../tools/ToolIcons.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QButtonGroup>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QGroupBox>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QMouseEvent>
#include <QPainter>
#include <QPushButton>
#include <QRadioButton>
#include <QSlider>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>

#include <cmath>

namespace {

/// The zoom steps the two magnifier buttons walk through, matching the ladder
/// CS6's filter previews use.
const QList<double> &zoomLadder()
{
    static const QList<double> ladder{0.125, 0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0,
                                      1.0,   2.0,  3.0,       4.0, 6.0,
                                      8.0,   16.0};
    return ladder;
}

/// Index of 100% in that ladder — where a preview opens.
constexpr int kHundredPercent = 5;

/// The thumbnail's size in the dialog. Fixed, as CS6's is: the region of the
/// document it shows is derived from it, and a preview that resized itself
/// would keep changing what it was showing.
constexpr int kPaneWidth = 240;
constexpr int kPaneHeight = 200;

/// CS6's Blur Center box is a small square beside the options.
constexpr int kCenterBoxSize = 108;

/// Tint for the two magnifier buttons, matching the options bar's glyphs.
const QColor kGlyphColor(0xd4, 0xd4, 0xd4);

QString magnifierSvg(bool plus)
{
    const QString sign = plus ? QStringLiteral("<path d=\"M6 8H10M8 6V10\" stroke-width=\"1.4\"/>")
                              : QStringLiteral("<path d=\"M6 8H10\" stroke-width=\"1.4\"/>");
    return QStringLiteral(R"SVG(<circle cx="8" cy="8" r="5" fill="none" stroke-width="1.4"/>
<path d="M12 12L17 17" stroke-width="1.6"/>)SVG")
           + sign;
}

/// "100%", "66.7%" — a whole number where it is one.
QString formatZoom(double zoom)
{
    const double percent = zoom * 100.0;
    if (qFuzzyCompare(percent, std::round(percent))) {
        return QStringLiteral("%1%").arg(int(std::round(percent)));
    }
    return QStringLiteral("%1%").arg(percent, 0, 'f', 1);
}

} // namespace

// ------------------------------------------------------------------- pane --

FilterPreviewPane::FilterPreviewPane(FilterPreviewDialog *dialog)
    : QWidget(dialog)
    , m_dialog(dialog)
{
    setFixedSize(kPaneWidth, kPaneHeight);
    setCursor(Qt::OpenHandCursor);
    setToolTip(QCoreApplication::translate(
        "FilterPreviewPane", "Drag to move the preview over the image"));
}

void FilterPreviewPane::setContent(const QImage &image, double zoom)
{
    m_image = image;
    m_zoom = zoom;
    update();
}

void FilterPreviewPane::paintEvent(QPaintEvent *event)
{
    Q_UNUSED(event)
    QPainter painter(this);
    painter.fillRect(rect(), QColor(0x2b, 0x2b, 0x2b));

    if (!m_image.isNull()) {
        // Nearest-neighbour once magnified, so that what the filter did to
        // individual pixels stays visible — the same rule the canvas follows
        // above 200%, and the reason to zoom a filter preview at all.
        painter.setRenderHint(QPainter::SmoothPixmapTransform, m_zoom < 2.0);
        const QSizeF scaled(m_image.width() * m_zoom, m_image.height() * m_zoom);
        const QPointF at((width() - scaled.width()) / 2.0, (height() - scaled.height()) / 2.0);
        painter.drawImage(QRectF(at, scaled), m_image);
    }

    painter.setPen(QPen(QColor(0x14, 0x14, 0x14), 1));
    painter.setBrush(Qt::NoBrush);
    painter.drawRect(rect().adjusted(0, 0, -1, -1));
}

void FilterPreviewPane::mousePressEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton) {
        QWidget::mousePressEvent(event);
        return;
    }
    m_dragging = true;
    m_dragFrom = event->position();
    setCursor(Qt::ClosedHandCursor);
}

void FilterPreviewPane::mouseMoveEvent(QMouseEvent *event)
{
    if (!m_dragging || m_zoom <= 0.0) {
        return;
    }
    // Dragging moves the image under a fixed window, so the region travels the
    // other way — and by the distance in *document* pixels, which is the
    // widget distance divided by the magnification.
    const QPointF delta = event->position() - m_dragFrom;
    m_dragFrom = event->position();
    m_dialog->panPreview(-delta / m_zoom);
}

void FilterPreviewPane::mouseReleaseEvent(QMouseEvent *event)
{
    Q_UNUSED(event)
    m_dragging = false;
    setCursor(Qt::OpenHandCursor);
}

// ------------------------------------------------------------ blur centre --

BlurCenterWidget::BlurCenterWidget(FilterPreviewDialog *dialog, std::function<bool()> spinning)
    : QWidget(dialog)
    , m_dialog(dialog)
    , m_spinning(std::move(spinning))
{
    setFixedSize(kCenterBoxSize, kCenterBoxSize);
    setCursor(Qt::CrossCursor);
    setToolTip(QCoreApplication::translate("BlurCenterWidget",
                                           "Drag to move the centre the blur turns about"));
}

void BlurCenterWidget::paintEvent(QPaintEvent *event)
{
    Q_UNUSED(event)
    QPainter painter(this);
    painter.fillRect(rect(), Qt::white);
    painter.setRenderHint(QPainter::Antialiasing, true);

    const QPointF at(m_center.x() * width(), m_center.y() * height());
    // Far enough to cover the box from wherever the centre has been dragged.
    const double reach = std::hypot(double(width()), double(height()));

    // The pattern is the shape of the blur, not the image: rings for a spin,
    // rays for a zoom. That is what CS6 draws here, and it is the only thing
    // legible at this size.
    QPen pen(QColor(0x20, 0x20, 0x20));
    pen.setWidthF(1.0);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    painter.setClipRect(rect());

    if (m_spinning && m_spinning()) {
        for (double r = 4.0; r < reach; r += 5.0) {
            painter.drawEllipse(at, r, r);
        }
    } else {
        constexpr int kRays = 48;
        for (int i = 0; i < kRays; ++i) {
            const double angle = i * 2.0 * M_PI / kRays;
            // Started off the centre, so the middle stays open the way it does
            // in a zoom, where nothing moves there.
            const QPointF from = at + QPointF(std::cos(angle), std::sin(angle)) * 4.0;
            const QPointF to = at + QPointF(std::cos(angle), std::sin(angle)) * reach;
            painter.drawLine(from, to);
        }
    }

    // The centre itself, in white on black so it reads over the pattern.
    painter.setPen(QPen(Qt::black, 3.0));
    painter.drawPoint(at);
    painter.setPen(QPen(Qt::white, 1.5));
    painter.drawPoint(at);

    painter.setClipping(false);
    painter.setPen(QPen(QColor(0x50, 0x50, 0x50), 1));
    painter.drawRect(rect().adjusted(0, 0, -1, -1));
}

void BlurCenterWidget::mousePressEvent(QMouseEvent *event)
{
    moveCenterTo(event->position());
}

void BlurCenterWidget::mouseMoveEvent(QMouseEvent *event)
{
    if (event->buttons() & Qt::LeftButton) {
        moveCenterTo(event->position());
    }
}

void BlurCenterWidget::moveCenterTo(const QPointF &pos)
{
    const QPointF wanted(qBound(0.0, pos.x() / width(), 1.0),
                         qBound(0.0, pos.y() / height(), 1.0));
    if (wanted == m_center) {
        return;
    }
    m_center = wanted;
    update();
    m_dialog->parametersChanged();
}

// ----------------------------------------------------------------- dialog --

FilterPreviewDialog::FilterPreviewDialog(Engine *engine, const QString &filterName,
                                         QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
    , m_filterName(filterName)
    , m_zoomStep(kHundredPercent)
{
    setWindowTitle(filterName);

    auto *root = new QGridLayout(this);

    m_pane = new FilterPreviewPane(this);
    root->addWidget(m_pane, 0, 0);

    // The OK/Cancel column, with Preview beneath it — CS6's arrangement.
    auto *side = new QVBoxLayout();
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    buttons->setOrientation(Qt::Vertical);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    side->addWidget(buttons);
    m_preview = new QCheckBox(tr("Preview"), this);
    m_preview->setChecked(true);
    m_preview->setToolTip(tr("Show the filter on the image while this is open"));
    side->addWidget(m_preview);
    side->addStretch(1);
    root->addLayout(side, 0, 1);

    // The zoom row under the thumbnail.
    m_zoomRow = new QWidget(this);
    auto *zoomRow = new QHBoxLayout(m_zoomRow);
    zoomRow->setContentsMargins(0, 0, 0, 0);
    zoomRow->addStretch(1);
    auto *out = new QToolButton(m_zoomRow);
    out->setAutoRaise(true);
    out->setIcon(ToolIcons::fromSvgBody(magnifierSvg(false), kGlyphColor));
    out->setToolTip(tr("Zoom the preview out"));
    zoomRow->addWidget(out);
    m_zoomLabel = new QLabel(formatZoom(zoomLadder().at(m_zoomStep)), m_zoomRow);
    m_zoomLabel->setAlignment(Qt::AlignCenter);
    m_zoomLabel->setMinimumWidth(56);
    zoomRow->addWidget(m_zoomLabel);
    auto *in = new QToolButton(m_zoomRow);
    in->setAutoRaise(true);
    in->setIcon(ToolIcons::fromSvgBody(magnifierSvg(true), kGlyphColor));
    in->setToolTip(tr("Zoom the preview in"));
    zoomRow->addWidget(in);
    zoomRow->addStretch(1);
    root->addWidget(m_zoomRow, 1, 0);

    connect(out, &QToolButton::clicked, this, [this] { setZoomStep(m_zoomStep - 1); });
    connect(in, &QToolButton::clicked, this, [this] { setZoomStep(m_zoomStep + 1); });

    m_params = new QGridLayout();
    m_params->setColumnStretch(1, 1);
    root->addLayout(m_params, 2, 0, 1, 2);

    // The titled boxes go below the numeric parameters, laid out as CS6 lays
    // Radial Blur out: the radio boxes stacked in a column on the left, the
    // Blur Center box beside them on the right.
    m_boxRow = new QHBoxLayout();
    m_boxColumn = new QVBoxLayout();
    m_boxRow->addLayout(m_boxColumn);
    m_boxRow->addStretch(1);
    root->addLayout(m_boxRow, 3, 0, 1, 2);

    // One redraw of the thumbnail and one of the canvas per burst of changes,
    // rather than one per slider tick. The canvas waits longer because it
    // filters the whole layer, which on a large image is the slow one.
    m_thumbTimer = new QTimer(this);
    m_thumbTimer->setSingleShot(true);
    m_thumbTimer->setInterval(30);
    connect(m_thumbTimer, &QTimer::timeout, this, &FilterPreviewDialog::refreshPreview);

    m_canvasTimer = new QTimer(this);
    m_canvasTimer->setSingleShot(true);
    m_canvasTimer->setInterval(180);
    connect(m_canvasTimer, &QTimer::timeout, this, &FilterPreviewDialog::refreshCanvasPreview);

    connect(m_preview, &QCheckBox::toggled, this, [this] { refreshCanvasPreview(); });

    // Open looking at the middle of the document until told otherwise.
    if (m_engine) {
        m_center = QPointF(m_engine->getCanvasWidth() / 2.0, m_engine->getCanvasHeight() / 2.0);
    }
}

FilterPreviewDialog::~FilterPreviewDialog()
{
    // Whatever happened, the layer goes back to how it was and the square
    // comes off the canvas. Pressing OK re-applies through `applyFilter`,
    // which is the only path that commits.
    if (m_engine) {
        m_engine->setFilterPreview(QString(), rust::Slice<const float>());
    }
    emit previewRegionChanged(QRectF());
}

int FilterPreviewDialog::addParameter(const QString &label, double min, double max, double value,
                                      int decimals, const QString &suffix)
{
    const int row = m_params->rowCount();

    auto *spin = new QDoubleSpinBox(this);
    spin->setDecimals(decimals);
    spin->setRange(min, max);
    spin->setValue(value);
    spin->setSuffix(suffix);
    spin->setSingleStep(decimals > 0 ? 0.1 : 1.0);

    m_params->addWidget(new QLabel(label, this), row, 0);
    m_params->addWidget(spin, row, 1);

    // The slider beneath it, which is the part that makes a filter dialog
    // usable: a blur is dialled in by dragging until it looks right, not by
    // knowing the number in advance. Sliders are integers, so a parameter
    // with decimals is scaled up and back down again.
    auto *slider = new QSlider(Qt::Horizontal, this);
    const double scale = std::pow(10.0, decimals);
    slider->setRange(int(std::lround(min * scale)), int(std::lround(max * scale)));
    slider->setValue(int(std::lround(value * scale)));
    m_params->addWidget(slider, row + 1, 0, 1, 2);

    connect(spin, &QDoubleSpinBox::valueChanged, this, [this, slider, scale](double v) {
        QSignalBlocker block(slider);
        slider->setValue(int(std::lround(v * scale)));
        parametersChanged();
    });
    connect(slider, &QSlider::valueChanged, this, [spin, scale](int v) {
        // The spin box is the one that reports the change, so that a drag of
        // the slider and a typed number take the same path.
        spin->setValue(v / scale);
    });

    m_slots.append([spin] { return spin->value(); });
    return m_slots.size() - 1;
}

int FilterPreviewDialog::addChoice(const QString &label, const QStringList &items,
                                   const QList<double> &values, int index)
{
    const int row = m_params->rowCount();
    auto *combo = new QComboBox(this);
    combo->addItems(items);
    combo->setCurrentIndex(qBound(0, index, items.size() - 1));
    m_params->addWidget(new QLabel(label, this), row, 0);
    m_params->addWidget(combo, row, 1);

    connect(combo, &QComboBox::currentIndexChanged, this,
            [this] { parametersChanged(); });

    m_slots.append([combo, values] {
        return values.value(combo->currentIndex(), 0.0);
    });
    return m_slots.size() - 1;
}

int FilterPreviewDialog::addRadioChoice(const QString &title, const QStringList &items,
                                        const QList<double> &values, int index)
{
    auto *box = new QGroupBox(title, this);
    auto *column = new QVBoxLayout(box);
    column->setSpacing(2);

    auto *group = new QButtonGroup(box);
    for (int i = 0; i < items.size(); ++i) {
        auto *radio = new QRadioButton(items.at(i), box);
        radio->setChecked(i == index);
        group->addButton(radio, i);
        column->addWidget(radio);
    }
    m_boxColumn->addWidget(box);

    connect(group, &QButtonGroup::idClicked, this, [this] { parametersChanged(); });

    m_slots.append([group, values] {
        return values.value(group->checkedId(), 0.0);
    });
    return m_slots.size() - 1;
}

int FilterPreviewDialog::addCenterPicker(const QString &title, std::function<bool()> spinning)
{
    auto *box = new QGroupBox(title, this);
    auto *column = new QVBoxLayout(box);
    auto *picker = new BlurCenterWidget(this, std::move(spinning));
    column->addWidget(picker, 0, Qt::AlignCenter);
    // To the right of the stacked radio boxes, where CS6 puts it. Inserted
    // before the trailing stretch so it does not float off to the edge.
    m_boxRow->insertWidget(m_boxRow->count() - 1, box, 0, Qt::AlignTop);
    m_pickers.append(picker);

    // One widget, two slots — the filter takes the centre as a pair.
    const int first = m_slots.size();
    m_slots.append([picker] { return picker->center().x(); });
    m_slots.append([picker] { return picker->center().y(); });
    return first;
}

void FilterPreviewDialog::setPreviewPaneVisible(bool visible)
{
    m_paneVisible = visible;
    m_pane->setVisible(visible);
    m_zoomRow->setVisible(visible);
    // Without a thumbnail there is nothing for the Preview checkbox to be
    // beside, and CS6's Radial Blur — the one dialog in this shape — has no
    // Preview either. Hiding it also spares the user a full-layer radial blur
    // recomputed on every twitch of the Amount slider.
    m_preview->setVisible(visible);
    m_preview->setChecked(visible);
    adjustSize();
}

float FilterPreviewDialog::slotValue(int slot) const
{
    if (slot < 0 || slot >= m_slots.size()) {
        return 0.0f;
    }
    return float(m_slots.at(slot)());
}

QList<float> FilterPreviewDialog::parameters() const
{
    QList<float> out;
    out.reserve(m_slots.size());
    for (int i = 0; i < m_slots.size(); ++i) {
        out.append(slotValue(i));
    }
    return out;
}

void FilterPreviewDialog::setPreviewCenter(const QPointF &documentPos)
{
    m_center = documentPos;
    panPreview(QPointF());
}

void FilterPreviewDialog::panPreview(const QPointF &documentDelta)
{
    if (!m_engine) {
        return;
    }
    const double docWidth = m_engine->getCanvasWidth();
    const double docHeight = m_engine->getCanvasHeight();
    const QRectF region = previewRegion();

    QPointF wanted = m_center + documentDelta;
    // Keep the region over the document. When it is wider than the document —
    // zoomed far out on a small image — there is nowhere to pan to, so it
    // stays centred.
    auto clamp = [](double value, double half, double extent) {
        if (half * 2.0 >= extent) {
            return extent / 2.0;
        }
        return qBound(half, value, extent - half);
    };
    wanted.setX(clamp(wanted.x(), region.width() / 2.0, docWidth));
    wanted.setY(clamp(wanted.y(), region.height() / 2.0, docHeight));

    if (wanted == m_center) {
        // Still refresh on the first call, when nothing has been drawn yet.
        m_thumbTimer->start();
        return;
    }
    m_center = wanted;
    m_thumbTimer->start();
}

QRectF FilterPreviewDialog::previewRegion() const
{
    const double zoom = zoomLadder().at(m_zoomStep);
    const double width = std::ceil(kPaneWidth / zoom);
    const double height = std::ceil(kPaneHeight / zoom);
    return QRectF(m_center.x() - width / 2.0, m_center.y() - height / 2.0, width, height);
}

void FilterPreviewDialog::setZoomStep(int step)
{
    const int clamped = qBound(0, step, int(zoomLadder().size()) - 1);
    if (clamped == m_zoomStep) {
        return;
    }
    m_zoomStep = clamped;
    m_zoomLabel->setText(formatZoom(zoomLadder().at(m_zoomStep)));
    // Zooming out can widen the region past the edge of the document, so the
    // centre has to be pulled back before the region is asked for again.
    panPreview(QPointF());
    refreshPreview();
}

void FilterPreviewDialog::parametersChanged()
{
    // The Blur Center box draws rings for a spin and rays for a zoom, so it
    // has to be redrawn when the method changes under it.
    for (BlurCenterWidget *picker : m_pickers) {
        picker->update();
    }
    m_thumbTimer->start();
    m_canvasTimer->start();
}

void FilterPreviewDialog::refreshPreview()
{
    // Nothing to draw, and for Radial Blur — the one dialog without a
    // thumbnail — asking would mean filtering the whole layer for an image
    // nobody sees, which is a second or more on a large one.
    if (!m_engine || !m_paneVisible) {
        return;
    }
    const QRectF region = previewRegion();
    const QList<float> params = parameters();
    const rust::Slice<const float> slice(params.constData(), size_t(params.size()));
    const QImage image =
        m_engine->filterPreview(m_filterName, slice, int(std::lround(region.x())),
                                int(std::lround(region.y())), int(region.width()),
                                int(region.height()));
    m_pane->setContent(image, zoomLadder().at(m_zoomStep));
    emit previewRegionChanged(region);
}

void FilterPreviewDialog::refreshCanvasPreview()
{
    if (!m_engine) {
        return;
    }
    if (m_preview->isChecked()) {
        const QList<float> params = parameters();
        m_engine->setFilterPreview(m_filterName,
                                   rust::Slice<const float>(params.constData(),
                                                            size_t(params.size())));
    } else {
        m_engine->setFilterPreview(QString(), rust::Slice<const float>());
    }
}

void FilterPreviewDialog::showEvent(QShowEvent *event)
{
    QDialog::showEvent(event);
    // Both previews are drawn once the dialog is up rather than during
    // construction, so that a caller still adding parameters is not filtering
    // the layer once per parameter.
    refreshPreview();
    refreshCanvasPreview();
}
