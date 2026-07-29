#pragma once

#include <QActionGroup>
#include <QHash>
#include <QToolBar>
#include <QToolButton>

#include "ToolId.h"

class CommandRegistry;
class ColorSwatchWidget;
class QTimer;

/// A tool strip button.
///
/// Adds the two things a plain `QToolButton` does not do: the small triangle in
/// the bottom-right corner marking a tool that has hidden tools behind it, and
/// the press-and-hold gesture that opens them. Photoshop opens the flyout on a
/// long press or a right-click, and selects the tool on a short click.
class ToolButton : public QToolButton
{
    Q_OBJECT

public:
    ToolButton(ToolId id, QWidget *parent = nullptr);

    ToolId toolId() const { return m_id; }

signals:
    /// A short click: activate this tool.
    void activated(ToolId id);
    /// A long press or right-click: show the hidden tools.
    void flyoutRequested(ToolId id);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;

private:
    ToolId m_id;
    QTimer *m_holdTimer = nullptr;
    /// Set once the flyout has opened, so releasing does not also select.
    bool m_flyoutShown = false;
};

/// The single-column tool strip down the left edge.
///
/// Buttons are mutually exclusive and bound to the `tool.*` commands in the
/// registry, so their shortcuts come from `shortcuts.json` rather than being
/// hard-coded here (CLAUDE.md §9). Below the tools sit the CS6 footer: the
/// foreground/background swatch, the Quick Mask toggle and the screen-mode
/// button.
class ToolStrip : public QToolBar
{
    Q_OBJECT

public:
    explicit ToolStrip(CommandRegistry *registry, QWidget *parent = nullptr);

    ToolId activeTool() const { return m_activeTool; }

    /// The active variant of a tool, indexing its `subTools()` list.
    /// Each tool remembers its own last-used variant, as CS6 does.
    int toolVariant(ToolId tool) const;

    /// The active variant of the active tool.
    int activeVariant() const { return toolVariant(m_activeTool); }

    /// Select a tool. `variant` of -1 keeps whichever variant that tool was
    /// last using.
    void setActiveTool(ToolId tool, int variant = -1);

    /// Select a tool and advance to the next variant sharing its shortcut
    /// letter — the Shift+letter gesture.
    void cycleVariant(ToolId tool);

    /// The swatch widget, so the Color panel can stay in sync with it.
    ColorSwatchWidget *swatches() const { return m_swatches; }

    /// Whether Quick Mask mode is on.
    bool quickMaskEnabled() const;

signals:
    /// The active tool or its variant changed. `variant` indexes the tool's
    /// `subTools()` list.
    void toolChanged(ToolId tool, int variant);
    void quickMaskToggled(bool enabled);

private slots:
    /// Open the hidden-tools flyout for a button.
    void showFlyout(ToolId id);
    void onQuickMaskToggled(bool on);

private:
    ToolButton *createButton(const ToolInfo &info);
    void createFooter();
    /// Tooltip text in CS6's form: "Brush Tool  (B)".
    QString tooltipFor(const ToolInfo &info, const QKeySequence &shortcut) const;

    CommandRegistry *m_registry = nullptr;
    QActionGroup *m_group = nullptr;
    QHash<ToolId, ToolButton *> m_buttons;

    ColorSwatchWidget *m_swatches = nullptr;
    QToolButton *m_quickMask = nullptr;
    QToolButton *m_screenMode = nullptr;

    ToolId m_activeTool = ToolId::Brush;
    /// Last-used variant per tool. Absent means variant 0.
    QHash<ToolId, int> m_variants;
};
