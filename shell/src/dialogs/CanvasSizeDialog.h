#pragma once

#include <QColor>
#include <QDialog>

class Engine;
class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QLabel;
class QPushButton;

/// The nine-square anchor grid from Canvas Size.
///
/// A single painted widget rather than nine buttons: the arrows point *away*
/// from the chosen square, so every cell's artwork depends on which one is
/// selected, and drawing the grid as a whole is what keeps that consistent.
class AnchorSelector : public QWidget
{
    Q_OBJECT
public:
    explicit AnchorSelector(QWidget *parent = nullptr);

    /// 0, 1 or 2 — left/centre/right and top/centre/bottom.
    int anchorX() const { return m_x; }
    int anchorY() const { return m_y; }

    QSize sizeHint() const override;

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;

private:
    QRect cellRect(int cx, int cy) const;

    int m_x = 1;
    int m_y = 1;
};

/// Photoshop's Image ▸ Canvas Size.
///
/// Changes how much room the image sits in without scaling a single pixel:
/// the canvas grows or is cropped around an anchor, and any new area on the
/// Background layer is filled with the extension colour.
class CanvasSizeDialog : public QDialog
{
    Q_OBJECT
public:
    explicit CanvasSizeDialog(Engine *engine, QWidget *parent = nullptr);

    int resultWidth() const;
    int resultHeight() const;
    int anchorX() const;
    int anchorY() const;
    /// The colour poured into new area, or an invalid colour to leave it
    /// transparent.
    QColor extensionColor() const;

private:
    void buildUi();
    void updateNewSize();
    void onRelativeToggled(bool on);
    void onExtensionChanged(int index);
    /// Open the colour picker on the swatch beside the menu, as CS6 does.
    void pickExtensionColor();
    void updateSwatch();

    /// Pixels per unit for the Width/Height menus.
    double unitScale(int unitIndex) const;
    /// The value in `field` converted to pixels, given its unit.
    int toPixels(const QDoubleSpinBox *field, int unitIndex, int base) const;
    /// Restate `field` in a new unit, keeping the size it describes.
    ///
    /// `prevUnit` is what the number in the field is currently expressed in;
    /// without it the value would be reinterpreted rather than converted, and
    /// 1000 pixels would become 1000 inches.
    void changeUnit(QDoubleSpinBox *field, int newUnit, int &prevUnit, int base);

    Engine *m_engine = nullptr;
    int m_pixelWidth = 0;
    int m_pixelHeight = 0;
    double m_resolution = 72.0;

    QLabel *m_currentSize = nullptr;
    QLabel *m_currentWidth = nullptr;
    QLabel *m_currentHeight = nullptr;
    QLabel *m_newSize = nullptr;
    QDoubleSpinBox *m_width = nullptr;
    QDoubleSpinBox *m_height = nullptr;
    QComboBox *m_widthUnit = nullptr;
    QComboBox *m_heightUnit = nullptr;
    /// The unit each field's number is currently written in.
    int m_widthUnitPrev = 0;
    int m_heightUnitPrev = 0;
    QCheckBox *m_relative = nullptr;
    AnchorSelector *m_anchor = nullptr;
    QComboBox *m_extension = nullptr;
    QPushButton *m_extensionSwatch = nullptr;
    QColor m_customColor = Qt::white;
};
