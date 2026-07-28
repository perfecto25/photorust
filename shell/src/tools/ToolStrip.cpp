#include "ToolStrip.h"

#include "../panels/ColorPanel.h"
#include "../shortcuts/CommandRegistry.h"

#include <QVBoxLayout>
#include <QWidget>

ToolStrip::ToolStrip(CommandRegistry *registry, QWidget *parent)
    : QToolBar(parent)
    , m_registry(registry)
{
    setObjectName(QStringLiteral("toolStrip"));
    setWindowTitle(tr("Tools"));
    setOrientation(Qt::Vertical);
    setMovable(false);
    setFloatable(false);
    setIconSize(QSize(20, 20));

    // Exclusive selection, like a radio group.
    m_group = new QActionGroup(this);
    m_group->setExclusive(true);

    const ToolInfo *table = toolTable();
    for (int i = 0; i < static_cast<int>(ToolId::Count); ++i) {
        const ToolInfo &info = table[i];
        if (info.groupBreak) {
            addSeparator();
        }
        QToolButton *button = createButton(info);
        addWidget(button);
        m_buttons.insert(info.id, button);
    }

    addSeparator();

    // The foreground/background swatch, centred at the bottom of the strip.
    auto *swatchHost = new QWidget(this);
    auto *swatchLayout = new QVBoxLayout(swatchHost);
    swatchLayout->setContentsMargins(0, 4, 0, 4);
    swatchLayout->setSpacing(0);
    m_swatches = new ColorSwatchWidget(swatchHost);
    swatchLayout->addWidget(m_swatches, 0, Qt::AlignHCenter);
    addWidget(swatchHost);

    setActiveTool(ToolId::Brush);
}

QToolButton *ToolStrip::createButton(const ToolInfo &info)
{
    auto *button = new QToolButton(this);
    button->setCheckable(true);
    button->setAutoRaise(true);
    // Placeholder glyphs until real CS6-style icons are drawn; see ToolId.h.
    button->setText(QString::fromUtf8(info.glyph));
    button->setToolButtonStyle(Qt::ToolButtonTextOnly);

    // Bind to the registry command so the shortcut comes from the keymap.
    QAction *action = m_registry
        ? m_registry->registerCommand(QLatin1String(info.commandId),
                                      QString::fromUtf8(info.name))
        : nullptr;

    const ToolId id = info.id;
    QString tip = QString::fromUtf8(info.name);
    if (action && !action->shortcut().isEmpty()) {
        tip += QStringLiteral("  (%1)").arg(
            action->shortcut().toString(QKeySequence::NativeText));
    }
    button->setToolTip(tip);
    button->setStatusTip(QString::fromUtf8(info.name));

    if (action) {
        action->setCheckable(true);
        m_group->addAction(action);
        // Keep button and action in step in both directions: the action fires
        // from the keyboard, the button from a click.
        connect(action, &QAction::triggered, this, [this, id]() { setActiveTool(id); });
        connect(button, &QToolButton::clicked, this, [this, id]() { setActiveTool(id); });

        // The registry may rebind at runtime; refresh the tooltip when it does.
        connect(m_registry, &CommandRegistry::shortcutChanged, button,
                [button, info](const QString &changedId, const QKeySequence &seq) {
                    if (changedId != QLatin1String(info.commandId)) {
                        return;
                    }
                    QString tip = QString::fromUtf8(info.name);
                    if (!seq.isEmpty()) {
                        tip += QStringLiteral("  (%1)").arg(
                            seq.toString(QKeySequence::NativeText));
                    }
                    button->setToolTip(tip);
                });
    } else {
        connect(button, &QToolButton::clicked, this, [this, id]() { setActiveTool(id); });
    }

    return button;
}

void ToolStrip::setActiveTool(ToolId tool)
{
    QToolButton *button = m_buttons.value(tool, nullptr);
    if (!button) {
        return;
    }

    // Uncheck every other button — QToolButtons are not grouped by the
    // QActionGroup, only their actions are.
    for (auto it = m_buttons.constBegin(); it != m_buttons.constEnd(); ++it) {
        it.value()->setChecked(it.key() == tool);
    }

    if (m_activeTool == tool) {
        return;
    }
    m_activeTool = tool;

    if (m_registry) {
        if (QAction *action = m_registry->action(QLatin1String(toolInfo(tool)->commandId))) {
            action->setChecked(true);
        }
    }

    emit toolChanged(tool);
}
