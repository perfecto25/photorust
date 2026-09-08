#include "ParagraphPanel.h"

#include "../tools/ToolIcons.h"

#include <QButtonGroup>
#include <QCheckBox>
#include <QDoubleSpinBox>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QToolButton>
#include <QVBoxLayout>

namespace {

/// Matches the Type options bar's own glyph tint (`kOptionsIconColor` in
/// MainWindow.cpp), which is file-local there.
const QColor kGlyphColor(0xd4, 0xd4, 0xd4);

/// CS6 pairs "Justify all lines" with the three plain alignments; the three
/// finer last-line variants beside it need a real paragraph layout to mean
/// anything, so this panel does not pretend to offer them.
QString justifySvg()
{
    return QStringLiteral(R"SVG(<path d="M3 5H17M3 10H17M3 15H17" stroke-width="1.6"/>)SVG");
}

} // namespace

ParagraphPanel::ParagraphPanel(QWidget *parent)
    : QWidget(parent)
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(6, 6, 6, 6);
    root->setSpacing(6);

    auto *alignRow = new QHBoxLayout();
    alignRow->setSpacing(2);

    auto *alignGroup = new QButtonGroup(this);
    alignGroup->setExclusive(true);

    auto makeAlignButton = [&](Qt::Alignment align, const QString &tip) {
        auto *button = new QToolButton(this);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setIconSize(QSize(20, 20));
        button->setToolTip(tip);
        alignGroup->addButton(button, int(align));
        alignRow->addWidget(button);
        return button;
    };

    m_alignLeft = makeAlignButton(Qt::AlignLeft, tr("Left align text"));
    m_alignCenter = makeAlignButton(Qt::AlignHCenter, tr("Center text"));
    m_alignRight = makeAlignButton(Qt::AlignRight, tr("Right align text"));
    m_alignLeft->setChecked(true);

    auto *justify = new QToolButton(this);
    justify->setAutoRaise(true);
    justify->setIconSize(QSize(20, 20));
    justify->setIcon(ToolIcons::fromSvgBody(justifySvg(), kGlyphColor));
    justify->setEnabled(false);
    justify->setToolTip(tr("Justify all lines — not implemented yet"));
    alignRow->addWidget(justify);

    alignRow->addStretch(1);
    root->addLayout(alignRow);

    connect(alignGroup, &QButtonGroup::idClicked, this, [this](int id) {
        if (!m_updating) {
            emit alignmentChanged(Qt::Alignment(id));
        }
    });

    rebuildAlignmentIcons();

    // The five indents and spacings the text record carries. They are in
    // document pixels; CS6 shows points, and the two are the same here
    // because the document model has no DPI for type to scale against.
    auto *grid = new QGridLayout();
    grid->setSpacing(4);
    grid->setContentsMargins(0, 0, 0, 0);

    auto addField = [&](int row, int col, const QString &label, QDoubleSpinBox *&field) {
        grid->addWidget(new QLabel(label, this), row, col * 2);
        field = new QDoubleSpinBox(this);
        field->setRange(-10000, 10000);
        field->setSuffix(QStringLiteral(" pt"));
        grid->addWidget(field, row, col * 2 + 1);
    };

    addField(0, 0, tr("Indent Left:"), m_indentLeft);
    addField(0, 1, tr("Indent Right:"), m_indentRight);
    addField(1, 0, tr("First Line:"), m_firstLine);
    addField(2, 0, tr("Space Before:"), m_spaceBefore);
    addField(2, 1, tr("Space After:"), m_spaceAfter);

    grid->setColumnStretch(1, 1);
    grid->setColumnStretch(3, 1);
    root->addLayout(grid);

    for (QDoubleSpinBox *field : {m_indentLeft, m_indentRight, m_firstLine, m_spaceBefore,
                                  m_spaceAfter}) {
        connect(field, &QDoubleSpinBox::valueChanged, this, [this] {
            if (!m_updating) {
                emit paragraphChanged(m_indentLeft->value(), m_indentRight->value(),
                                      m_firstLine->value(), m_spaceBefore->value(),
                                      m_spaceAfter->value());
            }
        });
    }

    auto *hyphenate = new QCheckBox(tr("Hyphenate"), this);
    hyphenate->setEnabled(false);
    // Hyphenation is about where a word may break when a line runs out of
    // room, and point text never runs out: there is no measure to wrap
    // against. It needs paragraph text before it can mean anything.
    hyphenate->setToolTip(tr("Hyphenate — needs paragraph text, which is not implemented"));
    root->addWidget(hyphenate);

    root->addStretch(1);
}

void ParagraphPanel::setParagraph(qreal indentLeft, qreal indentRight, qreal firstLine,
                                  qreal spaceBefore, qreal spaceAfter)
{
    m_updating = true;
    m_indentLeft->setValue(indentLeft);
    m_indentRight->setValue(indentRight);
    m_firstLine->setValue(firstLine);
    m_spaceBefore->setValue(spaceBefore);
    m_spaceAfter->setValue(spaceAfter);
    m_updating = false;
}

void ParagraphPanel::rebuildAlignmentIcons()
{
    // Vertical type runs down the page, so the same three buttons mean top,
    // centre and bottom — the Type options bar turns their icons a quarter
    // turn to say so, and this panel follows suit.
    m_alignLeft->setIcon(
        ToolIcons::fromSvgBody(ToolIcons::textAlignSvg(Qt::AlignLeft, m_vertical), kGlyphColor));
    m_alignCenter->setIcon(ToolIcons::fromSvgBody(
        ToolIcons::textAlignSvg(Qt::AlignHCenter, m_vertical), kGlyphColor));
    m_alignRight->setIcon(
        ToolIcons::fromSvgBody(ToolIcons::textAlignSvg(Qt::AlignRight, m_vertical), kGlyphColor));

    m_alignLeft->setToolTip(m_vertical ? tr("Top align text") : tr("Left align text"));
    m_alignRight->setToolTip(m_vertical ? tr("Bottom align text") : tr("Right align text"));
}

void ParagraphPanel::setValues(Qt::Alignment alignment, bool vertical)
{
    m_updating = true;

    if (m_vertical != vertical) {
        m_vertical = vertical;
        rebuildAlignmentIcons();
    }

    m_alignLeft->setChecked(alignment == Qt::AlignLeft);
    m_alignCenter->setChecked(alignment == Qt::AlignHCenter);
    m_alignRight->setChecked(alignment == Qt::AlignRight);

    m_updating = false;
}
