#include "ColorRangeDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QImage>
#include <QLabel>
#include <QSlider>
#include <QSpinBox>
#include <QToolButton>
#include <QVBoxLayout>

namespace {

/// The Select list, in the order the engine numbers them.
QStringList rangeNames()
{
    return {QObject::tr("Sampled Colors"), QObject::tr("Reds"),       QObject::tr("Yellows"),
            QObject::tr("Greens"),         QObject::tr("Cyans"),      QObject::tr("Blues"),
            QObject::tr("Magentas"),       QObject::tr("Highlights"), QObject::tr("Midtones"),
            QObject::tr("Shadows")};
}

/// Longest side of the preview thumbnail.
constexpr int kPreviewSize = 220;

/// Set by `MainWindow`, which owns the canvas the dropper picks from.
ColorRangeDialog::CursorHook g_cursorHook;

} // namespace

void ColorRangeDialog::setCursorHook(CursorHook hook)
{
    g_cursorHook = std::move(hook);
}

ColorRangeDialog::ColorRangeDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Color Range"));

    auto *root = new QVBoxLayout(this);
    auto *form = new QGridLayout();

    form->addWidget(new QLabel(tr("Select:"), this), 0, 0);
    m_select = new QComboBox(this);
    const QStringList names = rangeNames();
    for (int i = 0; i < names.size(); ++i) {
        m_select->addItem(names.at(i), i);
        // CS6 rules off between the colour bands and the tonal ones.
        if (i == 0 || i == 6) {
            m_select->insertSeparator(m_select->count());
        }
    }
    form->addWidget(m_select, 0, 1, 1, 2);

    form->addWidget(new QLabel(tr("Fuzziness:"), this), 1, 0);
    m_fuzziness = new QSpinBox(this);
    m_fuzziness->setRange(0, 200);
    m_fuzziness->setValue(40);
    form->addWidget(m_fuzziness, 1, 1);
    m_fuzzinessSlider = new QSlider(Qt::Horizontal, this);
    m_fuzzinessSlider->setRange(0, 200);
    m_fuzzinessSlider->setValue(40);
    form->addWidget(m_fuzzinessSlider, 2, 0, 1, 3);

    // The colour Sampled Colors matches against, and the eyedropper that sets
    // it. The swatch doubles as the readout, so there is no doubt what is
    // being matched.
    form->addWidget(new QLabel(tr("Color:"), this), 3, 0);
    m_swatch = new QLabel(this);
    m_swatch->setFixedSize(44, 18);
    m_swatch->setFrameShape(QFrame::Box);
    m_swatch->setAutoFillBackground(true);
    form->addWidget(m_swatch, 3, 1);
    m_eyedropper = new QToolButton(this);
    m_eyedropper->setCheckable(true);
    m_eyedropper->setChecked(true);
    m_eyedropper->setText(tr("Sample"));
    m_eyedropper->setToolTip(tr("Click the image to sample the colour to match"));
    form->addWidget(m_eyedropper, 3, 2);
    root->addLayout(form);

    m_invert = new QCheckBox(tr("Invert"), this);
    root->addWidget(m_invert);

    m_preview = new QLabel(this);
    m_preview->setAlignment(Qt::AlignCenter);
    m_preview->setMinimumHeight(kPreviewSize / 2);
    m_preview->setFrameShape(QFrame::Box);
    root->addWidget(m_preview, 1);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    root->addWidget(buttons);

    // Start on the foreground colour, which is usually the one just sampled
    // with the eyedropper — the same assumption CS6 makes.
    if (m_engine) {
        m_sampled = m_engine->foregroundColor();
    }

    connect(m_select, &QComboBox::currentIndexChanged, this, [this] {
        refreshEnabled();
        refreshPreview();
    });
    connect(m_fuzzinessSlider, &QSlider::valueChanged, m_fuzziness, &QSpinBox::setValue);
    connect(m_fuzziness, &QSpinBox::valueChanged, m_fuzzinessSlider, &QSlider::setValue);
    connect(m_fuzziness, &QSpinBox::valueChanged, this, [this] { refreshPreview(); });
    connect(m_invert, &QCheckBox::toggled, this, [this] { refreshPreview(); });
    connect(m_eyedropper, &QToolButton::toggled, this, [this] { refreshSamplingCursor(); });

    refreshEnabled();
    refreshPreview();
}

int ColorRangeDialog::range() const
{
    return m_select->currentData().toInt();
}

int ColorRangeDialog::fuzziness() const
{
    return m_fuzziness->value();
}

bool ColorRangeDialog::inverted() const
{
    return m_invert->isChecked();
}

void ColorRangeDialog::takeSample(const QPoint &documentPos, const QColor &color)
{
    Q_UNUSED(documentPos)
    // Only while the dropper is down, and only when there is a colour for it
    // to be: the tonal and hue bands are defined without one.
    if (!color.isValid() || !m_eyedropper->isChecked() || range() != 0) {
        return;
    }
    m_sampled = color;
    refreshPreview();
}

void ColorRangeDialog::refreshSamplingCursor()
{
    if (!g_cursorHook) {
        return;
    }
    if (m_eyedropper->isChecked() && range() == 0 && isVisible()) {
        static const QCursor dropper(Qt::CrossCursor);
        g_cursorHook(&dropper);
    } else {
        g_cursorHook(nullptr);
    }
}

void ColorRangeDialog::hideEvent(QHideEvent *event)
{
    // The canvas must not be left wearing the dropper after this window goes.
    if (g_cursorHook) {
        g_cursorHook(nullptr);
    }
    QDialog::hideEvent(event);
}

void ColorRangeDialog::refreshEnabled()
{
    const int chosen = range();
    const bool sampled = chosen == 0;
    // A colour band is defined by its hue, so there is nothing to sample for
    // it; CS6 greys its eyedroppers out the same way.
    m_swatch->setEnabled(sampled);
    m_eyedropper->setEnabled(sampled);
    refreshSamplingCursor();

    QPalette palette = m_swatch->palette();
    palette.setColor(QPalette::Window, sampled ? m_sampled : QColor(Qt::transparent));
    m_swatch->setPalette(palette);
}

void ColorRangeDialog::refreshPreview()
{
    if (!m_engine) {
        return;
    }
    QPalette palette = m_swatch->palette();
    palette.setColor(QPalette::Window, m_sampled);
    m_swatch->setPalette(palette);

    const int width = m_engine->getCanvasWidth();
    const int height = m_engine->getCanvasHeight();
    if (width <= 0 || height <= 0) {
        return;
    }

    const rust::Vec<uint8_t> mask =
        m_engine->colorRangeMask(range(), m_sampled, fuzziness(), inverted());
    if (int(mask.size()) != width * height) {
        return;
    }

    // Greyscale, exactly as Photoshop previews it: white is selected, black
    // is not, and the greys in between are the partly selected pixels a soft
    // edge is made of.
    QImage image(width, height, QImage::Format_Grayscale8);
    for (int y = 0; y < height; ++y) {
        uchar *row = image.scanLine(y);
        for (int x = 0; x < width; ++x) {
            row[x] = mask[size_t(y) * size_t(width) + size_t(x)];
        }
    }
    m_preview->setPixmap(QPixmap::fromImage(image).scaled(
        kPreviewSize, kPreviewSize, Qt::KeepAspectRatio, Qt::SmoothTransformation));
}
