#include "ColorPickerDialog.h"

#include <QCheckBox>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMouseEvent>
#include <QPainter>
#include <QPushButton>
#include <QRadioButton>
#include <QRegularExpressionValidator>
#include <QResizeEvent>
#include <QSpinBox>
#include <QVBoxLayout>

#include <cmath>

namespace {

/// Photoshop's field is 256×256 logical pixels.
constexpr int kPlaneSize = 256;
/// Width of the ramp's colour strip, excluding the arrow gutters.
constexpr int kRampWidth = 19;
/// Gutter either side of the strip, where the arrow markers are drawn.
constexpr int kRampGutter = 7;

/// Web-safe palette step. The 216-colour cube is every multiple of 0x33.
constexpr int kWebStep = 0x33;

// -- axis mapping -----------------------------------------------------------
//
// Each axis puts one component on the ramp and the other two on the field.
// The pairings below are Photoshop's:
//
//   axis   ramp          field X       field Y (top → bottom)
//   ----   -----------   -----------   ---------------------
//   H      hue           saturation    brightness  100 → 0
//   S      saturation    hue           brightness  100 → 0
//   B      brightness    hue           saturation  100 → 0
//   R      red           blue          green       255 → 0
//   G      green         blue          red         255 → 0
//   B      blue          red           green       255 → 0

/// Colour at a normalised field position. `y` is 0 at the top.
QColor planeColorAt(ColorAxis axis, int h, int s, int v, double x, double y)
{
    const auto to255 = [](double t) { return int(qBound(0.0, t, 1.0) * 255.0 + 0.5); };
    const auto to359 = [](double t) { return int(qBound(0.0, t, 1.0) * 359.0 + 0.5); };
    const QColor base = QColor::fromHsv(h, s, v);

    switch (axis) {
    case ColorAxis::Hue:
        return QColor::fromHsv(h, to255(x), to255(1.0 - y));
    case ColorAxis::Saturation:
        return QColor::fromHsv(to359(x), s, to255(1.0 - y));
    case ColorAxis::Brightness:
        return QColor::fromHsv(to359(x), to255(1.0 - y), v);
    case ColorAxis::Red:
        return QColor(base.red(), to255(1.0 - y), to255(x));
    case ColorAxis::Green:
        return QColor(to255(1.0 - y), base.green(), to255(x));
    case ColorAxis::Blue:
        return QColor(to255(x), to255(1.0 - y), base.blue());
    }
    return base;
}

/// Where the marker sits for a colour, normalised, `y` 0 at the top.
QPointF planePosFor(ColorAxis axis, int h, int s, int v)
{
    const QColor c = QColor::fromHsv(h, s, v);
    switch (axis) {
    case ColorAxis::Hue:
        return {s / 255.0, 1.0 - v / 255.0};
    case ColorAxis::Saturation:
        return {h / 359.0, 1.0 - v / 255.0};
    case ColorAxis::Brightness:
        return {h / 359.0, 1.0 - s / 255.0};
    case ColorAxis::Red:
        return {c.blue() / 255.0, 1.0 - c.green() / 255.0};
    case ColorAxis::Green:
        return {c.blue() / 255.0, 1.0 - c.red() / 255.0};
    case ColorAxis::Blue:
        return {c.red() / 255.0, 1.0 - c.green() / 255.0};
    }
    return {0.0, 0.0};
}

/// Colour along the ramp. `t` is 0 at the bottom, 1 at the top.
///
/// Note the hue ramp runs 0 at the bottom to 359 at the top, so reading
/// downward gives red → magenta → blue → cyan → green → yellow → red, which is
/// the order Photoshop shows.
QColor rampColorAt(ColorAxis axis, int h, int s, int v, double t)
{
    const auto to255 = [](double x) { return int(qBound(0.0, x, 1.0) * 255.0 + 0.5); };
    const QColor c = QColor::fromHsv(h, s, v);

    switch (axis) {
    case ColorAxis::Hue:
        // Always the fully saturated spectrum, independent of S and B.
        return QColor::fromHsv(int(qBound(0.0, t, 1.0) * 359.0 + 0.5), 255, 255);
    case ColorAxis::Saturation:
        return QColor::fromHsv(h, to255(t), v);
    case ColorAxis::Brightness:
        return QColor::fromHsv(h, s, to255(t));
    case ColorAxis::Red:
        return QColor(to255(t), c.green(), c.blue());
    case ColorAxis::Green:
        return QColor(c.red(), to255(t), c.blue());
    case ColorAxis::Blue:
        return QColor(c.red(), c.green(), to255(t));
    }
    return c;
}

/// The ramp position of a colour, 0 at the bottom.
double rampPosFor(ColorAxis axis, int h, int s, int v)
{
    const QColor c = QColor::fromHsv(h, s, v);
    switch (axis) {
    case ColorAxis::Hue:        return h / 359.0;
    case ColorAxis::Saturation: return s / 255.0;
    case ColorAxis::Brightness: return v / 255.0;
    case ColorAxis::Red:        return c.red() / 255.0;
    case ColorAxis::Green:      return c.green() / 255.0;
    case ColorAxis::Blue:       return c.blue() / 255.0;
    }
    return 0.0;
}

/// The value that determines a cached plane image, so the cache can be
/// invalidated only when it actually changes.
int planeCacheKey(ColorAxis axis, int h, int s, int v)
{
    const QColor c = QColor::fromHsv(h, s, v);
    switch (axis) {
    case ColorAxis::Hue:        return h;
    case ColorAxis::Saturation: return s;
    case ColorAxis::Brightness: return v;
    case ColorAxis::Red:        return c.red();
    case ColorAxis::Green:      return c.green();
    case ColorAxis::Blue:       return c.blue();
    }
    return 0;
}

/// Convert to HSV while keeping `h` when the colour is achromatic.
///
/// `QColor::hue()` reports -1 for greys. Without this, dragging brightness to
/// zero would discard the hue and snap the marker back to red on the way out.
void toHsvPreservingHue(const QColor &c, int &h, int &s, int &v)
{
    const int newHue = c.hue();
    if (newHue >= 0) {
        h = newHue;
    }
    s = c.saturation();
    v = c.value();
}

/// Draw the ring marker Photoshop uses on the field.
void drawPlaneMarker(QPainter &p, const QPointF &pos)
{
    p.setBrush(Qt::NoBrush);
    p.setRenderHint(QPainter::Antialiasing, true);
    // A dark ring inside a light one stays visible over any underlying colour.
    p.setPen(QPen(QColor(0, 0, 0, 190), 1.0));
    p.drawEllipse(pos, 5.5, 5.5);
    p.setPen(QPen(QColor(255, 255, 255, 230), 1.5));
    p.drawEllipse(pos, 4.5, 4.5);
}

} // namespace

