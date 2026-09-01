#include "CanvasSizeDialog.h"

#include "ColorPickerDialog.h"
#include "photorust_core/src/bridge.cxxqt.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDoubleSpinBox>
#include <QGridLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPushButton>
#include <QVBoxLayout>

#include <cmath>

namespace {

/// Width/Height units, in CS6's order.
enum Unit { UnitPercent = 0, UnitPixels, UnitInches, UnitCm, UnitMm, UnitPoints };

constexpr int kCell = 26;

QString sizeSummary(double bytes)
{
    if (bytes >= 1024.0 * 1024.0) {
        return QStringLiteral("%1M").arg(bytes / (1024.0 * 1024.0), 0, 'f', 2);
    }
    return QStringLiteral("%1K").arg(bytes / 1024.0, 0, 'f', 1);
}

} // namespace

// ------------------------------------------------------------- anchor grid ---

AnchorSelector::AnchorSelector(QWidget *parent)
    : QWidget(parent)
{
    setCursor(Qt::PointingHandCursor);
}

QSize AnchorSelector::sizeHint() const
{
    return QSize(kCell * 3, kCell * 3);
}

QRect AnchorSelector::cellRect(int cx, int cy) const
{
    return QRect(cx * kCell, cy * kCell, kCell, kCell);
}

void AnchorSelector::paintEvent(QPaintEvent *)
{
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing, true);

    const QColor ink(0xE8, 0xE8, 0xE8);
    const QColor frame(0x88, 0x88, 0x88);

    for (int cy = 0; cy < 3; ++cy) {
        for (int cx = 0; cx < 3; ++cx) {
            const QRect cell = cellRect(cx, cy).adjusted(1, 1, -1, -1);
            painter.setPen(QPen(frame, 1.0));
            painter.setBrush(Qt::NoBrush);
            painter.drawRect(cell);

            const QPointF centre = QRectF(cell).center();
            if (cx == m_x && cy == m_y) {
                // The anchored square holds the image itself.
                painter.setPen(Qt::NoPen);
                painter.setBrush(ink);
                painter.drawRect(QRectF(centre.x() - 4, centre.y() - 4, 8, 8));
                continue;
            }

            // Every other square points away from the anchor, showing which
            // way the canvas grows.
            const int dx = cx - m_x;
            const int dy = cy - m_y;
            // Only the eight squares around the anchor carry an arrow; a cell
            // two steps away on both axes still reads from its direction.
            const double len = std::hypot(double(dx), double(dy));
            if (len == 0.0) {
                continue;
            }
            const QPointF dir(dx / len, dy / len);
            const QPointF tip = centre + dir * 7.0;
            const QPointF tail = centre - dir * 6.0;
            const QPointF normal(-dir.y(), dir.x());

            painter.setPen(QPen(ink, 1.4, Qt::SolidLine, Qt::RoundCap));
            painter.drawLine(tail, tip - dir * 3.0);

            QPainterPath head;
            head.moveTo(tip);
            head.lineTo(tip - dir * 5.0 + normal * 3.0);
            head.lineTo(tip - dir * 5.0 - normal * 3.0);
            head.closeSubpath();
            painter.setPen(Qt::NoPen);
            painter.setBrush(ink);
            painter.drawPath(head);
        }
    }
}

void AnchorSelector::mousePressEvent(QMouseEvent *event)
{
    const int cx = event->position().x() / kCell;
    const int cy = event->position().y() / kCell;
    if (cx < 0 || cx > 2 || cy < 0 || cy > 2) {
        return;
    }
    m_x = cx;
    m_y = cy;
    update();
}

// ------------------------------------------------------------------ dialog ---

CanvasSizeDialog::CanvasSizeDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Canvas Size"));
    if (m_engine) {
        m_pixelWidth = m_engine->property("canvasWidth").toInt();
        m_pixelHeight = m_engine->property("canvasHeight").toInt();
        m_resolution = m_engine->imageResolution();
    }
    m_pixelWidth = qMax(1, m_pixelWidth);
    m_pixelHeight = qMax(1, m_pixelHeight);

    buildUi();
    updateNewSize();
}

double CanvasSizeDialog::unitScale(int unitIndex) const
{
    switch (unitIndex) {
    case UnitInches: return m_resolution;
    case UnitCm:     return m_resolution / 2.54;
    case UnitMm:     return m_resolution / 25.4;
    case UnitPoints: return m_resolution / 72.0;
    default:         return 1.0;   // Pixels
    }
}

int CanvasSizeDialog::toPixels(const QDoubleSpinBox *field, int unitIndex, int base) const
{
    const double v = field->value();
    if (unitIndex == UnitPercent) {
        // A percentage is of the current size either way; Relative decides
        // whether the result is added to it or replaces it.
        return qRound(base * v / 100.0);
    }
    return qRound(v * unitScale(unitIndex));
}

