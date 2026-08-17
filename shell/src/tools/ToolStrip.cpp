#include "ToolStrip.h"

#include "../panels/ColorPanel.h"
#include "../shortcuts/CommandRegistry.h"
#include "ToolIcons.h"

#include <QAction>
#include <QFrame>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QTimer>
#include <QVBoxLayout>
#include <QWidget>

namespace {

/// CS6's tool buttons are 26px square in the single-column layout.
constexpr int kButtonSize = 26;
/// Breathing room between the buttons and the panel edge, as CS6 has.
constexpr int kPanelMargin = 4;
/// How long a press must be held before the flyout opens.
constexpr int kHoldMs = 400;

/// The icon colour. CS6 draws tool glyphs in the same light grey as panel text.
const QColor kIconColor(0xd4, 0xd4, 0xd4);

} // namespace

// =============================================================== ToolButton

ToolButton::ToolButton(ToolId id, QWidget *parent)
    : QToolButton(parent)
    , m_id(id)
{
    setCheckable(true);
    setAutoRaise(true);
    setFixedSize(kButtonSize, kButtonSize);
    setIcon(ToolIcons::icon(id, kIconColor));
    setIconSize(QSize(20, 20));
    setToolButtonStyle(Qt::ToolButtonIconOnly);
    // The flyout is a press-and-hold, so the context menu must not also fire.
    setContextMenuPolicy(Qt::PreventContextMenu);

    m_holdTimer = new QTimer(this);
    m_holdTimer->setSingleShot(true);
    m_holdTimer->setInterval(kHoldMs);
    connect(m_holdTimer, &QTimer::timeout, this, [this] {
        if (hasFlyout(m_id)) {
            m_flyoutShown = true;
            emit flyoutRequested(m_id);
        }
    });
}

void ToolButton::paintEvent(QPaintEvent *event)
{
    QToolButton::paintEvent(event);

    if (!hasFlyout(m_id)) {
        return;
    }

    // The corner triangle marking hidden tools: a small filled wedge tucked
    // into the bottom-right, pointing down-right as CS6 draws it.
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, false);
    p.setPen(Qt::NoPen);
    p.setBrush(kIconColor);

    const int x = width() - 3;
    const int y = height() - 3;
    const QPoint corner[3] = {
        QPoint(x - 4, y),
        QPoint(x, y),
        QPoint(x, y - 4),
    };
    p.drawPolygon(corner, 3);
}

void ToolButton::mousePressEvent(QMouseEvent *event)
{
    m_flyoutShown = false;

    if (event->button() == Qt::RightButton) {
        // Right-click opens the flyout immediately, with no hold.
        if (hasFlyout(m_id)) {
            m_flyoutShown = true;
            emit flyoutRequested(m_id);
        }
        return;
    }

    if (event->button() == Qt::LeftButton) {
        m_holdTimer->start();
    }
    QToolButton::mousePressEvent(event);
}

void ToolButton::mouseReleaseEvent(QMouseEvent *event)
{
    m_holdTimer->stop();
    QToolButton::mouseReleaseEvent(event);

    // A release after the flyout opened is the user dismissing it, not a click
    // on the button underneath.
    if (event->button() == Qt::LeftButton && !m_flyoutShown && rect().contains(event->pos())) {
        emit activated(m_id);
    }
}

// ================================================================ ToolStrip

