#pragma once

#include <functional>

#include <QColor>
#include <QDialog>
#include <QImage>
#include <QWidget>

class QCheckBox;
class QLabel;
class QLineEdit;
class QRadioButton;
class QSpinBox;
class QTimer;

/// The component the vertical ramp controls.
///
/// In Photoshop this is chosen by the radio buttons beside the numeric fields,
/// and it re-maps *both* the ramp and the two axes of the square field. The
/// mapping is fixed per axis — see `axisMapping()` in the .cpp.
enum class ColorAxis {
    Hue,
    Saturation,
    Brightness,
    Red,
    Green,
    Blue,
};

/// The square colour field.
///
/// Renders the plane of the two components the current [`ColorAxis`] does not
/// control, with a ring marker at the current colour. The plane image is
/// cached and only re-rendered when the axis or the ramp value changes, since
/// it is 256×256 pixels of per-pixel work.
class ColorPlane : public QWidget
{
    Q_OBJECT

public:
    explicit ColorPlane(QWidget *parent = nullptr);

    void setAxis(ColorAxis axis);
    /// Update the displayed colour. Does not emit `picked`.
    void setHsv(int hue, int sat, int val);
    void setWebColorsOnly(bool webOnly);

    QSize sizeHint() const override;

signals:
    /// The user clicked or dragged to a new colour.
    void picked(int hue, int sat, int val);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;

private:
    /// Recompute the cached plane image for the current axis and ramp value.
    void rebuildCache();
    /// Translate a widget position into a colour and emit it.
    void pickAt(const QPoint &pos);

    ColorAxis m_axis = ColorAxis::Hue;
    int m_hue = 0;
    int m_sat = 255;
    int m_val = 255;
    bool m_webOnly = false;

    QImage m_cache;
    /// Guards the cache: it is only valid for this axis/value pair.
    ColorAxis m_cacheAxis = ColorAxis::Hue;
    int m_cacheValue = -1;
    bool m_cacheWebOnly = false;
};

/// The vertical ramp beside the field, with Photoshop's inward-pointing
/// arrow markers on either side.
class ColorRamp : public QWidget
{
    Q_OBJECT

public:
    explicit ColorRamp(QWidget *parent = nullptr);

    void setAxis(ColorAxis axis);
    void setHsv(int hue, int sat, int val);
    void setWebColorsOnly(bool webOnly);

    QSize sizeHint() const override;

signals:
    void picked(int hue, int sat, int val);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;

private:
    void rebuildCache();
    void pickAt(const QPoint &pos);
    /// The strip's drawing rectangle, excluding the arrow gutters.
    QRect stripRect() const;

    ColorAxis m_axis = ColorAxis::Hue;
    int m_hue = 0;
    int m_sat = 255;
    int m_val = 255;
    bool m_webOnly = false;

    QImage m_cache;
    ColorAxis m_cacheAxis = ColorAxis::Hue;
    int m_cacheHue = -1;
    int m_cacheSat = -1;
    int m_cacheVal = -1;
    bool m_cacheWebOnly = false;
};

/// The new/current colour comparison swatch.
///
/// Clicking the lower half reverts to the colour the dialog opened with, as
/// Photoshop does.
class ColorCompare : public QWidget
{
    Q_OBJECT

public:
    explicit ColorCompare(QWidget *parent = nullptr);

    void setCurrentColor(const QColor &color);
    void setOriginalColor(const QColor &color);

    QSize sizeHint() const override;

signals:
    /// The user clicked the "current" half to revert.
    void originalClicked();

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;

private:
    QColor m_current{Qt::black};
    QColor m_original{Qt::black};
};

/// Photoshop's Color Picker.
///
/// Replaces `QColorDialog`, which shares none of its layout or behaviour.
/// Use the static helper:
///
/// \code
///   const QColor picked =
///       ColorPickerDialog::getColor(startColor, this, tr("Foreground Color"));
///   if (picked.isValid()) { ... }
/// \endcode
class ColorPickerDialog : public QDialog
{
    Q_OBJECT

public:
    /// \param title Appears in the caption as "Color Picker (<title>)", which
    ///              is how Photoshop names the picker per call site.
    explicit ColorPickerDialog(const QColor &initial,
                               QWidget *parent = nullptr,
                               const QString &title = {});

    /// The colour currently shown.
    QColor selectedColor() const;

    /// Modal convenience wrapper. Returns an invalid QColor if cancelled.
    static QColor getColor(const QColor &initial,
                           QWidget *parent = nullptr,
                           const QString &title = {});

    /// How the eyedropper reads a colour off the image: given a point on
    /// screen, return the colour there, or an invalid colour if the point is
    /// not over the image.
    using Sampler = std::function<QColor(const QPoint &globalPos)>;

