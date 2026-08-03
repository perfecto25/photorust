#include "InfoPanel.h"

#include "../tools/ToolIcons.h"
#include "../tools/ToolId.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QFrame>
#include <QIcon>
#include <QGridLayout>
#include <QLabel>
#include <QVBoxLayout>

namespace {

/// Tint for the small eyedropper/crosshair glyphs, matching panel text.
const QColor kGlyphColor(0xd4, 0xd4, 0xd4);

/// Shown in place of a number when there is nothing to read.
const char *const kBlank = "";

/// Format a byte count the way Photoshop's "Doc:" line does.
QString formatBytes(double bytes)
{
    if (bytes >= 1024.0 * 1024.0) {
        return QStringLiteral("%1M").arg(bytes / (1024.0 * 1024.0), 0, 'f', 1);
    }
    return QStringLiteral("%1K").arg(bytes / 1024.0, 0, 'f', 1);
}

} // namespace

InfoPanel::InfoPanel(Engine *engine, QWidget *parent)
    : QWidget(parent)
    , m_engine(engine)
{
    setObjectName(QStringLiteral("infoPanel"));

    auto *outer = new QVBoxLayout(this);
    outer->setContentsMargins(6, 6, 6, 6);
    outer->setSpacing(6);

    m_grid = new QGridLayout;
    m_grid->setContentsMargins(0, 0, 0, 0);
    m_grid->setHorizontalSpacing(10);
    m_grid->setVerticalSpacing(8);
    m_grid->setColumnStretch(0, 1);
    m_grid->setColumnStretch(1, 1);
    outer->addLayout(m_grid);

    // Row 0: the two colour readouts. CS6 labels both with the bit depth.
    const QIcon pipette = ToolIcons::icon(ToolId::Eyedropper, kGlyphColor);
    m_rgb = addReadout(m_grid, 0, 0, {QStringLiteral("R"), QStringLiteral("G"),
                                      QStringLiteral("B")},
                       pipette, tr("8-bit"));
    m_cmyk = addReadout(m_grid, 0, 1, {QStringLiteral("C"), QStringLiteral("M"),
                                       QStringLiteral("Y"), QStringLiteral("K")},
                        pipette, tr("8-bit"));

    // Row 1: cursor position and selection size.
    m_position = addReadout(m_grid, 1, 0, {QStringLiteral("X"), QStringLiteral("Y")},
                            ToolIcons::fromSvgBody(ToolIcons::crosshairSvg(), kGlyphColor));
    m_size = addReadout(m_grid, 1, 1, {QStringLiteral("W"), QStringLiteral("H")},
                        ToolIcons::fromSvgBody(ToolIcons::boundsSvg(), kGlyphColor));

    outer->addStretch(1);

    auto *rule = new QFrame(this);
    rule->setFrameShape(QFrame::HLine);
    rule->setObjectName(QStringLiteral("infoRule"));
    outer->addWidget(rule);

    m_docSize = new QLabel(this);
    m_docSize->setObjectName(QStringLiteral("infoDocSize"));
    outer->addWidget(m_docSize);

    auto *rule2 = new QFrame(this);
    rule2->setFrameShape(QFrame::HLine);
    rule2->setObjectName(QStringLiteral("infoRule"));
    outer->addWidget(rule2);

    m_hint = new QLabel(this);
    m_hint->setObjectName(QStringLiteral("infoHint"));
    m_hint->setWordWrap(true);
    outer->addWidget(m_hint);

    setHint(tr("Click image to place new color sampler."));
    refresh();
}

