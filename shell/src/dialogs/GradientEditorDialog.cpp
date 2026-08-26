#include "GradientEditorDialog.h"
#include "ColorPickerDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QEvent>
#include <QGridLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QLinearGradient>
#include <QMouseEvent>
#include <QPainter>
#include <QPushButton>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QVBoxLayout>

static const char *kPresetNames[] = {
    "Foreground to Background",
    "Foreground to Transparent",
    "Black, White",
    "Red, Green",
    "Violet, Orange",
    "Blue, Red, Yellow",
    "Blue, Yellow, Blue",
    "Orange, Yellow, Orange",
    "Violet, Green, Orange",
    "Yellow, Violet, Orange, Blue",
    "Copper",
    "Chrome",
    "Spectrum",
    "Transparent Rainbow",
    "Transparent Stripes",
};
static constexpr int kPresetCount = static_cast<int>(std::size(kPresetNames));

// ---------------------------------------------------------------------------
// GradientStopBar
// ---------------------------------------------------------------------------

static constexpr int kBarHeight = 24;
static constexpr int kStopSize = 12;
static constexpr int kMargin = 8;

GradientStopBar::GradientStopBar(QWidget *parent)
    : QWidget(parent)
{
    setMinimumHeight(kBarHeight + kStopSize + 4);
    setMouseTracking(true);
}

QSize GradientStopBar::sizeHint() const
{
    return {300, kBarHeight + kStopSize + 4};
}

void GradientStopBar::setStops(const QVector<GradientColorStop> &stops)
{
    m_stops = stops;
    std::sort(m_stops.begin(), m_stops.end(),
              [](const auto &a, const auto &b) { return a.position < b.position; });
    if (m_selected >= m_stops.size())
        m_selected = m_stops.isEmpty() ? -1 : 0;
    update();
}

QRect GradientStopBar::barRect() const
{
    return QRect(kMargin, 0, width() - 2 * kMargin, kBarHeight);
}

int GradientStopBar::hitTest(const QPoint &pos) const
{
    const QRect bar = barRect();
    for (int i = 0; i < m_stops.size(); ++i) {
        int cx = bar.left() + static_cast<int>(m_stops[i].position * bar.width());
        int cy = bar.bottom() + kStopSize / 2 + 2;
        if (QRect(cx - kStopSize / 2, cy - kStopSize / 2, kStopSize, kStopSize)
                .contains(pos))
            return i;
    }
    return -1;
}

float GradientStopBar::posFromX(int x) const
{
    const QRect bar = barRect();
    return qBound(0.0f, static_cast<float>(x - bar.left()) / bar.width(), 1.0f);
}

void GradientStopBar::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing);

    const QRect bar = barRect();

    // Checkerboard behind (for transparency)
    const int cs = 6;
    for (int y = bar.top(); y < bar.bottom(); y += cs)
        for (int x = bar.left(); x < bar.right(); x += cs)
            p.fillRect(QRect(x, y, cs, cs),
                        ((x / cs + y / cs) & 1) ? QColor(200, 200, 200) : Qt::white);

    // Gradient fill
    QLinearGradient lg(bar.left(), 0, bar.right(), 0);
    for (const auto &s : m_stops)
        lg.setColorAt(static_cast<double>(s.position), s.color);
    p.fillRect(bar, lg);

    // Border
    p.setPen(QColor(120, 120, 120));
    p.setBrush(Qt::NoBrush);
    p.drawRect(bar);

    // Stop triangles below the bar
    for (int i = 0; i < m_stops.size(); ++i) {
        int cx = bar.left() + static_cast<int>(m_stops[i].position * bar.width());
        int cy = bar.bottom() + 2;

        QPolygon tri;
        tri << QPoint(cx, cy)
            << QPoint(cx - kStopSize / 2, cy + kStopSize)
            << QPoint(cx + kStopSize / 2, cy + kStopSize);

        p.setPen(i == m_selected ? Qt::black : QColor(100, 100, 100));
        p.setBrush(m_stops[i].color);
        p.drawPolygon(tri);

        if (i == m_selected) {
            // Inner highlight
            p.setPen(Qt::white);
            p.setBrush(Qt::NoBrush);
            QPolygon inner;
            inner << QPoint(cx, cy + 2)
                  << QPoint(cx - kStopSize / 2 + 2, cy + kStopSize - 1)
                  << QPoint(cx + kStopSize / 2 - 2, cy + kStopSize - 1);
            p.drawPolygon(inner);
        }
    }
}