// ===========================================================================
// Colour space helpers
// ===========================================================================

namespace {

double srgbToLinear(double c)
{
    return c <= 0.04045 ? c / 12.92 : std::pow((c + 0.055) / 1.055, 2.4);
}

double linearToSrgb(double c)
{
    return c <= 0.0031308 ? c * 12.92 : 1.055 * std::pow(c, 1.0 / 2.4) - 0.055;
}

// Photoshop's Lab readout is referenced to D50, not D65. Using D65 here would
// be off by several units in `a` and `b` for most colours.
constexpr double kXn = 0.96422;
constexpr double kYn = 1.00000;
constexpr double kZn = 0.82521;

constexpr double kEpsilon = 216.0 / 24389.0;  // 0.008856
constexpr double kKappa = 24389.0 / 27.0;     // 903.296

} // namespace

void rgbToLab(const QColor &color, double *l, double *a, double *b)
{
    const double r = srgbToLinear(color.redF());
    const double g = srgbToLinear(color.greenF());
    const double bl = srgbToLinear(color.blueF());

    // sRGB primaries → XYZ, D65 referenced.
    const double X = 0.4124564 * r + 0.3575761 * g + 0.1804375 * bl;
    const double Y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * bl;
    const double Z = 0.0193339 * r + 0.1191920 * g + 0.9503041 * bl;

    // Bradford chromatic adaptation, D65 → D50.
    const double x50 = 1.0478112 * X + 0.0228866 * Y - 0.0501270 * Z;
    const double y50 = 0.0295424 * X + 0.9904844 * Y - 0.0170491 * Z;
    const double z50 = -0.0092345 * X + 0.0150436 * Y + 0.7521316 * Z;

    const auto f = [](double t) {
        return t > kEpsilon ? std::cbrt(t) : (kKappa * t + 16.0) / 116.0;
    };
    const double fx = f(x50 / kXn);
    const double fy = f(y50 / kYn);
    const double fz = f(z50 / kZn);

    *l = 116.0 * fy - 16.0;
    *a = 500.0 * (fx - fy);
    *b = 200.0 * (fy - fz);
}