InfoPanel::Readout *InfoPanel::addReadout(QGridLayout *grid, int row, int column,
                                          const QStringList &keys, const QIcon &icon,
                                          const QString &footer, const QString &tag)
{
    auto *block = new QWidget(this);
    block->setObjectName(QStringLiteral("infoBlock"));
    auto *layout = new QGridLayout(block);
    layout->setContentsMargins(4, 3, 4, 3);
    layout->setHorizontalSpacing(4);
    layout->setVerticalSpacing(1);

    // CS6 puts the sampler number on the first row and the glyph below it; an
    // untagged block just has the glyph beside the whole stack of rows.
    int glyphRow = 0;
    int glyphSpan = keys.size();
    if (!tag.isEmpty()) {
        auto *label = new QLabel(tag, block);
        label->setObjectName(QStringLiteral("infoTag"));
        label->setAlignment(Qt::AlignTop | Qt::AlignLeft);
        layout->addWidget(label, 0, 0);
        glyphRow = 1;
        glyphSpan = qMax(1, keys.size() - 1);
    }

    auto *glyph = new QLabel(block);
    glyph->setPixmap(icon.pixmap(QSize(14, 14)));
    glyph->setAlignment(Qt::AlignTop | Qt::AlignLeft);
    layout->addWidget(glyph, glyphRow, 0, glyphSpan, 1, Qt::AlignTop);

    auto *readout = new Readout;
    for (int i = 0; i < keys.size(); ++i) {
        auto *key = new QLabel(keys.at(i) + QStringLiteral(" :"), block);
        key->setObjectName(QStringLiteral("infoKey"));
        key->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
        layout->addWidget(key, i, 1);

        auto *value = new QLabel(QLatin1String(kBlank), block);
        value->setObjectName(QStringLiteral("infoValue"));
        value->setAlignment(Qt::AlignLeft | Qt::AlignVCenter);
        // Reserve room for three digits so the column does not jitter as the
        // numbers change under the cursor.
        value->setMinimumWidth(26);
        layout->addWidget(value, i, 2);
        readout->values.append(value);
    }
    layout->setColumnStretch(2, 1);

    if (!footer.isEmpty()) {
        auto *label = new QLabel(footer, block);
        label->setObjectName(QStringLiteral("infoFooter"));
        label->setAlignment(Qt::AlignHCenter);
        layout->addWidget(label, keys.size(), 0, 1, 3);
    }

    grid->addWidget(block, row, column);
    readout->widget = block;
    return readout;
}

void InfoPanel::setValues(Readout *readout, const QStringList &values)
{
    if (!readout) {
        return;
    }
    for (int i = 0; i < readout->values.size(); ++i) {
        readout->values.at(i)->setText(i < values.size() ? values.at(i)
                                                         : QLatin1String(kBlank));
    }
}

void InfoPanel::setRulerMode(bool on)
{
    if (m_rulerMode == on) {
        return;
    }
    m_rulerMode = on;

    // Tear the old top-right block out of the grid and build its replacement
    // in the same cell.
    if (m_cmyk) {
        delete m_cmyk->widget;
        delete m_cmyk;
        m_cmyk = nullptr;
    }

    if (m_rulerMode) {
        m_cmyk = addReadout(m_grid, 0, 1, {QStringLiteral("A"), QStringLiteral("L")},
                            ToolIcons::fromSvgBody(ToolIcons::protractorSvg(), kGlyphColor));
    } else {
        m_cmyk = addReadout(m_grid, 0, 1,
                            {QStringLiteral("C"), QStringLiteral("M"), QStringLiteral("Y"),
                             QStringLiteral("K")},
                            ToolIcons::icon(ToolId::Eyedropper, kGlyphColor), tr("8-bit"));
    }

    // The W/H block changes what it reports, so refill it from the right side.
    if (m_rulerMode) {
        refreshRuler();
    } else {
        refreshSelection();
    }
}

void InfoPanel::refreshRuler()
{
    if (!m_engine || !m_rulerMode) {
        return;
    }
    // [X, Y, W, H, A, D1]; empty when no ruler has been drawn.
    const rust::Vec<float> m = m_engine->rulerMeasurement();
    if (m.size() < 6) {
        setValues(m_cmyk, {});
        setValues(m_size, {});
        return;
    }

    const auto number = [](float v) { return QStringLiteral("%1").arg(v, 0, 'f', 1); };
    setValues(m_cmyk, {QStringLiteral("%1°").arg(m[4], 0, 'f', 1), number(m[5])});
    setValues(m_size, {number(m[2]), number(m[3])});
}