int CanvasSizeDialog::resultWidth() const
{
    const int v = toPixels(m_width, m_widthUnit->currentIndex(), m_pixelWidth);
    return qMax(1, m_relative->isChecked() ? m_pixelWidth + v : v);
}

int CanvasSizeDialog::resultHeight() const
{
    const int v = toPixels(m_height, m_heightUnit->currentIndex(), m_pixelHeight);
    return qMax(1, m_relative->isChecked() ? m_pixelHeight + v : v);
}

int CanvasSizeDialog::anchorX() const
{
    return m_anchor->anchorX();
}

int CanvasSizeDialog::anchorY() const
{
    return m_anchor->anchorY();
}

QColor CanvasSizeDialog::extensionColor() const
{
    // Matched by text rather than index: the menu carries separators, so its
    // indices do not line up with the order the entries were added.
    const QString choice = m_extension->currentText();
    if (choice == tr("Foreground")) {
        return m_engine ? m_engine->foregroundColor() : QColor(Qt::black);
    }
    if (choice == tr("Background")) {
        return m_engine ? m_engine->backgroundColor() : QColor(Qt::white);
    }
    if (choice == tr("White")) {
        return QColor(Qt::white);
    }
    if (choice == tr("Black")) {
        return QColor(Qt::black);
    }
    if (choice == tr("Gray")) {
        return QColor(128, 128, 128);
    }
    return m_customColor;
}

void CanvasSizeDialog::buildUi()
{
    auto *outer = new QHBoxLayout(this);
    auto *left = new QVBoxLayout;

    // --- Current Size -----------------------------------------------------
    auto *currentBox = new QGroupBox;
    auto *currentGrid = new QGridLayout(currentBox);
    m_currentSize = new QLabel;
    currentGrid->addWidget(m_currentSize, 0, 0, 1, 2);
    currentGrid->addWidget(new QLabel(tr("Width:")), 1, 0, Qt::AlignRight);
    m_currentWidth = new QLabel;
    currentGrid->addWidget(m_currentWidth, 1, 1);
    currentGrid->addWidget(new QLabel(tr("Height:")), 2, 0, Qt::AlignRight);
    m_currentHeight = new QLabel;
    currentGrid->addWidget(m_currentHeight, 2, 1);
    left->addWidget(currentBox);

    // --- New Size ---------------------------------------------------------
    auto *newBox = new QGroupBox;
    auto *newGrid = new QGridLayout(newBox);
    m_newSize = new QLabel;
    newGrid->addWidget(m_newSize, 0, 0, 1, 3);

    const auto addUnits = [](QComboBox *box) {
        box->addItem(tr("Percent"));
        box->addItem(tr("Pixels"));
        box->addItem(tr("Inches"));
        box->addItem(tr("Centimeters"));
        box->addItem(tr("Millimeters"));
        box->addItem(tr("Points"));
    };

    newGrid->addWidget(new QLabel(tr("Width:")), 1, 0, Qt::AlignRight);
    m_width = new QDoubleSpinBox;
    m_width->setRange(-300000.0, 300000.0);
    newGrid->addWidget(m_width, 1, 1);
    m_widthUnit = new QComboBox;
    addUnits(m_widthUnit);
    newGrid->addWidget(m_widthUnit, 1, 2);

    newGrid->addWidget(new QLabel(tr("Height:")), 2, 0, Qt::AlignRight);
    m_height = new QDoubleSpinBox;
    m_height->setRange(-300000.0, 300000.0);
    newGrid->addWidget(m_height, 2, 1);
    m_heightUnit = new QComboBox;
    addUnits(m_heightUnit);
    newGrid->addWidget(m_heightUnit, 2, 2);

    m_relative = new QCheckBox(tr("Relative"));
    newGrid->addWidget(m_relative, 3, 1);

    newGrid->addWidget(new QLabel(tr("Anchor:")), 4, 0, Qt::AlignRight | Qt::AlignTop);
    m_anchor = new AnchorSelector;
    newGrid->addWidget(m_anchor, 4, 1, 1, 2, Qt::AlignLeft);
    left->addWidget(newBox);

    // --- Canvas extension colour ------------------------------------------
    auto *extRow = new QHBoxLayout;
    extRow->addWidget(new QLabel(tr("Canvas extension color:")));
    m_extension = new QComboBox;
    m_extension->addItem(tr("Foreground"));
    m_extension->addItem(tr("Background"));
    m_extension->insertSeparator(m_extension->count());
    m_extension->addItem(tr("White"));
    m_extension->addItem(tr("Black"));
    m_extension->addItem(tr("Gray"));
    m_extension->insertSeparator(m_extension->count());
    m_extension->addItem(tr("Other..."));
    m_extension->setCurrentIndex(m_extension->findText(tr("Background")));
    extRow->addWidget(m_extension, 1);
    // The swatch is a button, not a readout: in CS6 clicking it opens the
    // colour picker whatever the menu happens to say.
    m_extensionSwatch = new QPushButton;
    m_extensionSwatch->setFixedSize(24, 22);
    m_extensionSwatch->setToolTip(tr("Choose the canvas extension color"));
    m_extensionSwatch->setCursor(Qt::PointingHandCursor);
    extRow->addWidget(m_extensionSwatch);
    left->addLayout(extRow);

    left->addStretch();
    outer->addLayout(left, 1);

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

    const auto refresh = [this] { updateNewSize(); };
    connect(m_width, &QDoubleSpinBox::valueChanged, this, refresh);
    connect(m_height, &QDoubleSpinBox::valueChanged, this, refresh);
    connect(m_widthUnit, &QComboBox::currentIndexChanged, this, [this](int unit) {
        changeUnit(m_width, unit, m_widthUnitPrev, m_pixelWidth);
    });
    connect(m_heightUnit, &QComboBox::currentIndexChanged, this, [this](int unit) {
        changeUnit(m_height, unit, m_heightUnitPrev, m_pixelHeight);
    });
    connect(m_relative, &QCheckBox::toggled, this, &CanvasSizeDialog::onRelativeToggled);
    connect(m_extension, &QComboBox::currentIndexChanged,
            this, &CanvasSizeDialog::onExtensionChanged);
    connect(m_extensionSwatch, &QPushButton::clicked,
            this, &CanvasSizeDialog::pickExtensionColor);

    m_widthUnit->setCurrentIndex(UnitPixels);
    m_heightUnit->setCurrentIndex(UnitPixels);
    m_width->setValue(m_pixelWidth);
    m_height->setValue(m_pixelHeight);
    onExtensionChanged(m_extension->currentIndex());
}