void GradientStopBar::mousePressEvent(QMouseEvent *e)
{
    int hit = hitTest(e->pos());
    if (hit >= 0) {
        m_selected = hit;
        m_dragging = hit;
        m_dragRemove = false;
        emit stopSelected(m_selected);
        update();
        return;
    }

    // Click in the bar area below → add a new stop
    const QRect bar = barRect();
    if (e->pos().y() >= bar.bottom() && e->pos().y() <= bar.bottom() + kStopSize + 4) {
        float pos = posFromX(e->pos().x());

        // Interpolate color from existing gradient at this position
        QLinearGradient lg(0, 0, 1, 0);
        for (const auto &s : m_stops)
            lg.setColorAt(static_cast<double>(s.position), s.color);

        // Sample color: build a 256-wide line and read the pixel
        QImage line(256, 1, QImage::Format_ARGB32);
        QPainter lp(&line);
        QLinearGradient lg2(0, 0, 255, 0);
        for (const auto &s : m_stops)
            lg2.setColorAt(static_cast<double>(s.position), s.color);
        lp.fillRect(line.rect(), lg2);
        lp.end();
        int px = qBound(0, static_cast<int>(pos * 255), 255);
        QColor c = line.pixelColor(px, 0);

        GradientColorStop newStop{pos, c};
        m_stops.append(newStop);
        std::sort(m_stops.begin(), m_stops.end(),
                  [](const auto &a, const auto &b) { return a.position < b.position; });
        // Find the new stop
        for (int i = 0; i < m_stops.size(); ++i) {
            if (qFuzzyCompare(m_stops[i].position, pos)) {
                m_selected = i;
                m_dragging = i;
                break;
            }
        }
        emit stopSelected(m_selected);
        emit stopsChanged();
        update();
    }
}

void GradientStopBar::mouseMoveEvent(QMouseEvent *e)
{
    if (m_dragging < 0) return;

    const QRect bar = barRect();
    float newPos = posFromX(e->pos().x());

    // If dragged far below the bar, mark for removal
    m_dragRemove = (e->pos().y() > bar.bottom() + kStopSize + 30) && m_stops.size() > 2;

    m_stops[m_dragging].position = newPos;
    update();
}

void GradientStopBar::mouseReleaseEvent(QMouseEvent *)
{
    if (m_dragging >= 0) {
        if (m_dragRemove && m_stops.size() > 2) {
            m_stops.remove(m_dragging);
            m_selected = qBound(0, m_selected, m_stops.size() - 1);
            m_dragging = -1;
            emit stopSelected(m_selected);
            emit stopsChanged();
            update();
            return;
        }
        // Re-sort
        float draggedPos = m_stops[m_dragging].position;
        QColor draggedColor = m_stops[m_dragging].color;
        std::sort(m_stops.begin(), m_stops.end(),
                  [](const auto &a, const auto &b) { return a.position < b.position; });
        for (int i = 0; i < m_stops.size(); ++i) {
            if (qFuzzyCompare(m_stops[i].position, draggedPos)
                && m_stops[i].color == draggedColor) {
                m_selected = i;
                break;
            }
        }
        m_dragging = -1;
        emit stopSelected(m_selected);
        emit stopsChanged();
        update();
    }
}

void GradientStopBar::mouseDoubleClickEvent(QMouseEvent *e)
{
    int hit = hitTest(e->pos());
    if (hit >= 0) {
        QColor picked = ColorPickerDialog::getColor(
            m_stops[hit].color, this, tr("Stop Color"));
        if (picked.isValid()) {
            m_stops[hit].color = picked;
            emit stopsChanged();
            update();
        }
    }
}

