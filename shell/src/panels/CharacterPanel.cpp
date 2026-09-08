#include "CharacterPanel.h"

#include "../dialogs/ColorPickerDialog.h"
#include "../tools/ToolId.h"

#include <QComboBox>
#include <QDoubleSpinBox>
#include <QFontDatabase>
#include <QGridLayout>
#include <QIntValidator>
#include <QLabel>
#include <QPainter>
#include <QSpinBox>
#include <QToolButton>
#include <QVBoxLayout>

namespace {

/// Kept in step with the Type options bar's own preset list
/// (`MainWindow::addTypeOptions`), so the two size combos offer the same
/// common point sizes.
const int kSizePresets[] = {6, 7, 8, 9, 10, 11, 12, 14, 18, 24, 30, 36,
                            48, 60, 72, 96, 144, 192, 288};

} // namespace

CharacterPanel::CharacterPanel(QWidget *parent)
    : QWidget(parent)
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(6, 6, 6, 6);
    root->setSpacing(5);

    m_family = new QComboBox(this);
    QStringList families;
    for (const QString &name : QFontDatabase::families()) {
        // CS6 predates system emoji fonts; see MainWindow::addTypeOptions for
        // why this list leaves them out too.
        if (!name.contains(QLatin1String("Emoji"), Qt::CaseInsensitive)) {
            families << name;
        }
    }
    m_family->addItems(families);
    m_family->setToolTip(tr("Set the font family"));
    root->addWidget(m_family);

    m_style = new QComboBox(this);
    m_style->setToolTip(tr("Set the font style"));
    root->addWidget(m_style);
    refreshStyles(m_family->currentText(), tr("Regular"));

    auto *grid = new QGridLayout();
    grid->setSpacing(4);
    grid->setContentsMargins(0, 4, 0, 0);

    auto addField = [&](int row, int col, const QString &label, QWidget *field) {
        grid->addWidget(new QLabel(label, this), row, col * 2);
        grid->addWidget(field, row, col * 2 + 1);
    };

    m_size = new QComboBox(this);
    m_size->setEditable(true);
    m_size->setValidator(new QIntValidator(1, 1296, m_size));
    for (int pt : kSizePresets) {
        m_size->addItem(QString::number(pt));
    }
    m_size->setCurrentText(QString::number(TypeDefaults::kSize));
    m_size->setToolTip(tr("Set the font size"));
    addField(0, 0, tr("Size:"), m_size);

    // Leading, tracking, baseline shift and scale: CS6 has all four, but
    // nothing downstream of the Type tool tracks them per character run yet,
    // so they are shown for shape and disabled rather than silently doing
    // nothing when touched.
    auto *leading = new QComboBox(this);
    leading->addItem(tr("Auto"));
    for (int pt : kSizePresets) {
        leading->addItem(QString::number(pt));
    }
    leading->setEnabled(false);
    leading->setToolTip(tr("Leading — not implemented yet"));
    addField(0, 1, tr("Leading:"), leading);

    auto *tracking = new QSpinBox(this);
    tracking->setRange(-1000, 1000);
    tracking->setEnabled(false);
    tracking->setToolTip(tr("Tracking — not implemented yet"));
    addField(1, 0, tr("Tracking:"), tracking);

    auto *baseline = new QDoubleSpinBox(this);
    baseline->setRange(-1000, 1000);
    baseline->setSuffix(QStringLiteral(" pt"));
    baseline->setEnabled(false);
    baseline->setToolTip(tr("Baseline shift — not implemented yet"));
    addField(1, 1, tr("Baseline Shift:"), baseline);

    // These two are live: the text record carries a per-run horizontal and
    // vertical scale, which is also where a non-uniform Free Transform of a
    // type layer lands.
    m_hScale = new QSpinBox(this);
    m_hScale->setRange(1, 1000);
    m_hScale->setValue(100);
    m_hScale->setSuffix(QStringLiteral("%"));
    m_hScale->setToolTip(tr("Stretch the characters horizontally"));
    addField(2, 0, tr("Horiz. Scale:"), m_hScale);

    m_vScale = new QSpinBox(this);
    m_vScale->setRange(1, 1000);
    m_vScale->setValue(100);
    m_vScale->setSuffix(QStringLiteral("%"));
    m_vScale->setToolTip(tr("Stretch the characters vertically"));
    addField(2, 1, tr("Vert. Scale:"), m_vScale);

    m_colorSwatch = new QToolButton(this);
    m_colorSwatch->setFixedSize(22, 22);
    m_colorSwatch->setToolTip(tr("Set the text color"));
    refreshSwatch();
    addField(3, 0, tr("Color:"), m_colorSwatch);

    // The same list as Type ▸ Anti-Alias and the options bar, separator and
    // all, so the three never disagree about what is on offer.
    m_antialias = new QComboBox(this);
    for (const QString &method : TypeDefaults::antialiasMethods()) {
        if (method.isEmpty()) {
            m_antialias->insertSeparator(m_antialias->count());
        } else {
            m_antialias->addItem(method);
        }
    }
    m_antialias->setCurrentText(TypeDefaults::defaultAntialiasMethod());
    m_antialias->setToolTip(tr("Set the anti-aliasing method"));
    addField(3, 1, tr("Anti-alias:"), m_antialias);

    grid->setColumnStretch(1, 1);
    grid->setColumnStretch(3, 1);
    root->addLayout(grid);
    root->addStretch(1);

    connect(m_family, &QComboBox::currentTextChanged, this, [this](const QString &family) {
        if (m_updating) {
            return;
        }
        refreshStyles(family, m_style->currentText());
        emit familyChanged(family);
    });
    connect(m_style, &QComboBox::currentTextChanged, this, [this](const QString &style) {
        if (!m_updating) {
            emit styleChanged(style);
        }
    });
    connect(m_size, &QComboBox::currentTextChanged, this, [this](const QString &text) {
        if (m_updating) {
            return;
        }
        bool ok = false;
        const double pt = text.toDouble(&ok);
        if (ok && pt > 0) {
            emit sizeChanged(pt);
        }
    });
    connect(m_colorSwatch, &QToolButton::clicked, this, [this] {
        const QColor picked = ColorPickerDialog::getColor(m_color, this, tr("Text Color"));
        if (picked.isValid()) {
            m_color = picked;
            refreshSwatch();
            emit colorChanged(m_color);
        }
    });
    connect(m_antialias, &QComboBox::currentTextChanged, this, [this](const QString &text) {
        if (!m_updating && !text.isEmpty()) {
            emit antialiasChanged(text);
        }
    });
    connect(m_hScale, &QSpinBox::valueChanged, this, [this](int percent) {
        if (!m_updating) {
            emit horizontalScaleChanged(percent / 100.0);
        }
    });
    connect(m_vScale, &QSpinBox::valueChanged, this, [this](int percent) {
        if (!m_updating) {
            emit verticalScaleChanged(percent / 100.0);
        }
    });
}