QColor labToRgb(double l, double a, double b)
{
    const double fy = (l + 16.0) / 116.0;
    const double fx = fy + a / 500.0;
    const double fz = fy - b / 200.0;

    const auto finv = [](double t) {
        const double t3 = t * t * t;
        return t3 > kEpsilon ? t3 : (116.0 * t - 16.0) / kKappa;
    };
    const double x50 = finv(fx) * kXn;
    const double y50 = (l > kKappa * kEpsilon ? std::pow(fy, 3.0) : l / kKappa) * kYn;
    const double z50 = finv(fz) * kZn;

    // Bradford, D50 → D65 (inverse of the matrix above).
    const double X = 0.9555766 * x50 - 0.0230393 * y50 + 0.0631636 * z50;
    const double Y = -0.0282895 * x50 + 1.0099416 * y50 + 0.0210077 * z50;
    const double Z = 0.0122982 * x50 - 0.0204830 * y50 + 1.3299098 * z50;

    // XYZ → sRGB primaries.
    const double r = 3.2404542 * X - 1.5371385 * Y - 0.4985314 * Z;
    const double g = -0.9692660 * X + 1.8760108 * Y + 0.0415560 * Z;
    const double bl = 0.0556434 * X - 0.2040259 * Y + 1.0572252 * Z;

    // Lab covers far more than sRGB, so clamping here is expected rather than
    // exceptional — out-of-gamut entries land on the gamut boundary.
    const auto toByte = [](double c) {
        return int(qBound(0.0, linearToSrgb(qBound(0.0, c, 1.0)), 1.0) * 255.0 + 0.5);
    };
    return QColor(toByte(r), toByte(g), toByte(bl));
}

QColor snapToWebColor(const QColor &color)
{
    const auto snap = [](int c) {
        return qBound(0, int(std::lround(c / double(kWebStep)) * kWebStep), 255);
    };
    return QColor(snap(color.red()), snap(color.green()), snap(color.blue()));
}

bool isWebColor(const QColor &color)
{
    return color.red() % kWebStep == 0 && color.green() % kWebStep == 0
        && color.blue() % kWebStep == 0;
}

// ===========================================================================
// ColorPlane
// ===========================================================================

ColorPlane::ColorPlane(QWidget *parent)
    : QWidget(parent)
{
    setCursor(Qt::CrossCursor);
    setFixedSize(kPlaneSize, kPlaneSize);
}

QSize ColorPlane::sizeHint() const
{
    return QSize(kPlaneSize, kPlaneSize);
}

void ColorPlane::setAxis(ColorAxis axis)
{
    if (m_axis == axis) {
        return;
    }
    m_axis = axis;
    update();
}

void ColorPlane::setHsv(int hue, int sat, int val)
{
    m_hue = hue;
    m_sat = sat;
    m_val = val;
    update();
}

void ColorPlane::setWebColorsOnly(bool webOnly)
{
    if (m_webOnly == webOnly) {
        return;
    }
    m_webOnly = webOnly;
    update();
}

void ColorPlane::rebuildCache()
{
    const int key = planeCacheKey(m_axis, m_hue, m_sat, m_val);
    if (!m_cache.isNull() && m_cacheAxis == m_axis && m_cacheValue == key
        && m_cacheWebOnly == m_webOnly && m_cache.size() == size()) {
        return;
    }

    m_cache = QImage(size(), QImage::Format_RGB32);
    const int w = width();
    const int h = height();
    if (w <= 0 || h <= 0) {
        return;
    }

    for (int y = 0; y < h; ++y) {
        auto *line = reinterpret_cast<QRgb *>(m_cache.scanLine(y));
        const double yn = h > 1 ? y / double(h - 1) : 0.0;
        for (int x = 0; x < w; ++x) {
            const double xn = w > 1 ? x / double(w - 1) : 0.0;
            QColor c = planeColorAt(m_axis, m_hue, m_sat, m_val, xn, yn);
            if (m_webOnly) {
                c = snapToWebColor(c);
            }
            line[x] = c.rgb();
        }
    }

    m_cacheAxis = m_axis;
    m_cacheValue = key;
    m_cacheWebOnly = m_webOnly;
}

void ColorPlane::paintEvent(QPaintEvent *)
{
    rebuildCache();

    QPainter p(this);
    p.drawImage(0, 0, m_cache);

    const QPointF pos = planePosFor(m_axis, m_hue, m_sat, m_val);
    drawPlaneMarker(p, QPointF(pos.x() * (width() - 1), pos.y() * (height() - 1)));

    p.setRenderHint(QPainter::Antialiasing, false);
    p.setPen(QColor(0x2a, 0x2a, 0x2a));
    p.setBrush(Qt::NoBrush);
    p.drawRect(rect().adjusted(0, 0, -1, -1));
}