// ---------------------------------------------------------------------------
// GradientEditorDialog
// ---------------------------------------------------------------------------

QVector<GradientColorStop> GradientEditorDialog::stopsFromPresetName(
    Engine *engine, const QString &name)
{
    QVector<GradientColorStop> result;
    QImage img = engine->gradientPreview(name, 256, 1);
    if (img.isNull()) {
        result.append({0.0f, Qt::black});
        result.append({1.0f, Qt::white});
        return result;
    }

    // Sample the first and last pixel, plus any distinct color transitions
    result.append({0.0f, img.pixelColor(0, 0)});
    result.append({1.0f, img.pixelColor(255, 0)});

    // For a better approximation, sample key positions
    // But for named presets, just use a 2-stop version initially
    // Check a few midpoints for multi-stop gradients
    struct SamplePoint { float pos; int px; };
    SamplePoint samples[] = {{0.25f, 64}, {0.5f, 128}, {0.75f, 192}};

    for (const auto &sp : samples) {
        QColor c = img.pixelColor(sp.px, 0);
        // Check if this color differs significantly from linear interpolation
        QColor interp;
        float t = sp.pos;
        interp.setRed(static_cast<int>(result[0].color.red() * (1 - t) + result.last().color.red() * t));
        interp.setGreen(static_cast<int>(result[0].color.green() * (1 - t) + result.last().color.green() * t));
        interp.setBlue(static_cast<int>(result[0].color.blue() * (1 - t) + result.last().color.blue() * t));
        int diff = qAbs(c.red() - interp.red()) + qAbs(c.green() - interp.green())
                   + qAbs(c.blue() - interp.blue());
        if (diff > 30)
            result.insert(result.size() - 1, {sp.pos, c});
    }

    std::sort(result.begin(), result.end(),
              [](const auto &a, const auto &b) { return a.position < b.position; });
    return result;
}