ToolStrip::ToolStrip(CommandRegistry *registry, QWidget *parent)
    : QWidget(parent)
    , m_registry(registry)
{
    setObjectName(QStringLiteral("toolStrip"));
    // A plain QWidget only paints its stylesheet background once asked to.
    setAttribute(Qt::WA_StyledBackground, true);
    setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Preferred);

    // Exclusive selection, like a radio group.
    m_group = new QActionGroup(this);
    m_group->setExclusive(true);

    auto *outer = new QVBoxLayout(this);
    outer->setContentsMargins(kPanelMargin, kPanelMargin, kPanelMargin, kPanelMargin);
    outer->setSpacing(0);

    m_toolGrid = new QGridLayout;
    m_toolGrid->setContentsMargins(0, 0, 0, 0);
    m_toolGrid->setSpacing(0);
    outer->addLayout(m_toolGrid);

    m_footerGrid = new QGridLayout;
    m_footerGrid->setContentsMargins(0, 0, 0, 0);
    m_footerGrid->setSpacing(0);
    outer->addLayout(m_footerGrid);
    outer->addStretch(1);

    const ToolInfo *table = toolTable();
    for (int i = 0; i < static_cast<int>(ToolId::Count); ++i) {
        const ToolInfo &info = table[i];
        if (info.groupBreak) {
            m_toolEntries.append({createDivider(), true});
        }
        ToolButton *button = createButton(info);
        m_toolEntries.append({button, false});
        m_buttons.insert(info.id, button);
    }

    createFooter();
    applyColumnCount();
    setActiveTool(ToolId::Brush);
}

QString ToolStrip::tooltipFor(const ToolInfo &info, const QKeySequence &shortcut) const
{
    QString tip = QString::fromUtf8(info.name);
    if (!shortcut.isEmpty()) {
        // CS6 shows the tool letter in parentheses after the name.
        tip += QStringLiteral("  (%1)").arg(shortcut.toString(QKeySequence::NativeText));
    }
    return tip;
}
ToolButton *ToolStrip::createButton(const ToolInfo &info)
{
    auto *button = new ToolButton(info.id, this);

    // Bind to the registry command so the shortcut comes from the keymap.
    QAction *action = m_registry
        ? m_registry->registerCommand(QLatin1String(info.commandId),
                                      QString::fromUtf8(info.name))
        : nullptr;

    button->setToolTip(tooltipFor(info, action ? action->shortcut() : QKeySequence()));
    button->setStatusTip(QString::fromUtf8(info.name));

    // A lambda rather than a direct connect: setActiveTool takes a variant
    // argument the signal does not carry, and Qt rejects a slot with more
    // parameters than its signal even when the extra one has a default.
    connect(button, &ToolButton::activated, this,
            [this](ToolId id) { setActiveTool(id); });
    connect(button, &ToolButton::flyoutRequested, this, &ToolStrip::showFlyout);

    if (action) {
        action->setCheckable(true);
        m_group->addAction(action);
        const ToolId id = info.id;
        connect(action, &QAction::triggered, this, [this, id]() { setActiveTool(id); });

        // The registry may rebind at runtime; refresh the tooltip when it does.
        connect(m_registry, &CommandRegistry::shortcutChanged, button,
                [this, button, info](const QString &changedId, const QKeySequence &seq) {
                    if (changedId == QLatin1String(info.commandId)) {
                        button->setToolTip(tooltipFor(info, seq));
                    }
                });
    }

    return button;
}

QWidget *ToolStrip::createDivider()
{
    auto *divider = new QFrame(this);
    divider->setObjectName(QStringLiteral("toolStripDivider"));
    divider->setFrameShape(QFrame::NoFrame);
    divider->setFixedHeight(5);
    return divider;
}

void ToolStrip::flow(QGridLayout *grid, const QList<Entry> &entries)
{
    // Take everything out first. The items are layout wrappers, not the
    // widgets, so the buttons themselves survive the reflow.
    while (QLayoutItem *item = grid->takeAt(0)) {
        delete item;
    }

    // The panel is wider than its button column — the colour swatch sets the
    // width. An elastic column either side keeps the buttons centred in it
    // while full-width rows (dividers, the swatch) still reach both edges.
    const int span = m_columns + 2;
    for (int c = 0; c < span + 2; ++c) {
        grid->setColumnStretch(c, (c == 0 || c == span - 1) ? 1 : 0);
    }

    int row = 0;
    int column = 0;
    for (const Entry &entry : entries) {
        if (entry.divider) {
            if (column != 0) {
                ++row;
                column = 0;
            }
            grid->addWidget(entry.widget, row++, 0, 1, span);
            continue;
        }
        grid->addWidget(entry.widget, row, column + 1, Qt::AlignCenter);
        if (++column >= m_columns) {
            column = 0;
            ++row;
        }
    }
}