void ColorPlane::pickAt(const QPoint &pos)
{
    const double xn = width() > 1 ? qBound(0.0, pos.x() / double(width() - 1), 1.0) : 0.0;
    const double yn = height() > 1 ? qBound(0.0, pos.y() / double(height() - 1), 1.0) : 0.0;

    QColor c = planeColorAt(m_axis, m_hue, m_sat, m_val, xn, yn);
    if (m_webOnly) {
        c = snapToWebColor(c);
    }

    int h = m_hue;
    int s = m_sat;
    int v = m_val;
    toHsvPreservingHue(c, h, s, v);
    emit picked(h, s, v);
}

void ColorPlane::mousePressEvent(QMouseEvent *event)
{
    pickAt(event->pos());
}

void ColorPlane::mouseMoveEvent(QMouseEvent *event)
{
    if (event->buttons() & Qt::LeftButton) {
        pickAt(event->pos());
    }
}

void ColorPlane::resizeEvent(QResizeEvent *)
{
    m_cache = QImage();
}

// ===========================================================================
// ColorRamp
// ===========================================================================

ColorRamp::ColorRamp(QWidget *parent)
    : QWidget(parent)
{
    setCursor(Qt::SizeVerCursor);
    setFixedSize(kRampWidth + kRampGutter * 2, kPlaneSize);
}

QSize ColorRamp::sizeHint() const
{
    return QSize(kRampWidth + kRampGutter * 2, kPlaneSize);
}

QRect ColorRamp::stripRect() const
{
    return QRect(kRampGutter, 0, kRampWidth, height());
}

void ColorRamp::setAxis(ColorAxis axis)
{
    if (m_axis == axis) {
        return;
    }
    m_axis = axis;
    update();
}

void ColorRamp::setHsv(int hue, int sat, int val)
{
    m_hue = hue;
    m_sat = sat;
    m_val = val;
    update();
}

void ColorRamp::setWebColorsOnly(bool webOnly)
{
    if (m_webOnly == webOnly) {
        return;
    }
    m_webOnly = webOnly;
    update();
}

void ColorRamp::rebuildCache()
{
    // The hue ramp never changes with S/B, so it only needs rebuilding when
    // the axis does. The others depend on the full current colour.
    const bool valid = !m_cache.isNull() && m_cacheAxis == m_axis
        && m_cacheWebOnly == m_webOnly && m_cache.height() == height()
        && (m_axis == ColorAxis::Hue
            || (m_cacheHue == m_hue && m_cacheSat == m_sat && m_cacheVal == m_val));
    if (valid) {
        return;
    }

    const QRect strip = stripRect();
    if (strip.width() <= 0 || strip.height() <= 0) {
        return;
    }
    m_cache = QImage(strip.size(), QImage::Format_RGB32);

    for (int y = 0; y < strip.height(); ++y) {
        // t is 1 at the top of the widget.
        const double t = strip.height() > 1
            ? 1.0 - y / double(strip.height() - 1)
            : 0.0;
        QColor c = rampColorAt(m_axis, m_hue, m_sat, m_val, t);
        if (m_webOnly) {
            c = snapToWebColor(c);
        }
        auto *line = reinterpret_cast<QRgb *>(m_cache.scanLine(y));
        for (int x = 0; x < strip.width(); ++x) {
            line[x] = c.rgb();
        }
    }

    m_cacheAxis = m_axis;
    m_cacheHue = m_hue;
    m_cacheSat = m_sat;
    m_cacheVal = m_val;
    m_cacheWebOnly = m_webOnly;
}

void ColorRamp::paintEvent(QPaintEvent *)
{
    rebuildCache();

    QPainter p(this);
    const QRect strip = stripRect();
    p.drawImage(strip.topLeft(), m_cache);
    p.setPen(QColor(0x2a, 0x2a, 0x2a));
    p.setBrush(Qt::NoBrush);
    p.drawRect(strip.adjusted(0, 0, -1, -1));

    // Photoshop marks the ramp with a pair of triangles pointing inward from
    // the gutters, rather than a handle overlapping the colour.
    const double t = rampPosFor(m_axis, m_hue, m_sat, m_val);
    const double y = (1.0 - t) * (strip.height() - 1);

    p.setRenderHint(QPainter::Antialiasing, false);
    p.setPen(Qt::NoPen);
    p.setBrush(palette().color(QPalette::WindowText));

    const QPointF left[3] = {
        {0.0, y - 4.0},
        {double(kRampGutter) - 1.0, y},
        {0.0, y + 4.0},
    };
    p.drawPolygon(left, 3);

    const double rightEdge = width();
    const QPointF right[3] = {
        {rightEdge, y - 4.0},
        {rightEdge - kRampGutter + 1.0, y},
        {rightEdge, y + 4.0},
    };
    p.drawPolygon(right, 3);
}