    /// Install that function for every picker in the application.
    ///
    /// Deliberately application-wide rather than passed in per call: where the
    /// picker samples from is a property of the app — there is one canvas —
    /// and threading it through would mean teaching the Color panel and the
    /// tool strip's swatches about the canvas, which is not their business.
    /// Without one installed the eyedropper simply does not appear.
    static void setSampler(Sampler sampler);

protected:
    // While the pointer is off the dialog it holds the mouse, so these arrive
    // wherever it is — including over the canvas behind the dialog.
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void showEvent(QShowEvent *event) override;
    void hideEvent(QHideEvent *event) override;

private slots:
    void onAxisChanged();
    void onPlanePicked(int hue, int sat, int val);
    void onHsbFieldsEdited();
    void onRgbFieldsEdited();
    void onLabFieldsEdited();
    void onHexEdited();
    void onWebColorsToggled(bool on);
    void revertToOriginal();

private:
    /// Whether a screen point is over this dialog, decorations included.
    bool overSelf(const QPoint &pos) const;
    void buildUi(const QString &title);
    /// Follow the pointer: take the mouse while it is off the dialog, give it
    /// back when it returns, and sample whatever it is over in between.
    ///
    /// Called from a timer and from every mouse move, so it is the one place
    /// that decides whether sampling is happening.
    void updateHoverSampling();
    /// Read the colour at a screen point and show it. False if there is
    /// nothing to sample there.
    bool sampleAt(const QPoint &globalPos);
    /// Show the eyedropper over the image and an ordinary arrow anywhere else.
    void showCursorFor(bool overImage);
    /// Put the application's cursor back, exactly once.
    void clearCursorOverride();
    /// Push the current colour into every control except `except`, which is
    /// the one the user is typing into.
    void syncControls(QWidget *except = nullptr);
    /// Adopt a colour, preserving hue through achromatic values.
    void setColor(const QColor &color);
    void setHsv(int hue, int sat, int val);
    ColorAxis currentAxis() const;

    // Both representations are kept, and which one is authoritative depends on
    // what the user last touched.
    //
    // HSV must persist because `QColor::hue()` collapses to -1 for greys —
    // dragging brightness to zero would otherwise lose the hue and snap the
    // marker back to red on the way out.
    //
    // The RGB value must persist too, because HSV is a lossy intermediate at
    // 8-bit precision: round-tripping #598FC3 through HSV returns #5990C3.
    // Typing a hex value has to give that exact colour back, so RGB-side edits
    // store the colour directly instead of regenerating it from HSV.
    int m_hue = 0;
    int m_sat = 255;
    int m_val = 255;
    QColor m_color{Qt::black};

    QColor m_original{Qt::black};
    bool m_updating = false;

    /// True while the pointer is off the dialog and the mouse is held, which
    /// is when sampling happens.
    bool m_sampling = false;
    /// A click on the image holds that colour until the next one: the pointer
    /// can travel back to the dialog without the colour following it.
    bool m_latched = false;
    /// Polls where the pointer is. The dialog is modal, so nothing else in the
    /// application sees the pointer to tell it — and without the mouse held
    /// there are no move events to go on either.
    QTimer *m_hoverTimer = nullptr;
    /// Whether this dialog has an override cursor pushed. Kept so the push and
    /// the pop stay in step — an unbalanced restore would strip a cursor some
    /// other part of the application had set.
    bool m_cursorOverridden = false;

    ColorPlane *m_plane = nullptr;
    ColorRamp *m_ramp = nullptr;
    ColorCompare *m_compare = nullptr;

    QRadioButton *m_radioH = nullptr;
    QRadioButton *m_radioS = nullptr;
    QRadioButton *m_radioB = nullptr;
    QRadioButton *m_radioR = nullptr;
    QRadioButton *m_radioG = nullptr;
    QRadioButton *m_radioBlue = nullptr;

    QSpinBox *m_spinH = nullptr;
    QSpinBox *m_spinS = nullptr;
    QSpinBox *m_spinB = nullptr;
    QSpinBox *m_spinR = nullptr;
    QSpinBox *m_spinG = nullptr;
    QSpinBox *m_spinBlue = nullptr;
    QSpinBox *m_spinL = nullptr;
    QSpinBox *m_spinLabA = nullptr;
    QSpinBox *m_spinLabB = nullptr;
    QSpinBox *m_spinC = nullptr;
    QSpinBox *m_spinM = nullptr;
    QSpinBox *m_spinY = nullptr;
    QSpinBox *m_spinK = nullptr;

    QLineEdit *m_hex = nullptr;
    QCheckBox *m_webOnly = nullptr;
};

// ---------------------------------------------------------------------------
// Colour space helpers
// ---------------------------------------------------------------------------

/// sRGB → CIE L*a*b* (D50), the space Photoshop's Lab readout uses.
void rgbToLab(const QColor &color, double *l, double *a, double *b);

/// CIE L*a*b* (D50) → sRGB, clamped into gamut.
QColor labToRgb(double l, double a, double b);

/// Snap each channel to the nearest web-safe value (multiples of 0x33).
QColor snapToWebColor(const QColor &color);

/// Whether every channel is already a multiple of 0x33.
bool isWebColor(const QColor &color);
