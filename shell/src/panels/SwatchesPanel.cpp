#include "SwatchesPanel.h"

#include "LayerIcons.h"

#include <QEvent>
#include <QHBoxLayout>
#include <QHelpEvent>
#include <QInputDialog>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QScrollArea>
#include <QToolButton>
#include <QToolTip>
#include <QVBoxLayout>

namespace {

/// A swatch's box, and the gap between two of them. CS6's grid is tight —
/// the squares almost touch.
constexpr int kCell = 17;
constexpr int kGap = 1;
/// The border the grid keeps around the whole block.
constexpr int kMargin = 2;

/// Footer glyphs, matching the Layers panel's.
constexpr int kFooterGlyph = 18;
const QColor kGlyph(0xd4, 0xd4, 0xd4);

/// The ring drawn round the swatch the bin would delete.
const QColor kMarkOuter(0x1e, 0x1e, 0x1e);
const QColor kMarkInner(0xff, 0xff, 0xff);

/// The hues a sweep steps through, in the order the spectrum runs, with the
/// names CS6 uses for them.
struct Hue
{
    int degrees;
    const char *name;
};

const Hue kHues[] = {
    {0, "Red"},         {30, "Orange"},      {60, "Yellow"},  {90, "Chartreuse"},
    {120, "Green"},     {150, "Spring"},     {180, "Cyan"},   {210, "Azure"},
    {240, "Blue"},      {270, "Violet"},     {300, "Magenta"}, {330, "Rose"},
};

} // namespace

// ------------------------------------------------------------------- grid --

SwatchGrid::SwatchGrid(QWidget *parent)
    : QWidget(parent)
{
    setObjectName(QStringLiteral("swatchGrid"));
    setSwatches(defaultSwatches());
    // The name of the colour under the pointer is the whole reason a tooltip
    // is wanted here, so they have to be asked for per position.
    setMouseTracking(true);
}

QList<Swatch> SwatchGrid::defaultSwatches()
{
    QList<Swatch> out;
    const auto add = [&out](const QColor &color, const QString &name) {
        out.append({color, name});
    };

    // The primaries first, as every Photoshop library opens.
    add(QColor(255, 0, 0), QObject::tr("RGB Red"));
    add(QColor(255, 255, 0), QObject::tr("RGB Yellow"));
    add(QColor(0, 255, 0), QObject::tr("RGB Green"));
    add(QColor(0, 255, 255), QObject::tr("RGB Cyan"));
    add(QColor(0, 0, 255), QObject::tr("RGB Blue"));
    add(QColor(255, 0, 255), QObject::tr("RGB Magenta"));

    // Then the sweep, three times over: pure, light, dark. Laid out a row per
    // tone so the panel reads as a spectrum rather than as a scatter.
    struct Tone
    {
        int saturation;
        int value;
        const char *prefix;
    };
    const Tone tones[] = {
        {255, 255, ""},
        {110, 255, "Light "},
        {255, 150, "Dark "},
    };
    for (const Tone &tone : tones) {
        for (const Hue &hue : kHues) {
            // Half steps as well, so a row is long enough to read as a sweep.
            for (int half = 0; half < 2; ++half) {
                const int degrees = (hue.degrees + half * 15) % 360;
                QColor color;
                color.setHsv(degrees, tone.saturation, tone.value);
                const QString name = half == 0
                        ? QObject::tr("%1%2").arg(QString::fromLatin1(tone.prefix),
                                                  QObject::tr(hue.name))
                        : QObject::tr("%1%2 %3°").arg(QString::fromLatin1(tone.prefix),
                                                      QObject::tr(hue.name))
                                  .arg(degrees);
                add(color, name);
            }
        }
    }

    // And the greys, ending on black and white as CS6's default set does.
    for (int step = 0; step <= 8; ++step) {
        const int level = step * 255 / 8;
        add(QColor(level, level, level),
            level == 0       ? QObject::tr("Black")
                : level == 255 ? QObject::tr("White")
                               : QObject::tr("%1% Gray").arg(100 - step * 100 / 8));
    }
    return out;
}

void SwatchGrid::setSwatches(QList<Swatch> swatches)
{
    m_swatches = std::move(swatches);
    m_current = -1;
    updateGeometry();
    update();
    emit countChanged();
    emit currentChanged(m_current);
}

void SwatchGrid::addSwatch(const Swatch &swatch)
{
    m_swatches.append(swatch);
    m_current = int(m_swatches.size()) - 1;
    updateGeometry();
    update();
    emit countChanged();
    emit currentChanged(m_current);
}

