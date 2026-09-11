#include "FilterPreviewDialog.h"

#include "AngleDial.h"

#include "../tools/ToolIcons.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QCheckBox>
#include <QColorDialog>
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
#include <QRandomGenerator>
#include <QSpinBox>
#include <QSlider>
#include <QTabWidget>
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

/// Pinch's wireframe, which CS6 puts under the OK/Cancel column.
constexpr int kGridBoxSize = 120;

/// Shear's curve box.
constexpr int kCurveBoxSize = 140;
/// How near a click has to land to grab a control point, in pixels.
constexpr double kGrabRadius = 7.0;

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

// ------------------------------------------------------------ shear curve --

ShearCurveWidget::ShearCurveWidget(FilterPreviewDialog *dialog)
    : QWidget(dialog)
    , m_dialog(dialog)
{
    setFixedSize(kCurveBoxSize, kCurveBoxSize);
    setCursor(Qt::CrossCursor);
    setToolTip(QCoreApplication::translate(
        "ShearCurveWidget",
        "Drag the line to bend the image; click it to add a point, drag a point out to remove"));
}

QPointF ShearCurveWidget::at(int index) const
{
    const QPointF p = m_points.at(index);
    // The offset runs the full width of the box, so -1 is the left edge.
    return QPointF((p.x() + 1.0) / 2.0 * (width() - 1), p.y() * (height() - 1));
}

QList<double> ShearCurveWidget::sampled(int count) const
{
    QList<double> out;
    out.reserve(count);
    for (int i = 0; i < count; ++i) {
        const double y = count > 1 ? double(i) / (count - 1) : 0.0;
        // Between the two control points either side of this row. The list is
        // kept sorted, so the first point past `y` is the one to blend to.
        double value = m_points.first().x();
        for (int k = 1; k < m_points.size(); ++k) {
            const QPointF &low = m_points.at(k - 1);
            const QPointF &high = m_points.at(k);
            if (y <= high.y() || k == m_points.size() - 1) {
                const double span = high.y() - low.y();
                const double t = span > 0.0 ? qBound(0.0, (y - low.y()) / span, 1.0) : 0.0;
                value = low.x() * (1.0 - t) + high.x() * t;
                break;
            }
        }
        out.append(value);
    }
    return out;
}

void ShearCurveWidget::paintEvent(QPaintEvent *event)
{
    Q_UNUSED(event)
    QPainter painter(this);
    painter.fillRect(rect(), Qt::white);
    painter.setRenderHint(QPainter::Antialiasing, true);

    // The dotted 4×4 grid CS6 rules the box with.
    QPen guide(QColor(0xa0, 0xa0, 0xa0), 1.0, Qt::DotLine);
    painter.setPen(guide);
    for (int i = 1; i < 4; ++i) {
        const double t = double(i) / 4.0;
        painter.drawLine(QPointF(t * width(), 0), QPointF(t * width(), height()));
        painter.drawLine(QPointF(0, t * height()), QPointF(width(), t * height()));
    }

    QPolygonF line;
    for (int i = 0; i < m_points.size(); ++i) {
        line.append(at(i));
    }
    painter.setPen(QPen(QColor(0x20, 0x20, 0x20), 1.4));
    painter.drawPolyline(line);

    painter.setBrush(QColor(0x20, 0x20, 0x20));
    painter.setPen(Qt::NoPen);
    for (const QPointF &p : line) {
        painter.drawRect(QRectF(p.x() - 2.5, p.y() - 2.5, 5, 5));
    }

    painter.setBrush(Qt::NoBrush);
    painter.setPen(QPen(QColor(0x50, 0x50, 0x50), 1));
    painter.drawRect(rect().adjusted(0, 0, -1, -1));
}

void ShearCurveWidget::mousePressEvent(QMouseEvent *event)
{
    const QPointF pos = event->position();
    for (int i = 0; i < m_points.size(); ++i) {
        if (QLineF(pos, at(i)).length() <= kGrabRadius) {
            m_dragging = i;
            return;
        }
    }

    // Not on a point, so add one where the click landed, in curve order.
    const double y = qBound(0.0, pos.y() / (height() - 1), 1.0);
    const double x = qBound(-1.0, pos.x() / (width() - 1) * 2.0 - 1.0, 1.0);
    int index = 1;
    while (index < m_points.size() - 1 && m_points.at(index).y() < y) {
        ++index;
    }
    m_points.insert(index, QPointF(x, y));
    m_dragging = index;
    update();
    m_dialog->parametersChanged();
}

