#pragma once

#include <QActionGroup>
#include <QHash>
#include <QToolBar>
#include <QToolButton>

#include "ToolId.h"

class CommandRegistry;
class ColorSwatchWidget;

/// The single-column tool strip down the left edge.
///
/// Buttons are mutually exclusive and are bound to the `tool.*` commands in the
/// registry, so their shortcuts come from `shortcuts.json` rather than being
/// hard-coded here (CLAUDE.md §9). The foreground/background swatch sits at the
/// bottom, as in CS6.
class ToolStrip : public QToolBar
{
    Q_OBJECT

public:
    explicit ToolStrip(CommandRegistry *registry, QWidget *parent = nullptr);

    ToolId activeTool() const { return m_activeTool; }

    /// Select a tool programmatically, e.g. from a menu item.
    void setActiveTool(ToolId tool);

    /// The swatch widget, so the Color panel can stay in sync with it.
    ColorSwatchWidget *swatches() const { return m_swatches; }

signals:
    void toolChanged(ToolId tool);

private:
    QToolButton *createButton(const ToolInfo &info);

    CommandRegistry *m_registry = nullptr;
    QActionGroup *m_group = nullptr;
    QHash<ToolId, QToolButton *> m_buttons;
    ColorSwatchWidget *m_swatches = nullptr;
    ToolId m_activeTool = ToolId::Brush;
};
