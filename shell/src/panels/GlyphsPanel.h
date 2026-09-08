#pragma once

#include <QFont>
#include <QWidget>

class QComboBox;
class QListWidget;
class QSlider;

/// The Glyphs panel: every character a font actually has, in a grid, with a
/// double-click inserting one into the open type edit.
///
/// The grid is built by asking the font whether it has each code point in a
/// list of Unicode blocks, because neither Qt nor the shell can read a font's
/// character map directly. That covers the blocks a CS6-era Latin font would
/// fill and the symbol blocks its Glyphs panel lists; a font whose coverage
/// lies outside them shows fewer glyphs here than it really has.
class GlyphsPanel : public QWidget
{
    Q_OBJECT

public:
    explicit GlyphsPanel(QWidget *parent = nullptr);

    /// Follow the type tool's font, so the panel opens on the text being
    /// edited. Ignored once the user has picked a family here themselves —
    /// browsing to Wingdings should survive reaching for another tool.
    void setTypeFont(const QFont &font, const QString &styleName);

signals:
    /// A glyph was double-clicked. The family and style travel with it: the
    /// text's own font may have nothing at that code point, and inserting it
    /// regardless would put an empty box in the document.
    void glyphChosen(const QString &text, const QString &family, const QString &style);

private:
    void rebuildStyles(const QString &family, const QString &wanted);
    void rebuildGlyphs();
    /// The font the grid is drawn in, at the current zoom.
    QFont glyphFont() const;

    QComboBox *m_family = nullptr;
    QComboBox *m_style = nullptr;
    QComboBox *m_subset = nullptr;
    QListWidget *m_grid = nullptr;
    QSlider *m_zoom = nullptr;

    /// Set once the family combo is used here, which stops `setTypeFont`
    /// pulling the panel back to whatever the Type tool is set to.
    bool m_familyChosenHere = false;
    bool m_updating = false;
};
