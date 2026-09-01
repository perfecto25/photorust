#include "FillLayerDialog.h"

#include "AngleDial.h"
#include "photorust_core/src/bridge.cxxqt.h"

#include <QCheckBox>
#include <QGridLayout>
#include <QMenu>
#include <QPainter>
#include <QPixmap>
#include <QToolButton>
#include <QWidgetAction>
#include <QComboBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

namespace {

/// The OK / Cancel column CS6 puts down the right of both dialogs.
QVBoxLayout *buttonColumn(QDialog *dialog)
{
    auto *buttons = new QVBoxLayout;
    auto *ok = new QPushButton(QObject::tr("OK"));
    ok->setDefault(true);
    ok->setFixedWidth(90);
    auto *cancel = new QPushButton(QObject::tr("Cancel"));
    cancel->setFixedWidth(90);
    buttons->addWidget(ok);
    buttons->addWidget(cancel);
    buttons->addStretch();
    QObject::connect(ok, &QPushButton::clicked, dialog, &QDialog::accept);
    QObject::connect(cancel, &QPushButton::clicked, dialog, &QDialog::reject);
    return buttons;
}

} // namespace

// ------------------------------------------------------------- gradient ---

GradientFillDialog::GradientFillDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Gradient Fill"));

    auto *outer = new QHBoxLayout(this);
    auto *form = new QFormLayout;

    // The swatch drops CS6's preset grid. The engine renders both the swatch
    // and the grid, so neither can drift from what the layer draws.
    m_gradient = new QPushButton;
    m_gradient->setFixedSize(150, 20);
    if (m_engine) {
        const QStringList names =
            m_engine->gradientPresetNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts);
        m_preset = names.value(0);
    }
    paintPreset();
    connect(m_gradient, &QPushButton::clicked, this, &GradientFillDialog::showPresets);
    form->addRow(tr("Gradient:"), m_gradient);

    m_shape = new QComboBox;
    m_shape->addItems({tr("Linear"), tr("Radial"), tr("Angle"), tr("Reflected"),
                       tr("Diamond")});
    connect(m_shape, &QComboBox::currentIndexChanged, this, [this] { apply(); });
    form->addRow(tr("Style:"), m_shape);

    // The dial and the number are two views of one angle.
    auto *angleRow = new QWidget;
    auto *angleLayout = new QHBoxLayout(angleRow);
    angleLayout->setContentsMargins(0, 0, 0, 0);
    m_dial = new AngleDial;
    m_angle = new QDoubleSpinBox;
    m_angle->setRange(-360, 360);
    m_angle->setDecimals(0);
    m_angle->setValue(90);
    m_angle->setSuffix(QStringLiteral("°"));
    m_angle->setFixedWidth(70);
    m_dial->setAngle(90);
    angleLayout->addWidget(m_dial);
    angleLayout->addWidget(m_angle);
    angleLayout->addStretch();
    connect(m_dial, &AngleDial::angleChanged, this, [this](double degrees) {
        if (m_updating) {
            return;
        }
        m_updating = true;
        m_angle->setValue(degrees);
        m_updating = false;
        apply();
    });
    connect(m_angle, &QDoubleSpinBox::valueChanged, this, [this](double degrees) {
        if (m_updating) {
            return;
        }
        m_updating = true;
        m_dial->setAngle(degrees);
        m_updating = false;
        apply();
    });
    form->addRow(tr("Angle:"), angleRow);

    m_scale = new QSpinBox;
    m_scale->setRange(10, 400);
    m_scale->setValue(100);
    m_scale->setSuffix(QStringLiteral("%"));
    connect(m_scale, &QSpinBox::valueChanged, this, [this] { apply(); });
    form->addRow(tr("Scale:"), m_scale);

    auto *toggles = new QHBoxLayout;
    m_reverse = new QCheckBox(tr("Reverse"));
    m_dither = new QCheckBox(tr("Dither"));
    toggles->addWidget(m_reverse);
    toggles->addWidget(m_dither);
    toggles->addStretch();
    form->addRow(QString(), toggles);

    m_align = new QCheckBox(tr("Align with layer"));
    m_align->setChecked(true);
    form->addRow(QString(), m_align);
    for (QCheckBox *box : {m_reverse, m_dither, m_align}) {
        connect(box, &QCheckBox::toggled, this, [this] { apply(); });
    }

    outer->addLayout(form, 1);
    outer->addLayout(buttonColumn(this));

    // Show the layer as it stands before anything is touched.
    apply();
}