void InfoPanel::setCursorPosition(const QPointF &documentPos)
{
    if (!m_engine) {
        return;
    }

    const int x = int(std::floor(documentPos.x()));
    const int y = int(std::floor(documentPos.y()));
    setValues(m_position, {QString::number(x), QString::number(y)});

    // Off the canvas there is no pixel to report, so the colour blocks blank
    // rather than holding the last value they saw.
    if (x < 0 || y < 0 || x >= m_engine->getCanvasWidth() || y >= m_engine->getCanvasHeight()) {
        setValues(m_rgb, {});
        if (!m_rulerMode) {
            setValues(m_cmyk, {});
        }
        return;
    }

    const QColor c = m_engine->pickColor(x, y);
    setValues(m_rgb, {QString::number(c.red()), QString::number(c.green()),
                      QString::number(c.blue())});

    // In ruler mode that block holds the angle and length instead.
    if (m_rulerMode) {
        return;
    }

    const QColor k = c.toCmyk();
    const auto pct = [](int v) { return QString::number(int(std::lround(v * 100.0 / 255.0))); };
    setValues(m_cmyk, {pct(k.cyan()), pct(k.magenta()), pct(k.yellow()), pct(k.black())});
}

void InfoPanel::clearCursorPosition()
{
    setValues(m_position, {});
    setValues(m_rgb, {});
    if (!m_rulerMode) {
        setValues(m_cmyk, {});
    }
}

void InfoPanel::refreshSelection()
{
    // In ruler mode the W/H block belongs to the ruler, not the selection.
    if (!m_engine || m_rulerMode) {
        return;
    }
    // `selectionBounds` is [x, y, w, h], all zero when nothing is selected.
    const rust::Vec<::std::int32_t> bounds = m_engine->selectionBounds();
    if (bounds.size() < 4 || (bounds[2] == 0 && bounds[3] == 0)) {
        setValues(m_size, {});
        return;
    }
    setValues(m_size, {QString::number(bounds[2]), QString::number(bounds[3])});
}

void InfoPanel::refreshSamplers()
{
    // Tear down the previous sampler rows. The blocks above them are fixed, so
    // only the widgets this method created are removed.
    qDeleteAll(m_samplerWidgets);
    m_samplerWidgets.clear();
    qDeleteAll(m_samplers);
    m_samplers.clear();

    if (!m_engine) {
        return;
    }

    const int kind = static_cast<int>(MarkerKind::ColorSampler);
    const int count = m_engine->markerCount(kind);
    const QIcon pipette = ToolIcons::icon(ToolId::Eyedropper, kGlyphColor);

    for (int i = 0; i < count; ++i) {
        const int row = kSamplerFirstRow + i / 2;
        const int column = i % 2;

        // Photoshop labels each block with its sampler number.
        Readout *block = addReadout(m_grid, row, column,
                                    {QStringLiteral("R"), QStringLiteral("G"),
                                     QStringLiteral("B")},
                                    pipette, QString(),
                                    QStringLiteral("#%1").arg(i + 1));
        if (QLayoutItem *item = m_grid->itemAtPosition(row, column)) {
            if (QWidget *widget = item->widget()) {
                m_samplerWidgets.append(widget);
            }
        }
        m_samplers.append(block);

        const rust::Vec<::std::int32_t> p = m_engine->markerAt(kind, i);
        if (p.size() < 2) {
            continue;
        }
        const QColor c = m_engine->pickColor(p[0], p[1]);
        setValues(block, {QString::number(c.red()), QString::number(c.green()),
                          QString::number(c.blue())});
    }
}

void InfoPanel::refreshDocumentSize()
{
    if (!m_engine || !m_docSize) {
        return;
    }
    const rust::Vec<double> size = m_engine->documentSizeBytes();
    if (size.size() < 2) {
        m_docSize->clear();
        return;
    }
    m_docSize->setText(tr("Doc: %1/%2").arg(formatBytes(size[0]), formatBytes(size[1])));
}

void InfoPanel::setHint(const QString &hint)
{
    if (m_hint) {
        m_hint->setText(hint);
    }
}

void InfoPanel::refresh()
{
    refreshSamplers();
    refreshSelection();
    refreshRuler();
    refreshDocumentSize();
}