bool SwatchGrid::removeSwatch(int index)
{
    if (index < 0 || index >= m_swatches.size()) {
        return false;
    }
    m_swatches.removeAt(index);
    // Hold the place rather than the index: deleting a run of swatches one
    // after another should keep taking the next one along, and the last
    // deletion should leave the mark on what is now the end.
    m_current = m_swatches.isEmpty() ? -1 : qMin(index, int(m_swatches.size()) - 1);
    updateGeometry();
    update();
    emit countChanged();
    emit currentChanged(m_current);
    return true;
}

int SwatchGrid::columnsFor(int width) const
{
    const int usable = width - kMargin * 2 + kGap;
    return qMax(1, usable / (kCell + kGap));
}

int SwatchGrid::columns() const
{
    return columnsFor(width());
}

int SwatchGrid::heightFor(int width) const
{
    const int across = columnsFor(width);
    const int rows = (int(m_swatches.size()) + across - 1) / across;
    return kMargin * 2 + rows * (kCell + kGap);
}

QSize SwatchGrid::sizeHint() const
{
    return QSize(kMargin * 2 + kCell * 8 + kGap * 7, heightFor(width()));
}

QSize SwatchGrid::minimumSizeHint() const
{
    // The height is what matters here: it is what tells the scroll area
    // around the grid whether a scrollbar is needed.
    return QSize(kMargin * 2 + kCell, heightFor(width()));
}

QRect SwatchGrid::cellRect(int index) const
{
    const int across = columns();
    const int column = index % across;
    const int row = index / across;
    return QRect(kMargin + column * (kCell + kGap), kMargin + row * (kCell + kGap), kCell,
                 kCell);
}

int SwatchGrid::indexAt(const QPoint &pos) const
{
    if (m_swatches.isEmpty()) {
        return -1;
    }
    const int across = columns();
    const int column = (pos.x() - kMargin) / (kCell + kGap);
    const int row = (pos.y() - kMargin) / (kCell + kGap);
    if (pos.x() < kMargin || pos.y() < kMargin || column >= across) {
        return -1;
    }
    const int index = row * across + column;
    if (index < 0 || index >= m_swatches.size()) {
        return -1;
    }
    // Inside the square itself, not in the gap past it: the gap belongs to
    // nothing, and treating it as a hit makes the edges of the grid feel
    // sloppy.
    return cellRect(index).contains(pos) ? index : -1;
}

void SwatchGrid::resizeEvent(QResizeEvent *event)
{
    QWidget::resizeEvent(event);
    // How many rows the swatches need has just changed, and the scroll area
    // works that out from the hints above.
    updateGeometry();
}

void SwatchGrid::paintEvent(QPaintEvent *event)
{
    Q_UNUSED(event)
    QPainter painter(this);
    for (int i = 0; i < m_swatches.size(); ++i) {
        const QRect cell = cellRect(i);
        painter.fillRect(cell, m_swatches.at(i).color);
        // A hairline so that a white swatch against the panel is still a
        // square rather than a hole.
        painter.setPen(QPen(QColor(0, 0, 0, 90), 1));
        painter.drawRect(cell.adjusted(0, 0, -1, -1));
    }
    if (m_current >= 0 && m_current < m_swatches.size()) {
        const QRect cell = cellRect(m_current);
        painter.setPen(QPen(kMarkOuter, 1));
        painter.drawRect(cell.adjusted(0, 0, -1, -1));
        painter.setPen(QPen(kMarkInner, 1));
        painter.drawRect(cell.adjusted(1, 1, -2, -2));
    }
}

void SwatchGrid::mousePressEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton) {
        QWidget::mousePressEvent(event);
        return;
    }
    const int index = indexAt(event->pos());
    if (index < 0) {
        // CS6 fills the empty space past the last swatch with a paint bucket:
        // clicking there adds the foreground colour to the set.
        emit newSwatchRequested();
        return;
    }

    // Alt takes a swatch out of the set — CS6's scissors.
    if (event->modifiers() & Qt::AltModifier) {
        removeSwatch(index);
        return;
    }
    m_current = index;
    update();
    emit currentChanged(m_current);
    // Ctrl puts the colour in the background half of the pair, as CS6 does.
    if (event->modifiers() & Qt::ControlModifier) {
        emit backgroundPicked(m_swatches.at(index).color);
    } else {
        emit foregroundPicked(m_swatches.at(index).color);
    }
}

