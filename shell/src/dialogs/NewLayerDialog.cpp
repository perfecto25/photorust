#include "NewLayerDialog.h"

#include "panels/LayerIcons.h"
#include "photorust_core/src/bridge.cxxqt.h"

#include <QCheckBox>
#include <QComboBox>
#include <QIcon>
#include <QPixmap>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

#include <algorithm>

NewLayerDialog::NewLayerDialog(Engine *engine, bool fromBackground, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(fromBackground ? tr("New Layer from Background") : tr("New Layer"));

    auto *outer = new QHBoxLayout(this);
    auto *left = new QGridLayout;
    int row = 0;

    left->addWidget(new QLabel(tr("Name:")), row, 0, Qt::AlignRight);
    m_name = new QLineEdit;
    // The Background always becomes "Layer 0"; an ordinary new layer takes the
    // next free "Layer N", which the engine numbers.
    m_name->setText(fromBackground ? QStringLiteral("Layer 0") : QString());
    m_name->setPlaceholderText(tr("Layer"));
    m_name->setMinimumWidth(220);
    left->addWidget(m_name, row, 1, 1, 2);
    ++row;

    if (!fromBackground) {
        m_clipping = new QCheckBox(tr("Use Previous Layer to Create Clipping Mask"));
        left->addWidget(m_clipping, row, 1, 1, 2);
        ++row;
    }

    // CS6's Color is the row colour in the Layers panel, not the layer's own
    // — it is there to find a layer by in a tall stack.
    left->addWidget(new QLabel(tr("Color:")), row, 0, Qt::AlignRight);
    m_label = new QComboBox;
    const QStringList labels = LayerIcons::labelNames();
    for (int i = 0; i < labels.size(); ++i) {
        const QColor colour = LayerIcons::labelColor(i);
        if (colour.isValid()) {
            QPixmap swatch(14, 14);
            swatch.fill(colour);
            m_label->addItem(QIcon(swatch), labels.at(i));
        } else {
            m_label->addItem(labels.at(i));
        }
    }
    left->addWidget(m_label, row, 1, 1, 2);
    ++row;

    left->addWidget(new QLabel(tr("Mode:")), row, 0, Qt::AlignRight);
    m_mode = new QComboBox;
    populateBlendModes();
    left->addWidget(m_mode, row, 1);

    auto *opacityRow = new QHBoxLayout;
    opacityRow->addWidget(new QLabel(tr("Opacity:")));
    m_opacity = new QSpinBox;
    m_opacity->setRange(0, 100);
    m_opacity->setValue(100);
    m_opacity->setSuffix(QStringLiteral("%"));
    opacityRow->addWidget(m_opacity);
    left->addLayout(opacityRow, row, 2);

    outer->addLayout(left, 1);

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

    m_name->setFocus();
    m_name->selectAll();
}

void NewLayerDialog::populateBlendModes()
{
    if (!m_engine) {
        return;
    }
    // The engine owns the list and its order, so the combo cannot drift out of
    // sync with the BlendMode discriminants — the same arrangement the Layers
    // panel uses.
    const QStringList names =
        m_engine->blendModeNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    m_mode->addItems(names);

    QList<int> positions;
    const QString spec = m_engine->blendModeSeparators();
    for (const QString &part : spec.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
        bool ok = false;
        const int at = part.toInt(&ok);
        if (ok) {
            positions.append(at);
        }
    }
    // In reverse, so the earlier positions stay valid as rows shift down.
    std::sort(positions.begin(), positions.end(), std::greater<int>());
    for (int at : positions) {
        if (at > 0 && at < m_mode->count()) {
            m_mode->insertSeparator(at);
        }
    }
}

void NewLayerDialog::presetName(const QString &name)
{
    m_name->setText(name);
    m_name->selectAll();
}

QString NewLayerDialog::layerName() const
{
    return m_name->text().trimmed();
}

int NewLayerDialog::blendMode() const
{
    // Separators occupy rows without being modes, so the discriminant is the
    // count of real entries above the current one.
    int value = 0;
    for (int row = 0; row < m_mode->currentIndex(); ++row) {
        if (!m_mode->itemText(row).isEmpty()) {
            ++value;
        }
    }
    return value;
}

int NewLayerDialog::labelColor() const
{
    return m_label ? m_label->currentIndex() : 0;
}

int NewLayerDialog::opacityPercent() const
{
    return m_opacity->value();
}

bool NewLayerDialog::useClippingMask() const
{
    return m_clipping && m_clipping->isChecked();
}