void ToolStrip::setColumnCount(int columns)
{
    const int clamped = qBound(1, columns, 2);
    if (clamped == m_columns) {
        return;
    }
    m_columns = clamped;
    applyColumnCount();
    emit columnCountChanged(m_columns);
}

void ToolStrip::applyColumnCount()
{
    flow(m_toolGrid, m_toolEntries);
    flow(m_footerGrid, m_footerEntries);
    refitWidth();
}

void ToolStrip::refitWidth()
{
    // The button column is the floor, not the answer: the colour swatch is
    // wider than one button and would be clipped to a sliver if the panel were
    // sized to the buttons alone. CS6's panel is likewise wider than its 26px
    // buttons.
    const int swatchWidth = m_swatches ? m_swatches->sizeHint().width() : 0;
    const int content = qMax(kButtonSize * m_columns, swatchWidth);
    setFixedWidth(content + 2 * kPanelMargin);
    updateGeometry();
}

void ToolStrip::createFooter()
{
    m_footerEntries.append({createDivider(), true});

    // Foreground/background swatch, centred as in CS6. It spans the strip in
    // either layout, so it goes in as a divider-style full-width row.
    auto *swatchHost = new QWidget(this);
    auto *swatchLayout = new QVBoxLayout(swatchHost);
    swatchLayout->setContentsMargins(0, 4, 0, 4);
    swatchLayout->setSpacing(0);
    m_swatches = new ColorSwatchWidget(swatchHost);
    swatchLayout->addWidget(m_swatches, 0, Qt::AlignHCenter);
    m_footerEntries.append({swatchHost, true});

    m_quickMask = new QToolButton(this);
    m_quickMask->setCheckable(true);
    m_quickMask->setAutoRaise(true);
    m_quickMask->setFixedSize(kButtonSize, kButtonSize);
    m_quickMask->setIconSize(QSize(20, 20));
    m_quickMask->setIcon(ToolIcons::fromSvgBody(ToolIcons::quickMaskSvg(false), kIconColor));
    if (QAction *bound = m_registry ? m_registry->action(QStringLiteral("tool.quickMask"))
                                    : nullptr) {
        m_quickMask->setToolTip(
            tr("Edit in Quick Mask Mode  (%1)")
                .arg(bound->shortcut().toString(QKeySequence::NativeText)));
        connect(bound, &QAction::triggered, m_quickMask, &QToolButton::toggle);
    } else {
        m_quickMask->setToolTip(tr("Edit in Quick Mask Mode"));
    }
    connect(m_quickMask, &QToolButton::toggled, this, &ToolStrip::onQuickMaskToggled);
    m_footerEntries.append({m_quickMask, false});

    m_screenMode = new QToolButton(this);
    m_screenMode->setAutoRaise(true);
    m_screenMode->setFixedSize(kButtonSize, kButtonSize);
    m_screenMode->setIconSize(QSize(20, 20));
    m_screenMode->setIcon(ToolIcons::fromSvgBody(ToolIcons::screenModeSvg(), kIconColor));
    m_screenMode->setToolTip(tr("Change Screen Mode  (F)"));
    // Screen modes are not implemented; the button holds its place in the
    // strip rather than pretending to switch anything.
    m_screenMode->setEnabled(false);
    m_footerEntries.append({m_screenMode, false});
}

bool ToolStrip::quickMaskEnabled() const
{
    return m_quickMask && m_quickMask->isChecked();
}

void ToolStrip::onQuickMaskToggled(bool on)
{
    m_quickMask->setIcon(ToolIcons::fromSvgBody(ToolIcons::quickMaskSvg(on), kIconColor));

    // One button, two names: it says what pressing it would do next, which is
    // how CS6 labels the pair.
    const QString shortcut = m_registry
        ? m_registry->shortcut(QStringLiteral("tool.quickMask")).toString(QKeySequence::NativeText)
        : QString();
    const QString name = on ? tr("Edit in Standard Mode") : tr("Edit in Quick Mask Mode");
    m_quickMask->setToolTip(shortcut.isEmpty() ? name
                                               : tr("%1  (%2)").arg(name, shortcut));
    emit quickMaskToggled(on);
}