void ColorRamp::pickAt(const QPoint &pos)
{
    const QRect strip = stripRect();
    const double t = strip.height() > 1
        ? qBound(0.0, 1.0 - pos.y() / double(strip.height() - 1), 1.0)
        : 0.0;

    QColor c = rampColorAt(m_axis, m_hue, m_sat, m_val, t);
    if (m_webOnly) {
        c = snapToWebColor(c);
    }

    int h = m_hue;
    int s = m_sat;
    int v = m_val;
    if (m_axis == ColorAxis::Hue) {
        // The hue ramp is drawn at full saturation and brightness, so read the
        // hue from it directly and keep the existing S and B.
        h = int(qBound(0.0, t, 1.0) * 359.0 + 0.5);
    } else {
        toHsvPreservingHue(c, h, s, v);
    }
    emit picked(h, s, v);
}

void ColorRamp::mousePressEvent(QMouseEvent *event)
{
    pickAt(event->pos());
}

void ColorRamp::mouseMoveEvent(QMouseEvent *event)
{
    if (event->buttons() & Qt::LeftButton) {
        pickAt(event->pos());
    }
}

void ColorRamp::resizeEvent(QResizeEvent *)
{
    m_cache = QImage();
}

// ===========================================================================
// ColorCompare
// ===========================================================================

ColorCompare::ColorCompare(QWidget *parent)
    : QWidget(parent)
{
    setFixedSize(60, 52);
    setToolTip(tr("Click the lower swatch to restore the original color"));
}

QSize ColorCompare::sizeHint() const
{
    return QSize(60, 52);
}

void ColorCompare::setCurrentColor(const QColor &color)
{
    m_current = color;
    update();
}

void ColorCompare::setOriginalColor(const QColor &color)
{
    m_original = color;
    update();
}

void ColorCompare::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    const int half = height() / 2;
    p.fillRect(QRect(0, 0, width(), half), m_current);
    p.fillRect(QRect(0, half, width(), height() - half), m_original);

    p.setPen(QColor(0x2a, 0x2a, 0x2a));
    p.setBrush(Qt::NoBrush);
    p.drawRect(rect().adjusted(0, 0, -1, -1));
}

void ColorCompare::mousePressEvent(QMouseEvent *event)
{
    if (event->pos().y() >= height() / 2) {
        emit originalClicked();
    }
}

// ===========================================================================
// ColorPickerDialog
// ===========================================================================

ColorPickerDialog::ColorPickerDialog(const QColor &initial, QWidget *parent,
                                     const QString &title)
    : QDialog(parent)
    , m_original(initial.isValid() ? initial : QColor(Qt::black))
{
    buildUi(title);

    setColor(m_original);
    m_compare->setOriginalColor(m_original);
    syncControls();
}

