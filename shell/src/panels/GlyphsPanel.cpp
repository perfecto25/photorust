#include "GlyphsPanel.h"

#include <QComboBox>
#include <QFontDatabase>
#include <QFontMetrics>
#include <QHBoxLayout>
#include <QLabel>
#include <QListWidget>
#include <QSlider>
#include <QVBoxLayout>

namespace {

/// A named run of code points, both for the subset filter and for deciding
/// what to scan in the first place.
struct Block {
    const char *name;
    char32_t first;
    char32_t last;
};

/// The blocks the grid looks in. A CS6-era Latin font fills the first few;
/// the rest are the symbol ranges the original panel offers as subsets.
constexpr Block kBlocks[] = {
    {"Basic Latin", 0x0020, 0x007E},
    {"Latin-1 Supplement", 0x00A0, 0x00FF},
    {"Latin Extended-A", 0x0100, 0x017F},
    {"Latin Extended-B", 0x0180, 0x024F},
    {"Greek", 0x0370, 0x03FF},
    {"Cyrillic", 0x0400, 0x04FF},
    {"Punctuation", 0x2000, 0x206F},
    {"Superscripts & Subscripts", 0x2070, 0x209F},
    {"Currency", 0x20A0, 0x20BF},
    {"Letterlike Symbols", 0x2100, 0x214F},
    {"Number Forms", 0x2150, 0x218F},
    {"Arrows", 0x2190, 0x21FF},
    {"Mathematical Operators", 0x2200, 0x22FF},
    {"Geometric Shapes", 0x25A0, 0x25FF},
    {"Miscellaneous Symbols", 0x2600, 0x26FF},
    {"Dingbats", 0x2700, 0x27BF},
};

/// Point size of the glyphs at each end of the zoom slider.
constexpr int kMinGlyphPoints = 10;
constexpr int kMaxGlyphPoints = 40;

/// Space around a glyph in its cell.
constexpr int kCellPadding = 12;

} // namespace

GlyphsPanel::GlyphsPanel(QWidget *parent)
    : QWidget(parent)
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(6, 6, 6, 6);
    root->setSpacing(5);

    // -- font pickers ---------------------------------------------------------
    auto *fontRow = new QHBoxLayout();
    fontRow->setSpacing(4);

    m_family = new QComboBox(this);
    QStringList families;
    for (const QString &name : QFontDatabase::families()) {
        // Left out for the same reason as the Type options bar's list: CS6
        // predates colour emoji fonts entirely.
        if (!name.contains(QLatin1String("Emoji"), Qt::CaseInsensitive)) {
            families << name;
        }
    }
    m_family->addItems(families);
    m_family->setToolTip(tr("Show the glyphs of this font family"));
    fontRow->addWidget(m_family, 2);

    m_style = new QComboBox(this);
    m_style->setToolTip(tr("Show the glyphs of this font style"));
    fontRow->addWidget(m_style, 1);
    root->addLayout(fontRow);

    rebuildStyles(m_family->currentText(), tr("Regular"));

    // -- subset ---------------------------------------------------------------
    m_subset = new QComboBox(this);
    m_subset->addItem(tr("Entire Font"));
    for (const Block &block : kBlocks) {
        m_subset->addItem(tr(block.name));
    }
    m_subset->setToolTip(tr("Show only part of the font"));
    root->addWidget(m_subset);

    // -- the grid -------------------------------------------------------------
    // IconMode with wrapping reflows the glyphs into as many columns as the
    // panel is wide, which is what the original does, and costs nothing to
    // keep right on a resize.
    m_grid = new QListWidget(this);
    m_grid->setViewMode(QListView::IconMode);
    m_grid->setResizeMode(QListView::Adjust);
    m_grid->setMovement(QListView::Static);
    m_grid->setUniformItemSizes(true);
    m_grid->setWrapping(true);
    m_grid->setSelectionMode(QAbstractItemView::SingleSelection);
    m_grid->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    root->addWidget(m_grid, 1);

    // -- zoom -----------------------------------------------------------------
    auto *zoomRow = new QHBoxLayout();
    zoomRow->setSpacing(4);
    zoomRow->addStretch(1);

    auto *small = new QLabel(QStringLiteral("A"), this);
    QFont smallFont = small->font();
    smallFont.setPointSize(8);
    small->setFont(smallFont);
    zoomRow->addWidget(small);

    m_zoom = new QSlider(Qt::Horizontal, this);
    m_zoom->setRange(kMinGlyphPoints, kMaxGlyphPoints);
    m_zoom->setValue(18);
    m_zoom->setFixedWidth(90);
    m_zoom->setToolTip(tr("Glyph size"));
    zoomRow->addWidget(m_zoom);

    auto *large = new QLabel(QStringLiteral("A"), this);
    QFont largeFont = large->font();
    largeFont.setPointSize(13);
    large->setFont(largeFont);
    zoomRow->addWidget(large);
    root->addLayout(zoomRow);

    // -- wiring ---------------------------------------------------------------
    connect(m_family, &QComboBox::currentTextChanged, this, [this](const QString &family) {
        if (!m_updating) {
            m_familyChosenHere = true;
        }
        rebuildStyles(family, m_style->currentText());
        rebuildGlyphs();
    });
    connect(m_style, &QComboBox::currentTextChanged, this, [this] { rebuildGlyphs(); });
    connect(m_subset, &QComboBox::currentIndexChanged, this, [this] { rebuildGlyphs(); });
    connect(m_zoom, &QSlider::valueChanged, this, [this] { rebuildGlyphs(); });
    connect(m_grid, &QListWidget::itemDoubleClicked, this, [this](QListWidgetItem *item) {
        if (item) {
            emit glyphChosen(item->text(), m_family->currentText(), m_style->currentText());
        }
    });

    rebuildGlyphs();
}