void ToolStrip::showFlyout(ToolId id)
{
    ToolButton *button = m_buttons.value(id, nullptr);
    if (!button) {
        return;
    }
    const QList<SubTool> subs = subTools(id);
    if (subs.size() < 2) {
        return;
    }
    const int active = toolVariant(id);

    QMenu menu(this);
    menu.setObjectName(QStringLiteral("toolFlyout"));

    for (int i = 0; i < subs.size(); ++i) {
        const SubTool &sub = subs.at(i);
        auto *action = menu.addAction(QString::fromUtf8(sub.name));
        action->setIcon(ToolIcons::icon(id, i, kIconColor));

        if (sub.shortcut) {
            // CS6 shows the tool letter right-aligned on the row. Setting it
            // as the action's shortcut is what gets Qt to lay it out that way;
            // the menu is transient, so this never competes with the real
            // window-level binding.
            action->setShortcut(QKeySequence(QLatin1String(sub.shortcut)));
        }

        if (!sub.implemented) {
            // Listed so the flyout keeps CS6's shape, but disabled — the
            // engine has nothing behind these yet.
            action->setEnabled(false);
            action->setToolTip(tr("Not implemented yet"));
            continue;
        }

        // A dot marks the variant currently in use.
        action->setCheckable(true);
        action->setChecked(m_activeTool == id && i == active);
        connect(action, &QAction::triggered, this, [this, id, i] { setActiveTool(id, i); });
    }

    // Photoshop pops the flyout clear of the panel's right edge, top-aligned
    // with the button — so in two-column mode the left column's flyout does
    // not land on top of the right column.
    const QPoint stripRight = mapToGlobal(QPoint(width(), 0));
    const QPoint buttonTop = button->mapToGlobal(QPoint(0, 0));
    menu.exec(QPoint(stripRight.x(), buttonTop.y()));
}

int ToolStrip::toolVariant(ToolId tool) const
{
    return m_variants.value(tool, 0);
}

void ToolStrip::cycleVariant(ToolId tool)
{
    setActiveTool(tool, nextVariantInCycle(tool, toolVariant(tool)));
}

void ToolStrip::setActiveTool(ToolId tool, int variant)
{
    ToolButton *button = m_buttons.value(tool, nullptr);
    if (!button) {
        return;
    }

    const int previousVariant = toolVariant(tool);
    // -1 means "whatever this tool was last using".
    const int wanted = variant < 0 ? previousVariant : variant;
    const int clamped = qBound(0, wanted, qMax(0, int(subTools(tool).size()) - 1));

    if (clamped != previousVariant) {
        m_variants.insert(tool, clamped);
        // CS6 shows the active variant's icon on the strip button.
        button->setIcon(ToolIcons::icon(tool, clamped, kIconColor));
        button->setToolTip(tr("%1  (%2)")
                               .arg(toolVariantName(tool, clamped),
                                    m_registry
                                        ? m_registry->shortcut(QLatin1String(
                                                                   toolInfo(tool)->commandId))
                                              .toString(QKeySequence::NativeText)
                                        : QString()));
    }

    // Uncheck every other button — QToolButtons are not grouped by the
    // QActionGroup, only their actions are.
    for (auto it = m_buttons.constBegin(); it != m_buttons.constEnd(); ++it) {
        it.value()->setChecked(it.key() == tool);
    }

    // Still emit when only the variant moved, so the canvas and options bar
    // follow a Rectangular → Elliptical switch.
    if (m_activeTool == tool && clamped == previousVariant) {
        return;
    }
    m_activeTool = tool;

    if (m_registry) {
        if (QAction *action = m_registry->action(QLatin1String(toolInfo(tool)->commandId))) {
            action->setChecked(true);
        }
    }

    emit toolChanged(tool, clamped);
}