void ColorPickerDialog::buildUi(const QString &title)
{
    setWindowTitle(title.isEmpty() ? tr("Color Picker")
                                   : tr("Color Picker (%1)").arg(title));
    setModal(true);

    auto *root = new QHBoxLayout(this);
    root->setContentsMargins(12, 12, 12, 12);
    root->setSpacing(10);

    // -- field, with the web-colors toggle beneath it ----------------------
    auto *fieldColumn = new QVBoxLayout();
    fieldColumn->setSpacing(8);
    m_plane = new ColorPlane(this);
    fieldColumn->addWidget(m_plane);

    m_webOnly = new QCheckBox(tr("Only Web Colors"), this);
    m_webOnly->setToolTip(tr("Restrict the picker to the 216-color web-safe palette"));
    fieldColumn->addWidget(m_webOnly);
    fieldColumn->addStretch(1);
    root->addLayout(fieldColumn);

    // -- ramp ---------------------------------------------------------------
    auto *rampColumn = new QVBoxLayout();
    rampColumn->setSpacing(8);
    m_ramp = new ColorRamp(this);
    rampColumn->addWidget(m_ramp);
    rampColumn->addStretch(1);
    root->addLayout(rampColumn);

    // -- preview + numeric fields ------------------------------------------
    auto *centre = new QVBoxLayout();
    centre->setSpacing(6);

    auto *newLabel = new QLabel(tr("new"), this);
    newLabel->setAlignment(Qt::AlignHCenter);
    centre->addWidget(newLabel);

    m_compare = new ColorCompare(this);
    centre->addWidget(m_compare, 0, Qt::AlignHCenter);

    auto *currentLabel = new QLabel(tr("current"), this);
    currentLabel->setAlignment(Qt::AlignHCenter);
    centre->addWidget(currentLabel);
    centre->addSpacing(8);

    auto *grid = new QGridLayout();
    grid->setHorizontalSpacing(4);
    grid->setVerticalSpacing(3);

    // Builds one "(o) X: [ 12 ] unit" row. `radio` may be null for the
    // read-only rows.
    const auto addRow = [&](int row, int col, QRadioButton **radio,
                            const QString &label, QSpinBox **spin, int lo, int hi,
                            const QString &unit, bool readOnly) {
        if (radio) {
            *radio = new QRadioButton(label, this);
            grid->addWidget(*radio, row, col);
        } else {
            auto *text = new QLabel(label, this);
            grid->addWidget(text, row, col, Qt::AlignRight | Qt::AlignVCenter);
        }
        *spin = new QSpinBox(this);
        (*spin)->setRange(lo, hi);
        (*spin)->setFixedWidth(52);
        (*spin)->setButtonSymbols(QAbstractSpinBox::NoButtons);
        (*spin)->setReadOnly(readOnly);
        if (readOnly) {
            (*spin)->setFocusPolicy(Qt::NoFocus);
        }
        grid->addWidget(*spin, row, col + 1);
        grid->addWidget(new QLabel(unit, this), row, col + 2);
    };

    // Left column: HSB then RGB then hex — the components you can drive.
    addRow(0, 0, &m_radioH, tr("H:"), &m_spinH, 0, 360, tr("°"), false);
    addRow(1, 0, &m_radioS, tr("S:"), &m_spinS, 0, 100, tr("%"), false);
    addRow(2, 0, &m_radioB, tr("B:"), &m_spinB, 0, 100, tr("%"), false);
    addRow(3, 0, &m_radioR, tr("R:"), &m_spinR, 0, 255, QString(), false);
    addRow(4, 0, &m_radioG, tr("G:"), &m_spinG, 0, 255, QString(), false);
    addRow(5, 0, &m_radioBlue, tr("B:"), &m_spinBlue, 0, 255, QString(), false);

    // Right column: Lab is editable; CMYK is a readout.
    addRow(0, 4, nullptr, tr("L:"), &m_spinL, 0, 100, QString(), false);
    addRow(1, 4, nullptr, tr("a:"), &m_spinLabA, -128, 127, QString(), false);
    addRow(2, 4, nullptr, tr("b:"), &m_spinLabB, -128, 127, QString(), false);
    addRow(3, 4, nullptr, tr("C:"), &m_spinC, 0, 100, tr("%"), true);
    addRow(4, 4, nullptr, tr("M:"), &m_spinM, 0, 100, tr("%"), true);
    addRow(5, 4, nullptr, tr("Y:"), &m_spinY, 0, 100, tr("%"), true);
    addRow(6, 4, nullptr, tr("K:"), &m_spinK, 0, 100, tr("%"), true);

    const QString cmykTip =
        tr("CMYK is a direct conversion without a press profile, so it will not\n"
           "match a color-managed application that has one loaded.");
    for (QSpinBox *box : {m_spinC, m_spinM, m_spinY, m_spinK}) {
        box->setToolTip(cmykTip);
    }
    for (QSpinBox *box : {m_spinL, m_spinLabA, m_spinLabB}) {
        box->setToolTip(tr("CIE L*a*b*, D50 reference white"));
    }

    // Hex, on the bottom-left of the grid.
    grid->addWidget(new QLabel(QStringLiteral("#"), this), 6, 0,
                    Qt::AlignRight | Qt::AlignVCenter);
    m_hex = new QLineEdit(this);
    m_hex->setFixedWidth(72);
    m_hex->setMaxLength(6);
    m_hex->setValidator(new QRegularExpressionValidator(
        QRegularExpression(QStringLiteral("[0-9A-Fa-f]{0,6}")), this));
    grid->addWidget(m_hex, 6, 1, 1, 2);

    grid->setColumnMinimumWidth(3, 14);
    centre->addLayout(grid);
    centre->addStretch(1);
    root->addLayout(centre);

    // -- buttons ------------------------------------------------------------
    auto *buttons = new QVBoxLayout();
    buttons->setSpacing(5);

    auto *ok = new QPushButton(tr("OK"), this);
    ok->setDefault(true);
    auto *cancel = new QPushButton(tr("Cancel"), this);
    auto *addSwatch = new QPushButton(tr("Add to Swatches"), this);
    auto *libraries = new QPushButton(tr("Color Libraries"), this);

    // Present for layout fidelity, but there is nothing behind them yet: the
    // Swatches panel is still a placeholder and no spot-colour libraries ship.
    addSwatch->setEnabled(false);
    addSwatch->setToolTip(tr("The Swatches panel is not implemented yet"));
    libraries->setEnabled(false);
    libraries->setToolTip(tr("Spot color libraries are not implemented yet"));

    for (QPushButton *b : {ok, cancel, addSwatch, libraries}) {
        b->setMinimumWidth(124);
        buttons->addWidget(b);
    }
    buttons->addStretch(1);
    root->addLayout(buttons);

    connect(ok, &QPushButton::clicked, this, &QDialog::accept);
    connect(cancel, &QPushButton::clicked, this, &QDialog::reject);

    // -- wiring -------------------------------------------------------------
    connect(m_plane, &ColorPlane::picked, this, &ColorPickerDialog::onPlanePicked);
    connect(m_ramp, &ColorRamp::picked, this, &ColorPickerDialog::onPlanePicked);
    connect(m_compare, &ColorCompare::originalClicked,
            this, &ColorPickerDialog::revertToOriginal);

    for (QRadioButton *r : {m_radioH, m_radioS, m_radioB, m_radioR, m_radioG,
                            m_radioBlue}) {
        connect(r, &QRadioButton::toggled, this, &ColorPickerDialog::onAxisChanged);
    }
    for (QSpinBox *s : {m_spinH, m_spinS, m_spinB}) {
        connect(s, &QSpinBox::valueChanged, this, &ColorPickerDialog::onHsbFieldsEdited);
    }
    for (QSpinBox *s : {m_spinR, m_spinG, m_spinBlue}) {
        connect(s, &QSpinBox::valueChanged, this, &ColorPickerDialog::onRgbFieldsEdited);
    }
    for (QSpinBox *s : {m_spinL, m_spinLabA, m_spinLabB}) {
        connect(s, &QSpinBox::valueChanged, this, &ColorPickerDialog::onLabFieldsEdited);
    }
    connect(m_hex, &QLineEdit::textEdited, this, &ColorPickerDialog::onHexEdited);
    connect(m_webOnly, &QCheckBox::toggled, this,
            &ColorPickerDialog::onWebColorsToggled);

    // Hue is Photoshop's default axis.
    m_radioH->setChecked(true);
}