void CanvasSizeDialog::changeUnit(QDoubleSpinBox *field, int newUnit, int &prevUnit, int base)
{
    const int pixels = toPixels(field, prevUnit, base);
    prevUnit = newUnit;

    const QSignalBlocker block(field);
    // Pixels are whole things; the physical units need decimals to be usable.
    field->setDecimals(newUnit == UnitPixels ? 0 : 3);
    if (newUnit == UnitPercent) {
        field->setValue(base > 0 ? pixels * 100.0 / base : 0.0);
    } else {
        field->setValue(pixels / unitScale(newUnit));
    }
    updateNewSize();
}

void CanvasSizeDialog::onRelativeToggled(bool on)
{
    // Relative counts from the current size, so the fields start at zero —
    // "no change" — rather than repeating the dimensions.
    const QSignalBlocker bw(m_width);
    const QSignalBlocker bh(m_height);
    if (on) {
        m_width->setValue(0);
        m_height->setValue(0);
    } else {
        // Back to absolute: restate the current canvas in whichever unit each
        // field is showing. A percentage of the current size is 100.
        const auto restore = [this](QDoubleSpinBox *field, int unit, int base) {
            field->setValue(unit == UnitPercent ? 100.0 : base / unitScale(unit));
        };
        restore(m_width, m_widthUnit->currentIndex(), m_pixelWidth);
        restore(m_height, m_heightUnit->currentIndex(), m_pixelHeight);
    }
    updateNewSize();
}

void CanvasSizeDialog::onExtensionChanged(int)
{
    if (m_extension->currentText() == tr("Other...")) {
        pickExtensionColor();
        return;
    }
    updateSwatch();
}

void CanvasSizeDialog::pickExtensionColor()
{
    const QColor picked =
        ColorPickerDialog::getColor(extensionColor(), this, tr("Canvas Extension Color"));
    if (picked.isValid()) {
        m_customColor = picked;
        // A colour chosen by hand is nobody's preset, so the menu follows it
        // to Other... — blocked, or that would reopen the picker.
        const QSignalBlocker block(m_extension);
        m_extension->setCurrentIndex(m_extension->findText(tr("Other...")));
    }
    updateSwatch();
}

void CanvasSizeDialog::updateSwatch()
{
    m_extensionSwatch->setStyleSheet(
        QStringLiteral("background-color: %1; border: 1px solid #000;")
            .arg(extensionColor().name()));
}

void CanvasSizeDialog::updateNewSize()
{
    const double perPixel = m_engine && m_pixelWidth > 0 && m_pixelHeight > 0
        ? double(m_engine->imageDataBytes()) / (double(m_pixelWidth) * double(m_pixelHeight))
        : 3.0;

    m_currentSize->setText(
        tr("Current Size: %1").arg(sizeSummary(perPixel * m_pixelWidth * m_pixelHeight)));
    m_currentWidth->setText(tr("%1 Pixels").arg(m_pixelWidth));
    m_currentHeight->setText(tr("%1 Pixels").arg(m_pixelHeight));

    const int w = resultWidth();
    const int h = resultHeight();
    m_newSize->setText(tr("New Size: %1").arg(sizeSummary(perPixel * w * h)));
}
