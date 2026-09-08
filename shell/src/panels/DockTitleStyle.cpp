#include "DockTitleStyle.h"

#include "../tools/ToolIcons.h"

#include <QDockWidget>

namespace {

/// The tint the rest of the panel chrome uses — see `PanelHeader`.
const QColor kTitleIconColor(0xd4, 0xd4, 0xd4);

} // namespace

QIcon DockTitleStyle::standardIcon(StandardPixmap pixmap, const QStyleOption *option,
                                   const QWidget *widget) const
{
    // Only a dock widget's title bar, not every restore or close button in
    // the application: `QDockWidget` passes itself as the widget when it asks.
    if (qobject_cast<const QDockWidget *>(widget)) {
        // Chevrons pointing right, back towards the column the panel came
        // from — the same glyph `PanelHeader` uses on the Tools panel.
        if (pixmap == SP_TitleBarNormalButton) {
            return ToolIcons::fromSvgBody(ToolIcons::columnToggleSvg(false), kTitleIconColor);
        }
        // Both roles, because which one a dock's close button asks for is
        // Qt's business and has moved between the two; the tests pin down
        // that whichever it is, it lands on the button.
        if (pixmap == SP_DockWidgetCloseButton || pixmap == SP_TitleBarCloseButton) {
            return ToolIcons::fromSvgBody(ToolIcons::closeSvg(), kTitleIconColor);
        }
    }
    return QProxyStyle::standardIcon(pixmap, option, widget);
}