ColorAxis ColorPickerDialog::currentAxis() const
{
    if (m_radioS->isChecked()) {
        return ColorAxis::Saturation;
    }
    if (m_radioB->isChecked()) {
        return ColorAxis::Brightness;
    }
    if (m_radioR->isChecked()) {
        return ColorAxis::Red;
    }
    if (m_radioG->isChecked()) {
        return ColorAxis::Green;
    }
    if (m_radioBlue->isChecked()) {
        return ColorAxis::Blue;
    }
    return ColorAxis::Hue;
}

QColor ColorPickerDialog::selectedColor() const
{
    return m_color;
}

/// Adopt an HSV triple. Use for edits that are naturally HSV — the field, the
/// ramp, and the H/S/B boxes.
void ColorPickerDialog::setHsv(int hue, int sat, int val)
{
    m_hue = qBound(0, hue, 359);
    m_sat = qBound(0, sat, 255);
    m_val = qBound(0, val, 255);
    m_color = QColor::fromHsv(m_hue, m_sat, m_val);
}

/// Adopt an RGB colour exactly. Use for edits that are naturally RGB — the
/// R/G/B boxes, hex, and Lab — so the entered value survives untouched.
void ColorPickerDialog::setColor(const QColor &color)
{
    m_color = color.toRgb();
    toHsvPreservingHue(m_color, m_hue, m_sat, m_val);
}