void CharacterPanel::refreshStyles(const QString &familyName, const QString &wanted)
{
    const QSignalBlocker blocker(m_style);
    m_style->clear();
    QStringList styles = QFontDatabase::styles(familyName);
    if (styles.isEmpty()) {
        styles << tr("Regular");
    }
    m_style->addItems(styles);
    const int idx = m_style->findText(wanted);
    m_style->setCurrentIndex(idx >= 0 ? idx : 0);
}

void CharacterPanel::refreshSwatch()
{
    QPixmap pm(16, 16);
    pm.fill(m_color);
    QPainter p(&pm);
    p.setPen(QColor(0, 0, 0, 160));
    p.drawRect(pm.rect().adjusted(0, 0, -1, -1));
    m_colorSwatch->setIcon(QIcon(pm));
}

void CharacterPanel::setValues(const QFont &font, const QString &styleName, const QColor &color,
                               const QString &antialiasMethod, qreal hScale, qreal vScale)
{
    m_updating = true;

    const int familyIdx = m_family->findText(font.family());
    m_family->setCurrentIndex(familyIdx >= 0 ? familyIdx : 0);
    refreshStyles(font.family(), styleName);

    m_size->setCurrentText(QString::number(int(font.pointSizeF())));

    m_color = color;
    refreshSwatch();

    m_antialias->setCurrentText(antialiasMethod);
    m_hScale->setValue(qRound(hScale * 100.0));
    m_vScale->setValue(qRound(vScale * 100.0));

    m_updating = false;
}
