#pragma once

#include <QProxyStyle>

/// Replaces the icons Qt draws on a `QDockWidget`'s own title bar.
///
/// Fusion puts unlabelled rounded squares on the float and close buttons,
/// which say nothing about what pressing them does. CS6 puts a double chevron
/// and a cross there, so this hands those back instead.
///
/// It goes through the style rather than `setIcon()` on the button because
/// `QDockWidget` re-asks the style for these icons every time the dock's
/// state or features change — including on the very float/dock toggle this
/// button performs — which would quietly undo a direct assignment.
class DockTitleStyle : public QProxyStyle
{
    Q_OBJECT

public:
    using QProxyStyle::QProxyStyle;

    QIcon standardIcon(StandardPixmap pixmap, const QStyleOption *option = nullptr,
                       const QWidget *widget = nullptr) const override;
};