void ColorPickerDialog::syncControls(QWidget *except)
{
    // Re-entrancy guard: every setValue() below emits valueChanged, which would
    // otherwise route straight back into an edit handler.
    m_updating = true;

    const QColor color = selectedColor();

    m_plane->setAxis(currentAxis());
    m_ramp->setAxis(currentAxis());
    m_plane->setHsv(m_hue, m_sat, m_val);
    m_ramp->setHsv(m_hue, m_sat, m_val);
    m_compare->setCurrentColor(color);

    const auto assign = [except](QSpinBox *box, int value) {
        if (box != except) {
            box->setValue(value);
        }
    };

    // HSB, in the units Photoshop displays: degrees and percent.
    assign(m_spinH, m_hue);
    assign(m_spinS, int(std::lround(m_sat * 100.0 / 255.0)));
    assign(m_spinB, int(std::lround(m_val * 100.0 / 255.0)));

    assign(m_spinR, color.red());
    assign(m_spinG, color.green());
    assign(m_spinBlue, color.blue());

    double l = 0.0;
    double a = 0.0;
    double b = 0.0;
    rgbToLab(color, &l, &a, &b);
    assign(m_spinL, int(std::lround(l)));
    assign(m_spinLabA, int(std::lround(a)));
    assign(m_spinLabB, int(std::lround(b)));

    const QColor cmyk = color.toCmyk();
    assign(m_spinC, int(std::lround(cmyk.cyan() * 100.0 / 255.0)));
    assign(m_spinM, int(std::lround(cmyk.magenta() * 100.0 / 255.0)));
    assign(m_spinY, int(std::lround(cmyk.yellow() * 100.0 / 255.0)));
    assign(m_spinK, int(std::lround(cmyk.black() * 100.0 / 255.0)));

    if (m_hex != except) {
        m_hex->setText(color.name().mid(1).toUpper());
    }

    m_updating = false;
}

void ColorPickerDialog::onAxisChanged()
{
    if (m_updating) {
        return;
    }
    // Only the axis changed, not the colour — but the field and ramp both need
    // to re-render against the new mapping.
    syncControls();
}

void ColorPickerDialog::onPlanePicked(int hue, int sat, int val)
{
    if (m_updating) {
        return;
    }
    setHsv(hue, sat, val);
    syncControls();
}

void ColorPickerDialog::onHsbFieldsEdited()
{
    if (m_updating) {
        return;
    }
    // 360° and 0° are the same hue; fold it so the ramp marker does not jump
    // off the end.
    const int hue = m_spinH->value() % 360;
    setHsv(hue,
           int(std::lround(m_spinS->value() * 255.0 / 100.0)),
           int(std::lround(m_spinB->value() * 255.0 / 100.0)));
    syncControls(qobject_cast<QWidget *>(sender()));
}

void ColorPickerDialog::onRgbFieldsEdited()
{
    if (m_updating) {
        return;
    }
    QColor c(m_spinR->value(), m_spinG->value(), m_spinBlue->value());
    if (m_webOnly->isChecked()) {
        c = snapToWebColor(c);
    }
    setColor(c);
    syncControls(qobject_cast<QWidget *>(sender()));
}

void ColorPickerDialog::onLabFieldsEdited()
{
    if (m_updating) {
        return;
    }
    QColor c = labToRgb(m_spinL->value(), m_spinLabA->value(), m_spinLabB->value());
    if (m_webOnly->isChecked()) {
        c = snapToWebColor(c);
    }
    setColor(c);
    syncControls(qobject_cast<QWidget *>(sender()));
}

void ColorPickerDialog::onHexEdited()
{
    if (m_updating) {
        return;
    }
    const QString text = m_hex->text().trimmed();
    // Only react once a full triplet has been typed, so the colour does not
    // lurch around as the user types the first characters.
    if (text.size() != 6) {
        return;
    }
    QColor c(QStringLiteral("#") + text);
    if (!c.isValid()) {
        return;
    }
    if (m_webOnly->isChecked()) {
        c = snapToWebColor(c);
    }
    setColor(c);
    syncControls(m_hex);
}

void ColorPickerDialog::onWebColorsToggled(bool on)
{
    m_plane->setWebColorsOnly(on);
    m_ramp->setWebColorsOnly(on);
    if (on) {
        setColor(snapToWebColor(selectedColor()));
    }
    syncControls();
}

void ColorPickerDialog::revertToOriginal()
{
    setColor(m_original);
    syncControls();
}

QColor ColorPickerDialog::getColor(const QColor &initial, QWidget *parent,
                                   const QString &title)
{
    ColorPickerDialog dialog(initial, parent, title);
    if (dialog.exec() != QDialog::Accepted) {
        return {};
    }
    return dialog.selectedColor();
}