void GlyphsPanel::rebuildStyles(const QString &familyName, const QString &wanted)
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

QFont GlyphsPanel::glyphFont() const
{
    // By name rather than by QFont's bold/italic bits, for the reason
    // `MainWindow::pushTypeOptions` looks fonts up the same way: a style is
    // whatever the family calls it, not a pair of flags.
    QFont font = QFontDatabase::font(m_family->currentText(), m_style->currentText(),
                                     m_zoom->value());
    font.setPointSize(m_zoom->value());
    return font;
}

void GlyphsPanel::rebuildGlyphs()
{
    m_grid->clear();

    const QFont font = glyphFont();
    const QFontMetrics metrics(font);

    // Index 0 is "Entire Font"; anything else is the block at index - 1.
    const int chosen = m_subset->currentIndex();
    const int firstBlock = chosen <= 0 ? 0 : chosen - 1;
    const int lastBlock = chosen <= 0 ? int(std::size(kBlocks)) - 1 : chosen - 1;

    const int cell = m_zoom->value() + kCellPadding * 2;
    m_grid->setGridSize(QSize(cell, cell));

    for (int b = firstBlock; b <= lastBlock; ++b) {
        const Block &block = kBlocks[b];
        for (char32_t code = block.first; code <= block.last; ++code) {
            // The only way to ask what a font has: Qt exposes no character
            // map, so each candidate is offered up one at a time.
            if (!metrics.inFontUcs4(code)) {
                continue;
            }
            auto *item = new QListWidgetItem(QString::fromUcs4(&code, 1), m_grid);
            item->setFont(font);
            item->setTextAlignment(Qt::AlignCenter);
            item->setSizeHint(QSize(cell, cell));
            item->setToolTip(
                QStringLiteral("U+%1").arg(uint(code), 4, 16, QLatin1Char('0')).toUpper());
        }
    }
}

void GlyphsPanel::setTypeFont(const QFont &font, const QString &styleName)
{
    if (m_familyChosenHere) {
        return;
    }

    m_updating = true;
    const int idx = m_family->findText(font.family());
    if (idx >= 0) {
        m_family->setCurrentIndex(idx);
    }
    rebuildStyles(m_family->currentText(), styleName);
    m_updating = false;

    rebuildGlyphs();
}
