#pragma once

#include <QWidget>

class QDoubleSpinBox;
class QToolButton;

/// The Paragraph panel: the three alignments, and the indents and spacing
/// the text record carries.
///
/// Hyphenate and the justify variants stay disabled, because both need line
/// wrapping and point text has none — there is no measure for a word to run
/// past. The right indent is offered because it still means something
/// without wrapping: it moves right-aligned text in from its own edge.
class ParagraphPanel : public QWidget
{
    Q_OBJECT

public:
    explicit ParagraphPanel(QWidget *parent = nullptr);

    /// Push the type tool's current alignment in, without echoing it back out
    /// through `alignmentChanged` — see `CharacterPanel::setValues` for why.
    void setValues(Qt::Alignment alignment, bool vertical);

    /// Push a layer's indents and spacing in, without echoing them back out.
    void setParagraph(qreal indentLeft, qreal indentRight, qreal firstLine,
                      qreal spaceBefore, qreal spaceAfter);

signals:
    void alignmentChanged(Qt::Alignment alignment);
    /// Any of the five indent/spacing fields moved, all reported together —
    /// the engine takes them as a set.
    void paragraphChanged(qreal indentLeft, qreal indentRight, qreal firstLine,
                          qreal spaceBefore, qreal spaceAfter);

private:
    void rebuildAlignmentIcons();

    QToolButton *m_alignLeft = nullptr;
    QToolButton *m_alignCenter = nullptr;
    QToolButton *m_alignRight = nullptr;
    QDoubleSpinBox *m_indentLeft = nullptr;
    QDoubleSpinBox *m_indentRight = nullptr;
    QDoubleSpinBox *m_firstLine = nullptr;
    QDoubleSpinBox *m_spaceBefore = nullptr;
    QDoubleSpinBox *m_spaceAfter = nullptr;

    bool m_vertical = false;
    bool m_updating = false;
};