void ShearCurveWidget::mouseMoveEvent(QMouseEvent *event)
{
    if (m_dragging < 0) {
        return;
    }
    const QPointF pos = event->position();
    QPointF &point = m_points[m_dragging];
    point.setX(qBound(-1.0, pos.x() / (width() - 1) * 2.0 - 1.0, 1.0));
    // The two ends belong to the top and bottom rows and only slide sideways.
    if (m_dragging > 0 && m_dragging < m_points.size() - 1) {
        point.setY(qBound(m_points.at(m_dragging - 1).y(), pos.y() / (height() - 1),
                          m_points.at(m_dragging + 1).y()));
    }
    update();
    m_dialog->parametersChanged();
}

void ShearCurveWidget::mouseReleaseEvent(QMouseEvent *event)
{
    // Dragged out of the box: take the point away, unless it is an end, which
    // the curve cannot do without.
    if (m_dragging > 0 && m_dragging < m_points.size() - 1
        && !rect().adjusted(-2, -2, 2, 2).contains(event->position().toPoint())) {
        m_points.removeAt(m_dragging);
        update();
        m_dialog->parametersChanged();
    }
    m_dragging = -1;
}

// ------------------------------------------------------------ distort grid --

DistortGridWidget::DistortGridWidget(QWidget *parent)
    : QWidget(parent)
{
    setFixedSize(kGridBoxSize, kGridBoxSize);
}

void DistortGridWidget::setGrid(const QList<QPointF> &points, int cells)
{
    m_points = points;
    m_cells = cells;
    update();
}