bool SwatchGrid::event(QEvent *event)
{
    if (event->type() == QEvent::ToolTip) {
        auto *help = static_cast<QHelpEvent *>(event);
        const int index = indexAt(help->pos());
        if (index >= 0) {
            QToolTip::showText(help->globalPos(), m_swatches.at(index).name, this);
        } else {
            QToolTip::hideText();
            event->ignore();
        }
        return true;
    }
    return QWidget::event(event);
}

// ------------------------------------------------------------------ panel --

SwatchesPanel::SwatchesPanel(QWidget *parent)
    : QWidget(parent)
{
    setObjectName(QStringLiteral("swatchesPanel"));

    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);

    m_grid = new SwatchGrid(this);
    m_scroll = new QScrollArea(this);
    m_scroll->setObjectName(QStringLiteral("swatchScroll"));
    m_scroll->setWidget(m_grid);
    // The grid follows the viewport's width and sets its own height, which is
    // what makes it reflow as the panel is resized and scroll when the
    // colours no longer fit.
    m_scroll->setWidgetResizable(true);
    m_scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_scroll->setFrameShape(QFrame::NoFrame);
    m_scroll->setAlignment(Qt::AlignTop | Qt::AlignLeft);
    root->addWidget(m_scroll, 1);

    auto *footer = new QWidget(this);
    footer->setObjectName(QStringLiteral("panelFooter"));
    auto *footerLayout = new QHBoxLayout(footer);
    footerLayout->setContentsMargins(4, 2, 4, 2);
    footerLayout->setSpacing(2);
    // CS6 keeps these two hard against the right-hand end.
    footerLayout->addStretch(1);

    auto makeButton = [&](LayerIcons::Glyph glyph, const QString &tip) {
        auto *button = new QToolButton(footer);
        button->setIconSize(QSize(kFooterGlyph, kFooterGlyph));
        button->setIcon(LayerIcons::icon(glyph, kGlyph, kFooterGlyph));
        button->setToolTip(tip);
        button->setAutoRaise(true);
        footerLayout->addWidget(button);
        return button;
    };
    m_newButton = makeButton(LayerIcons::Glyph::NewLayer,
                             tr("Create a new swatch from the foreground colour"));
    m_deleteButton = makeButton(LayerIcons::Glyph::Delete, tr("Delete the marked swatch"));
    root->addWidget(footer);

    connect(m_newButton, &QToolButton::clicked, this,
            &SwatchesPanel::addSwatchFromForeground);
    connect(m_deleteButton, &QToolButton::clicked, this,
            &SwatchesPanel::deleteCurrentSwatch);
    connect(m_grid, &SwatchGrid::newSwatchRequested, this,
            &SwatchesPanel::addSwatchFromForeground);
    connect(m_grid, &SwatchGrid::foregroundPicked, this, &SwatchesPanel::foregroundPicked);
    connect(m_grid, &SwatchGrid::backgroundPicked, this, &SwatchesPanel::backgroundPicked);
    // Nothing marked, nothing to delete.
    connect(m_grid, &SwatchGrid::currentChanged, this,
            [this](int index) { m_deleteButton->setEnabled(index >= 0); });
    m_deleteButton->setEnabled(m_grid->current() >= 0);

    setContextMenuPolicy(Qt::CustomContextMenu);
    connect(this, &QWidget::customContextMenuRequested, this,
            &SwatchesPanel::showContextMenu);
}

void SwatchesPanel::setForegroundColor(const QColor &color)
{
    if (color.isValid()) {
        m_foreground = color;
    }
}

void SwatchesPanel::addSwatchFromForeground()
{
    bool named = false;
    const QString name =
        QInputDialog::getText(this, tr("New Swatch"), tr("Name:"), QLineEdit::Normal,
                              m_foreground.name().toUpper(), &named);
    if (!named) {
        return;
    }
    m_grid->addSwatch({m_foreground, name.isEmpty() ? m_foreground.name().toUpper() : name});
}

void SwatchesPanel::deleteCurrentSwatch()
{
    m_grid->removeSwatch(m_grid->current());
}

void SwatchesPanel::showContextMenu(const QPoint &at)
{
    QMenu menu(this);
    menu.addAction(tr("New Swatch..."), this, &SwatchesPanel::addSwatchFromForeground);
    QAction *remove = menu.addAction(tr("Delete Swatch"), this,
                                     &SwatchesPanel::deleteCurrentSwatch);
    remove->setEnabled(m_grid->current() >= 0);
    menu.addSeparator();
    menu.addAction(tr("Reset Swatches"), this,
                   [this] { m_grid->setSwatches(SwatchGrid::defaultSwatches()); });
    menu.exec(mapToGlobal(at));
}