void GradientFillDialog::apply()
{
    if (!m_engine) {
        return;
    }
    m_engine->updateGradientFillPreview(m_preset, shape(), float(angle()), scalePercent(),
                                        reverse(), dither(), alignWithLayer());
    if (auto *window = parentWidget()) {
        window->update();
    }
}

void GradientFillDialog::paintPreset()
{
    if (!m_engine) {
        return;
    }
    const QImage strip = m_engine->gradientPreview(m_preset, 148, 18);
    if (!strip.isNull()) {
        QPixmap swatch = QPixmap::fromImage(strip);
        m_gradient->setIcon(QIcon(swatch));
        m_gradient->setIconSize(swatch.size());
    }
    m_gradient->setToolTip(m_preset);
}

void GradientFillDialog::showPresets()
{
    if (!m_engine) {
        return;
    }
    const QStringList names =
        m_engine->gradientPresetNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts);

    // A grid of strips rather than a list of words: a gradient is a thing you
    // recognise by looking at it.
    QMenu menu(this);
    auto *page = new QWidget(&menu);
    auto *grid = new QGridLayout(page);
    grid->setContentsMargins(6, 6, 6, 6);
    grid->setSpacing(4);
    for (int i = 0; i < names.size(); ++i) {
        const QString name = names.at(i);
        auto *swatch = new QToolButton(page);
        swatch->setAutoRaise(true);
        swatch->setToolTip(name);
        const QImage strip = m_engine->gradientPreview(name, 56, 22);
        if (!strip.isNull()) {
            swatch->setIcon(QIcon(QPixmap::fromImage(strip)));
            swatch->setIconSize(strip.size());
        }
        connect(swatch, &QToolButton::clicked, this, [this, name, &menu] {
            m_preset = name;
            paintPreset();
            apply();
            menu.close();
        });
        grid->addWidget(swatch, i / 5, i % 5);
    }
    auto *holder = new QWidgetAction(&menu);
    holder->setDefaultWidget(page);
    menu.addAction(holder);
    menu.exec(m_gradient->mapToGlobal(QPoint(0, m_gradient->height())));
}

int GradientFillDialog::shape() const
{
    return m_shape->currentIndex();
}

double GradientFillDialog::angle() const
{
    return m_angle->value();
}

int GradientFillDialog::scalePercent() const
{
    return m_scale->value();
}

bool GradientFillDialog::reverse() const
{
    return m_reverse->isChecked();
}

bool GradientFillDialog::dither() const
{
    return m_dither->isChecked();
}

bool GradientFillDialog::alignWithLayer() const
{
    return m_align->isChecked();
}

// -------------------------------------------------------------- pattern ---

PatternFillDialog::PatternFillDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Pattern Fill"));

    auto *outer = new QHBoxLayout(this);
    auto *form = new QFormLayout;

    m_pattern = new QComboBox;
    if (engine) {
        // The engine owns the set — generated tiles, not Photoshop's artwork —
        // so the list cannot drift from what gets drawn.
        m_pattern->addItems(
            engine->patternNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts));
    }
    form->addRow(tr("Pattern:"), m_pattern);

    m_scale = new QSpinBox;
    m_scale->setRange(10, 1000);
    m_scale->setValue(100);
    m_scale->setSuffix(QStringLiteral("%"));
    form->addRow(tr("Scale:"), m_scale);

    m_link = new QCheckBox(tr("Link with Layer"));
    m_link->setChecked(true);
    m_link->setToolTip(tr("Anchor the tiling to the layer, so moving it takes the "
                          "pattern along."));
    form->addRow(QString(), m_link);

    connect(m_pattern, &QComboBox::currentIndexChanged, this, [this] { apply(); });
    connect(m_scale, &QSpinBox::valueChanged, this, [this] { apply(); });
    connect(m_link, &QCheckBox::toggled, this, [this] { apply(); });

    outer->addLayout(form, 1);
    outer->addLayout(buttonColumn(this));

    apply();
}

void PatternFillDialog::apply()
{
    if (!m_engine) {
        return;
    }
    m_engine->updatePatternFillPreview(pattern(), scalePercent(), linkWithLayer());
    if (auto *window = parentWidget()) {
        window->update();
    }
}

int PatternFillDialog::pattern() const
{
    return m_pattern->currentIndex();
}

int PatternFillDialog::scalePercent() const
{
    return m_scale->value();
}

bool PatternFillDialog::linkWithLayer() const
{
    return m_link->isChecked();
}