GradientEditorDialog::GradientEditorDialog(Engine *engine,
                                           const QVector<GradientColorStop> &initial,
                                           QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Gradient Editor"));
    setFixedSize(560, 420);

    auto *outer = new QHBoxLayout(this);

    auto *left = new QVBoxLayout;

    // Presets group
    auto *presetsGroup = new QGroupBox(tr("Presets"));
    auto *presetsScroll = new QScrollArea;
    auto *presetsWidget = new QWidget;
    auto *presetsGrid = new QGridLayout(presetsWidget);
    presetsGrid->setSpacing(2);
    presetsGrid->setContentsMargins(4, 4, 4, 4);

    for (int i = 0; i < kPresetCount; ++i) {
        const QString name = QString::fromUtf8(kPresetNames[i]);
        auto *btn = new QPushButton;
        QImage img = m_engine->gradientPreview(name, 48, 20);
        btn->setIcon(QIcon(QPixmap::fromImage(img)));
        btn->setIconSize(QSize(48, 20));
        btn->setFixedSize(54, 26);
        btn->setToolTip(name);
        presetsGrid->addWidget(btn, i / 7, i % 7);

        connect(btn, &QPushButton::clicked, this, [this, name] {
            auto stops = stopsFromPresetName(m_engine, name);
            m_stopBar->setStops(stops);
            m_nameEdit->setText(name);
            onStopsChanged();
            onStopSelected(m_stopBar->selectedIndex());
        });
    }

    presetsScroll->setWidget(presetsWidget);
    presetsScroll->setWidgetResizable(true);
    presetsScroll->setFixedHeight(90);
    auto *pLayout = new QVBoxLayout(presetsGroup);
    pLayout->addWidget(presetsScroll);
    left->addWidget(presetsGroup);

    // Name row
    auto *nameRow = new QHBoxLayout;
    nameRow->addWidget(new QLabel(tr("Name:")));
    m_nameEdit = new QLineEdit(tr("Custom"));
    nameRow->addWidget(m_nameEdit, 1);
    left->addLayout(nameRow);

    left->addSpacing(8);

    // Gradient stop bar
    m_stopBar = new GradientStopBar;
    m_stopBar->setStops(initial);
    left->addWidget(m_stopBar);

    left->addSpacing(4);

    // Stops info row
    auto *stopsRow = new QHBoxLayout;
    stopsRow->addWidget(new QLabel(tr("Color:")));
    m_colorSwatch = new QLabel;
    m_colorSwatch->setFixedSize(24, 18);
    m_colorSwatch->setCursor(Qt::PointingHandCursor);
    m_colorSwatch->installEventFilter(this);
    stopsRow->addWidget(m_colorSwatch);

    stopsRow->addSpacing(20);
    stopsRow->addWidget(new QLabel(tr("Location:")));
    m_locationSpin = new QSpinBox;
    m_locationSpin->setRange(0, 100);
    m_locationSpin->setSuffix(QStringLiteral(" %"));
    m_locationSpin->setFixedWidth(65);
    stopsRow->addWidget(m_locationSpin);

    auto *deleteBtn = new QPushButton(tr("Delete"));
    deleteBtn->setFixedWidth(55);
    stopsRow->addWidget(deleteBtn);
    stopsRow->addStretch();
    left->addLayout(stopsRow);

    left->addStretch();

    outer->addLayout(left, 1);

    // Right buttons
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(70);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(70);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    // Connections
    connect(m_stopBar, &GradientStopBar::stopsChanged,
            this, &GradientEditorDialog::onStopsChanged);
    connect(m_stopBar, &GradientStopBar::stopSelected,
            this, &GradientEditorDialog::onStopSelected);

    connect(m_locationSpin, QOverload<int>::of(&QSpinBox::valueChanged), this, [this](int v) {
        int sel = m_stopBar->selectedIndex();
        if (sel < 0) return;
        auto stops = m_stopBar->stops();
        stops[sel].position = static_cast<float>(v) / 100.0f;
        m_stopBar->setStops(stops);
        onStopsChanged();
    });

    connect(deleteBtn, &QPushButton::clicked, this, [this] {
        int sel = m_stopBar->selectedIndex();
        auto stops = m_stopBar->stops();
        if (sel >= 0 && stops.size() > 2) {
            stops.remove(sel);
            m_stopBar->setStops(stops);
            onStopsChanged();
            onStopSelected(m_stopBar->selectedIndex());
        }
    });

    connect(okBtn, &QPushButton::clicked, this, &QDialog::accept);
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);

    onStopSelected(m_stopBar->selectedIndex());
}

QVector<GradientColorStop> GradientEditorDialog::resultStops() const
{
    return m_stopBar->stops();
}

void GradientEditorDialog::onStopsChanged()
{
    updatePreview();
}

void GradientEditorDialog::onStopSelected(int index)
{
    auto stops = m_stopBar->stops();
    if (index >= 0 && index < stops.size()) {
        m_colorSwatch->setStyleSheet(
            QStringLiteral("background-color: %1; border: 1px solid #555;")
                .arg(stops[index].color.name()));
        QSignalBlocker b(m_locationSpin);
        m_locationSpin->setValue(static_cast<int>(stops[index].position * 100.0f + 0.5f));
    }
}

void GradientEditorDialog::updatePreview()
{
    // Nothing special needed — the stop bar already shows the gradient
}

void GradientEditorDialog::pickStopColor()
{
    int sel = m_stopBar->selectedIndex();
    auto stops = m_stopBar->stops();
    if (sel < 0 || sel >= stops.size()) return;

    QColor picked = ColorPickerDialog::getColor(stops[sel].color, this, tr("Stop Color"));
    if (!picked.isValid()) return;

    stops[sel].color = picked;
    m_stopBar->setStops(stops);
    onStopsChanged();
    onStopSelected(sel);
}

bool GradientEditorDialog::eventFilter(QObject *obj, QEvent *event)
{
    if (obj == m_colorSwatch && event->type() == QEvent::MouseButtonRelease) {
        pickStopColor();
        return true;
    }
    return QDialog::eventFilter(obj, event);
}
