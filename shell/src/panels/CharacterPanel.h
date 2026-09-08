#pragma once

#include <QColor>
#include <QFont>
#include <QWidget>

class QComboBox;
class QSpinBox;
class QDoubleSpinBox;
class QToolButton;

/// The Character panel: family, style, size, colour, anti-aliasing and the
/// two Scale fields, all of which the text record carries per run. CS6's
/// leading, tracking and baseline shift are shown for shape but disabled —
/// nothing downstream of the Type tool tracks those yet.
///
/// Alignment lives on the Paragraph panel in CS6, not here, so it is not
/// duplicated on this one even though `MainWindow` already tracks it for the
/// Type options bar.
class CharacterPanel : public QWidget
{
    Q_OBJECT

public:
    explicit CharacterPanel(QWidget *parent = nullptr);

    /// Push the type tool's current values in, without echoing them back out
    /// through the change signals below — the same shape as
    /// `ColorPanel::setForegroundColor`, for the same reason: this comes from
    /// the options bar or a reopened text layer, not from the user touching
    /// this panel.
    void setValues(const QFont &font, const QString &styleName, const QColor &color,
                   const QString &antialiasMethod, qreal hScale = 1.0, qreal vScale = 1.0);

signals:
    void familyChanged(const QString &family);
    void styleChanged(const QString &style);
    void sizeChanged(qreal pointSize);
    void colorChanged(const QColor &color);
    /// One of `TypeDefaults::antialiasMethods()`.
    void antialiasChanged(const QString &method);
    /// Photoshop's Horizontal/Vertical Scale, as a fraction — 1.0 is 100%.
    void horizontalScaleChanged(qreal scale);
    void verticalScaleChanged(qreal scale);

private:
    void refreshStyles(const QString &familyName, const QString &wanted);
    void refreshSwatch();

    QComboBox *m_family = nullptr;
    QComboBox *m_style = nullptr;
    QComboBox *m_size = nullptr;
    QComboBox *m_antialias = nullptr;
    QSpinBox *m_hScale = nullptr;
    QSpinBox *m_vScale = nullptr;
    QToolButton *m_colorSwatch = nullptr;

    QColor m_color{Qt::black};
    bool m_updating = false;
};