void DistortGridWidget::paintEvent(QPaintEvent *event)
{
    Q_UNUSED(event)
    QPainter painter(this);
    painter.fillRect(rect(), Qt::white);
    if (m_cells < 1 || m_points.size() != (m_cells + 1) * (m_cells + 1)) {
        return;
    }
    painter.setRenderHint(QPainter::Antialiasing, true);
    painter.setPen(QPen(QColor(0x20, 0x20, 0x20), 0.9));

    // A margin, because a bulge pushes the outermost lines past the frame the
    // undistorted grid occupied.
    const qreal inset = 6.0;
    const qreal span = width() - inset * 2;
    auto at = [&](int row, int col) {
        const QPointF p = m_points.at(row * (m_cells + 1) + col);
        return QPointF(inset + p.x() * span, inset + p.y() * span);
    };

    for (int row = 0; row <= m_cells; ++row) {
        QPolygonF line;
        for (int col = 0; col <= m_cells; ++col) {
            line.append(at(row, col));
        }
        painter.drawPolyline(line);
    }
    for (int col = 0; col <= m_cells; ++col) {
        QPolygonF line;
        for (int row = 0; row <= m_cells; ++row) {
            line.append(at(row, col));
        }
        painter.drawPolyline(line);
    }
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
    m_sideColumn = new QVBoxLayout();
    auto *side = m_sideColumn;
    m_buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    m_buttons->setOrientation(Qt::Vertical);
    connect(m_buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(m_buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    side->addWidget(m_buttons);
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
    if (m_canvasDriver) {
        m_canvasDriver({}, false);
    } else if (m_engine) {
        m_engine->setFilterPreview(QString(), rust::Slice<const float>());
    }
    emit previewRegionChanged(QRectF());
}

int FilterPreviewDialog::addParameter(const QString &label, double min, double max, double value,
                                      int decimals, const QString &suffix, bool withSlider)
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

    if (!withSlider) {
        connect(spin, &QDoubleSpinBox::valueChanged, this, [this] { parametersChanged(); });
        m_slots.append([spin] { return spin->value(); });
        return m_slots.size() - 1;
    }

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

int FilterPreviewDialog::addAngleParameter(const QString &label, double value)
{
    const int row = m_params->rowCount();

    auto *spin = new QDoubleSpinBox(this);
    spin->setDecimals(0);
    spin->setRange(-360, 360);
    spin->setValue(value);
    spin->setSuffix(QStringLiteral("°"));
    spin->setWrapping(true);

    auto *dial = new AngleDial(this);
    dial->setAngle(value);

    auto *cell = new QWidget(this);
    auto *across = new QHBoxLayout(cell);
    across->setContentsMargins(0, 0, 0, 0);
    across->addWidget(spin);
    across->addWidget(dial);
    across->addStretch(1);

    m_params->addWidget(new QLabel(label, this), row, 0);
    m_params->addWidget(cell, row, 1);

    connect(dial, &AngleDial::angleChanged, this, [spin](double degrees) {
        // Through the spin box, so that dragging the wheel and typing a number
        // take the same path — as they do for a slider and its field.
        spin->setValue(degrees);
    });
    connect(spin, &QDoubleSpinBox::valueChanged, this, [this, dial](double degrees) {
        QSignalBlocker block(dial);
        dial->setAngle(degrees);
        parametersChanged();
    });

    m_slots.append([spin] { return spin->value(); });
    return m_slots.size() - 1;
}

int FilterPreviewDialog::addRangeParameter(const QString &label, const QString &lowLabel,
                                           const QString &highLabel, double min, double max,
                                           double low, double high, int decimals,
                                           const QString &suffix)
{
    const int row = m_params->rowCount();

    // The two headings sit above the fields, shared between them, which is
    // how CS6 fits Wave's six numbers into three rows.
    auto *headings = new QWidget(this);
    auto *headingRow = new QHBoxLayout(headings);
    headingRow->setContentsMargins(0, 0, 0, 0);
    headingRow->addWidget(new QLabel(lowLabel, headings), 1);
    headingRow->addWidget(new QLabel(highLabel, headings), 1);
    m_params->addWidget(headings, row, 1);

    auto *fields = new QWidget(this);
    auto *fieldRow = new QHBoxLayout(fields);
    fieldRow->setContentsMargins(0, 0, 0, 0);

    auto build = [&](double value) {
        auto *spin = new QDoubleSpinBox(fields);
        spin->setDecimals(decimals);
        spin->setRange(min, max);
        spin->setValue(value);
        spin->setSuffix(suffix);
        fieldRow->addWidget(spin, 1);
        return spin;
    };
    QDoubleSpinBox *lowSpin = build(low);
    QDoubleSpinBox *highSpin = build(high);

    m_params->addWidget(new QLabel(label, this), row + 1, 0);
    m_params->addWidget(fields, row + 1, 1);

    for (QDoubleSpinBox *spin : {lowSpin, highSpin}) {
        connect(spin, &QDoubleSpinBox::valueChanged, this, [this] { parametersChanged(); });
    }
    // The pair is a range, so the low field must not overtake the high one.
    connect(lowSpin, &QDoubleSpinBox::valueChanged, this, [highSpin](double v) {
        if (v > highSpin->value()) {
            highSpin->setValue(v);
        }
    });
    connect(highSpin, &QDoubleSpinBox::valueChanged, this, [lowSpin](double v) {
        if (v < lowSpin->value()) {
            lowSpin->setValue(v);
        }
    });

    const int first = m_slots.size();
    m_slots.append([lowSpin] { return lowSpin->value(); });
    m_slots.append([highSpin] { return highSpin->value(); });
    return first;
}

int FilterPreviewDialog::addRandomizeButton(const QString &label)
{
    // The seed lives in a spin box that happens to be hidden: the button has
    // to re-roll something the filter can be handed, and something a repeat of
    // the filter can be handed *again* — a wave that changed every time it was
    // applied could not be previewed, undone or redone.
    auto *seed = new QSpinBox(this);
    seed->setRange(0, 999999);
    seed->setValue(1);
    seed->setVisible(false);

    auto *button = new QPushButton(label, this);
    m_params->addWidget(button, m_params->rowCount(), 1, Qt::AlignLeft);
    connect(button, &QPushButton::clicked, this, [this, seed] {
        seed->setValue(QRandomGenerator::global()->bounded(1, 999999));
        parametersChanged();
    });

    m_slots.append([seed] { return double(seed->value()); });
    return m_slots.size() - 1;
}

int FilterPreviewDialog::addShearCurve(int points)
{
    auto *curve = new ShearCurveWidget(this);
    m_params->addWidget(curve, m_params->rowCount(), 0, 1, 2, Qt::AlignHCenter);

    const int first = m_slots.size();
    for (int i = 0; i < points; ++i) {
        m_slots.append([curve, points, i] { return curve->sampled(points).at(i); });
    }
    return first;
}

int FilterPreviewDialog::addChoice(const QString &label, const QStringList &items,
                                   const QList<double> &values, int index,
                                   const QList<int> &separatorsAfter)
{
    const int row = m_params->rowCount();
    auto *combo = new QComboBox(this);
    for (int i = 0; i < items.size(); ++i) {
        // The value travels with the item, so a separator inserted between
        // two of them cannot quietly shift what the list means.
        combo->addItem(items.at(i), values.value(i, 0.0));
        if (separatorsAfter.contains(i)) {
            combo->insertSeparator(combo->count());
        }
    }
    combo->setCurrentIndex(combo->findData(values.value(qBound(0, index, items.size() - 1), 0.0)));
    m_params->addWidget(new QLabel(label, this), row, 0);
    m_params->addWidget(combo, row, 1);

    connect(combo, &QComboBox::currentIndexChanged, this,
            [this] { parametersChanged(); });

    m_slots.append([combo] { return combo->currentData().toDouble(); });
    return m_slots.size() - 1;
}

int FilterPreviewDialog::addChoiceWithAngle(const QString &label, const QStringList &items,
                                            const QList<double> &values, int index, double angle,
                                            int angleForIndex)
{
    const int row = m_params->rowCount();

    auto *combo = new QComboBox(this);
    combo->addItems(items);
    combo->setCurrentIndex(qBound(0, index, items.size() - 1));

    auto *spin = new QDoubleSpinBox(this);
    spin->setDecimals(0);
    spin->setRange(-360, 360);
    spin->setValue(angle);
    spin->setSuffix(QStringLiteral("°"));
    spin->setWrapping(true);
    auto *dial = new AngleDial(this);
    dial->setAngle(angle);

    auto *cell = new QWidget(this);
    auto *across = new QHBoxLayout(cell);
    across->setContentsMargins(0, 0, 0, 0);
    across->addWidget(combo);
    across->addWidget(spin);
    across->addWidget(dial);
    across->addStretch(1);

    m_params->addWidget(new QLabel(label, this), row, 0);
    m_params->addWidget(cell, row, 1);

    // An angle only means something for one of the choices; CS6 greys it out
    // for the rest rather than hiding it, so the row does not jump about.
    auto syncEnabled = [combo, spin, dial, angleForIndex] {
        const bool wanted = combo->currentIndex() == angleForIndex;
        spin->setEnabled(wanted);
        dial->setEnabled(wanted);
    };
    syncEnabled();

    connect(combo, &QComboBox::currentIndexChanged, this, [this, syncEnabled] {
        syncEnabled();
        parametersChanged();
    });
    connect(dial, &AngleDial::angleChanged, this, [spin](double degrees) {
        spin->setValue(degrees);
    });
    connect(spin, &QDoubleSpinBox::valueChanged, this, [this, dial](double degrees) {
        QSignalBlocker block(dial);
        dial->setAngle(degrees);
        parametersChanged();
    });

    const int first = m_slots.size();
    m_slots.append([combo, values] { return values.value(combo->currentIndex(), 0.0); });
    m_slots.append([spin] { return spin->value(); });
    return first;
}

int FilterPreviewDialog::addCheckBox(const QString &label, bool checked)
{
    auto *box = new QCheckBox(label, this);
    box->setChecked(checked);
    m_params->addWidget(box, m_params->rowCount(), 0, 1, 2);
    connect(box, &QCheckBox::toggled, this, [this] { parametersChanged(); });

    m_slots.append([box] { return box->isChecked() ? 1.0 : 0.0; });
    return m_slots.size() - 1;
}

void FilterPreviewDialog::beginTab(const QString &title)
{
    auto *root = static_cast<QGridLayout *>(layout());
    if (!m_tabs) {
        m_tabs = new QTabWidget(this);
        // Below whatever was added before it, and above the boxes row, so a
        // dialog can have a line or two outside the tabs if it wants.
        root->addWidget(m_tabs, root->rowCount(), 0, 1, 2);
    }

    // Each tab gets its own parameter grid, and `m_params` is simply pointed
    // at the newest one — every `add…` below goes on adding rows without
    // knowing that tabs exist at all.
    auto *page = new QWidget(m_tabs);
    auto *grid = new QGridLayout(page);
    grid->setColumnStretch(1, 1);
    grid->setRowStretch(1000, 1);
    m_params = grid;
    m_tabs->addTab(page, title);
}

int FilterPreviewDialog::addColorButton(const QString &label, const QColor &initial,
                                        std::function<bool()> enabledWhen)
{
    const int row = m_params->rowCount();
    auto *button = new QPushButton(this);
    button->setFixedSize(32, 20);
    button->setAutoFillBackground(true);

    // The swatch is the button: its own colour is the readout, so there is no
    // separate label to fall out of step with it.
    auto *chosen = new QColor(initial);
    button->setProperty("swatch", initial);
    auto paint = [button] {
        const QColor colour = button->property("swatch").value<QColor>();
        button->setStyleSheet(QStringLiteral("background-color: %1; border: 1px solid #202020;")
                                  .arg(colour.name()));
    };
    paint();

    m_params->addWidget(new QLabel(label, this), row, 0);
    m_params->addWidget(button, row, 1, Qt::AlignLeft);

    connect(button, &QPushButton::clicked, this, [this, button, paint] {
        const QColor before = button->property("swatch").value<QColor>();
        const QColor picked = QColorDialog::getColor(before, this, tr("Color"));
        if (picked.isValid()) {
            button->setProperty("swatch", picked);
            paint();
            parametersChanged();
        }
    });

    if (enabledWhen) {
        m_conditional.append({button, enabledWhen});
    }
    delete chosen;

    const int first = m_slots.size();
    m_slots.append([button] { return button->property("swatch").value<QColor>().red(); });
    m_slots.append([button] { return button->property("swatch").value<QColor>().green(); });
    m_slots.append([button] { return button->property("swatch").value<QColor>().blue(); });
    return first;
}

void FilterPreviewDialog::addHeading(const QString &text)
{
    m_params->addWidget(new QLabel(text, this), m_params->rowCount(), 0, 1, 2);
}

void FilterPreviewDialog::addDisabledNote(const QString &text)
{
    auto *note = new QLabel(text, this);
    note->setEnabled(false);
    m_params->addWidget(note, m_params->rowCount(), 0, 1, 2);
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

void FilterPreviewDialog::setCanvasPreviewDriver(PreviewDriver driver)
{
    m_canvasDriver = std::move(driver);
    // There is something to preview again even without a thumbnail.
    if (m_canvasDriver && !m_paneVisible) {
        m_preview->setVisible(true);
        m_preview->setChecked(true);
    }
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
    m_preview->setVisible(visible || bool(m_canvasDriver));
    m_preview->setChecked(visible || bool(m_canvasDriver));

    // With no thumbnail to sit beside, the buttons drop to the foot of the
    // dialog. Left where they were they would be a column of two floating at
    // the top with nothing alongside them.
    if (!visible) {
        m_sideColumn->removeWidget(m_buttons);
        m_buttons->setOrientation(Qt::Horizontal);
        auto *root = static_cast<QGridLayout *>(layout());
        root->addWidget(m_buttons, root->rowCount(), 0, 1, 2);
    }
    adjustSize();
}

void FilterPreviewDialog::addDistortGrid()
{
    m_distortGrid = new DistortGridWidget(this);
    // Below the buttons and the Preview tick, but above the stretch that
    // holds the column up — which is where CS6 puts it.
    m_sideColumn->insertWidget(m_sideColumn->count() - 1, m_distortGrid, 0, Qt::AlignHCenter);
    refreshDistortGrid();
}

void FilterPreviewDialog::refreshDistortGrid()
{
    if (!m_distortGrid || !m_engine) {
        return;
    }
    constexpr int kCells = 12;
    const QList<float> params = parameters();
    const rust::Vec<float> flat = m_engine->distortGrid(
        m_filterName, rust::Slice<const float>(params.constData(), size_t(params.size())),
        kCells);

    QList<QPointF> points;
    points.reserve(int(flat.size()) / 2);
    for (size_t i = 0; i + 1 < flat.size(); i += 2) {
        points.append(QPointF(flat[i], flat[i + 1]));
    }
    m_distortGrid->setGrid(points, kCells);
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
    // A control that follows a tick box elsewhere — CS6 greys the custom
    // colour out until its box is ticked.
    for (const auto &[widget, live] : m_conditional) {
        widget->setEnabled(live());
    }
    refreshDistortGrid();
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
    if (m_canvasDriver) {
        m_canvasDriver(parameters(), m_preview->isChecked());
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
