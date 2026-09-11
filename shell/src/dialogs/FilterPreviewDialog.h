#pragma once

#include <QColor>
#include <QDialog>
#include <QImage>
#include <QList>
#include <QPointF>
#include <QRectF>
#include <QWidget>

#include <functional>

class Engine;
class FilterPreviewDialog;
class QCheckBox;
class QDialogButtonBox;
class QComboBox;
class QDoubleSpinBox;
class QGridLayout;
class QHBoxLayout;
class QLabel;
class QSlider;
class QTabWidget;
class QTimer;
class QVBoxLayout;

/// The thumbnail inside a filter dialog.
///
/// It shows one region of the document with the filter already applied, at its
/// own zoom rather than the canvas's — that is the whole point of it, since a
/// blur is hard to judge on a canvas fitted to the window. Dragging inside it
/// pans the region, exactly as Photoshop's does.
///
/// Deliberately not a `QObject` with signals: it has one listener, the dialog
/// that owns it, and calling straight back into that is clearer than wiring a
/// connection between two halves of the same widget.
class FilterPreviewPane : public QWidget
{
public:
    explicit FilterPreviewPane(FilterPreviewDialog *dialog);

    /// Show `image` — a crop of the document at 1:1 — magnified by `zoom`.
    void setContent(const QImage &image, double zoom);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;

private:
    FilterPreviewDialog *m_dialog = nullptr;
    QImage m_image;
    double m_zoom = 1.0;
    bool m_dragging = false;
    QPointF m_dragFrom;
};

/// CS6's Blur Center box: the square in the Radial Blur dialog that says where
/// the blur turns about, and lets you drag it somewhere else.
///
/// It draws the shape of the blur rather than the image — concentric rings for
/// a spin, rays for a zoom — which is what makes it readable at 100 pixels
/// square, and is what CS6 shows.
///
/// The centre is kept in normalized 0..1 coordinates, so it means the same
/// thing whatever size the image is.
class BlurCenterWidget : public QWidget
{
public:
    /// `spinning` is asked, each repaint, which of the two patterns to draw —
    /// the method lives in another control, and this follows it.
    BlurCenterWidget(FilterPreviewDialog *dialog, std::function<bool()> spinning);

    QPointF center() const { return m_center; }

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;

private:
    void moveCenterTo(const QPointF &pos);

    FilterPreviewDialog *m_dialog = nullptr;
    std::function<bool()> m_spinning;
    QPointF m_center{0.5, 0.5};
};

/// The wireframe CS6 draws beside Pinch's preview: a regular grid deformed the
/// way the filter will deform the picture.
///
/// It draws what the *engine* says the distortion does rather than working it
/// out again here, so the diagram and the filter cannot disagree.
class DistortGridWidget : public QWidget
{
public:
    explicit DistortGridWidget(QWidget *parent);

    /// `points` is `(cells + 1)²` x/y pairs in 0..1, row-major, as
    /// `Engine::distortGrid` returns them.
    void setGrid(const QList<QPointF> &points, int cells);

protected:
    void paintEvent(QPaintEvent *event) override;

private:
    QList<QPointF> m_points;
    int m_cells = 0;
};

/// CS6's Shear curve: a box in which a line runs from the top of the image to
/// the bottom, and dragging it sideways pushes those rows sideways.
///
/// Click on the line to add a point, drag one to move it, and drag one out of
/// the box to take it away — as CS6's does. The two ends cannot be removed,
/// since a curve with fewer than two points is not a curve.
class ShearCurveWidget : public QWidget
{
public:
    explicit ShearCurveWidget(FilterPreviewDialog *dialog);

    /// The curve sampled at `count` evenly spaced rows, each an offset from
    /// -1 to 1 where 1 is half the image's width.
    QList<double> sampled(int count) const;

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;

private:
    /// Widget position of a control point.
    QPointF at(int index) const;
    /// The control points, `x` the offset in -1..1 and `y` the height down the
    /// image in 0..1, kept sorted by `y`.
    QList<QPointF> m_points{{0.0, 0.0}, {0.0, 1.0}};
    FilterPreviewDialog *m_dialog = nullptr;
    int m_dragging = -1;
};

/// The dialog shared by every filter that takes a number: CS6's Box Blur,
/// Gaussian Blur, Motion Blur, Radial Blur, Surface Blur, Unsharp Mask and Add
/// Noise all wear the same face — a preview with its own zoom, an OK/Cancel
/// column with a Preview checkbox under it, and one or two parameter rows,
/// each a spin box with a slider beneath.
///
/// A caller builds it by naming its parameters:
///
/// ```
/// FilterPreviewDialog dlg(engine, "Box Blur", this);
/// dlg.addParameter(tr("Radius:"), 1, 500, 10, 0, tr(" Pixels"));
/// ```
///
/// and reads `p1()` and `p2()` back out, which are the two floats
/// `Engine::applyFilter` takes, in the order the parameters were added.
///
/// The dialog previews but never commits: pressing OK leaves the layer exactly
/// as it found it, and the caller applies the filter through `applyFilter` as
/// it always did. That keeps one path to the History panel.
class FilterPreviewDialog : public QDialog
{
    Q_OBJECT

public:
    FilterPreviewDialog(Engine *engine, const QString &filterName, QWidget *parent = nullptr);
    ~FilterPreviewDialog() override;

    /// Add a numeric parameter. Returns the slot it filled, counting from 0
    /// in the order parameters were added.
    /// `withSlider` adds CS6's slider under the field. Off for the dialogs
    /// that have none — Color Halftone is a set of plain numbers, and a
    /// slider under each would be four times the height for no gain.
    int addParameter(const QString &label, double min, double max, double value,
                     int decimals = 0, const QString &suffix = QString(),
                     bool withSlider = true);

    /// Add an angle: a field in degrees with CS6's angle wheel beside it, and
    /// no slider — an angle wraps, so a bar with two ends is the wrong shape
    /// for one, and CS6 gives it a wheel for exactly that reason.
    int addAngleParameter(const QString &label, double value);

    /// Add a parameter chosen from a list rather than dialled — Radial Blur's
    /// Spin and Zoom. `values` says what each item is worth as a filter
    /// parameter, since the engine takes floats.
    ///
    /// `separatorsAfter` names items to rule a line under, for the lists CS6
    /// groups — Mezzotint's dots, lines and strokes. The value is read from
    /// the chosen item rather than from its position, so a separator cannot
    /// shift what the list means.
    int addChoice(const QString &label, const QStringList &items, const QList<double> &values,
                  int index, const QList<int> &separatorsAfter = {});

    /// A choice with an angle beside it, greyed out unless the chosen item is
    /// the one that uses it — Smart Sharpen's "Remove:", where only Motion
    /// Blur has a direction. Fills **two** slots, the choice then the angle,
    /// and returns the first.
    int addChoiceWithAngle(const QString &label, const QStringList &items,
                           const QList<double> &values, int index, double angle,
                           int angleForIndex);

    /// A pair of numbers on one row under shared "Min."/"Max." headings —
    /// CS6's Wavelength, Amplitude and Scale rows on Wave. Fills **two**
    /// slots, low then high, and returns the first.
    int addRangeParameter(const QString &label, const QString &lowLabel,
                          const QString &highLabel, double min, double max, double low,
                          double high, int decimals = 0, const QString &suffix = QString());

    /// CS6's Randomize button on Wave, which re-rolls the seed its generators
    /// are drawn from. Fills one slot, holding that seed.
    int addRandomizeButton(const QString &label);

    /// CS6's Shear curve. Fills `points` slots, one per sampled row.
    int addShearCurve(int points);

    /// Show CS6's distortion wireframe beside the parameters, kept up to date
    /// from the engine as they change. Fills no slot.
    void addDistortGrid();

    /// A tick box — Add Noise's "Monochromatic". Fills one slot, worth 1 when
    /// ticked and 0 when not.
    int addCheckBox(const QString &label, bool checked);

    /// Put everything added from here on into a tab of this name, as CS6's
    /// longer filter dialogs are divided. The first call turns the parameter
    /// area into a tab strip; controls added before it stay above the strip.
    void beginTab(const QString &title);

    /// A colour swatch that opens the colour picker — Flame's "Custom Color
    /// for Flames". Fills **three** slots, red, green and blue as 0..255, and
    /// returns the first. `enabledWhen` is asked whether the swatch should be
    /// live, for the tick boxes CS6 greys these out behind.
    int addColorButton(const QString &label, const QColor &initial,
                       std::function<bool()> enabledWhen = {});

    /// A plain line of text spanning both columns, for the headings CS6 rules
    /// its longer dialogs into groups with. Fills no slot.
    void addHeading(const QString &text);

    /// A greyed-out line naming a part of CS6's dialog that is not built, so
    /// that its absence reads as a decision rather than an oversight. Fills no
    /// slot.
    void addDisabledNote(const QString &text);

    /// The same choice as a titled box of radio buttons, which is how CS6
    /// shows Radial Blur's Blur Method and Quality. Two or three options that
    /// all need to be visible at once are a box of radios there, not a
    /// dropdown.
    int addRadioChoice(const QString &title, const QStringList &items,
                       const QList<double> &values, int index);

    /// Add CS6's Blur Center box. Fills **two** slots, x then y, in normalized
    /// 0..1 coordinates, and returns the first of them.
    int addCenterPicker(const QString &title, std::function<bool()> spinning);

    /// Drive the canvas preview through something other than `applyFilter`.
    ///
    /// Flame is not a `Filter` — it draws along the document's path rather
    /// than working on the layer alone — so it has its own route into the
    /// engine. Setting a driver also keeps the Preview tick box on a dialog
    /// whose thumbnail has been hidden, since there is still something for it
    /// to switch on.
    using PreviewDriver = std::function<void(const QList<float> &params, bool on)>;
    void setCanvasPreviewDriver(PreviewDriver driver);

    /// Whether to show the preview thumbnail and its zoom row at all.
    ///
    /// CS6's Radial Blur has neither — it offers the Blur Center box in their
    /// place — so that dialog turns them off.
    void setPreviewPaneVisible(bool visible);

    /// The parameters, in the order they were added — what
    /// `Engine::applyFilter` takes.
    QList<float> parameters() const;

    /// One of them, for a caller that needs to look at a control's value while
    /// building the dialog: the Blur Center box asks the method's slot which
    /// pattern to draw.
    float parameterValue(int slot) const { return slotValue(slot); }

    /// Centre the preview on a document point — the middle of what the canvas
    /// is showing, so the dialog opens looking at what the user was looking
    /// at.
    void setPreviewCenter(const QPointF &documentPos);

    /// Move the preview by a distance in document pixels. Called by the pane
    /// when it is dragged.
    void panPreview(const QPointF &documentDelta);

    /// The region the thumbnail is showing, in document coordinates.
    QRectF previewRegion() const;

    /// Step along the zoom ladder — what the two magnifier buttons do, and
    /// what the tests drive to check the region narrows as it magnifies.
    /// Clamped to the ends of the ladder; 5 is 100%.
    void setZoomStep(int step);

    /// A control changed: the thumbnail and the canvas preview both need
    /// redoing. Public because the Blur Center box is dragged rather than
    /// edited, so it reports its own changes.
    void parametersChanged();

signals:
    /// The thumbnail has moved or zoomed. The canvas draws this as a square,
    /// so the user can see which part of the image they are inspecting. An
    /// empty rect means the dialog has gone.
    void previewRegionChanged(const QRectF &region);

protected:
    void showEvent(QShowEvent *event) override;

private:
    /// Recompute the thumbnail, and schedule the canvas preview if it is on.
    void refreshPreview();
    /// Push the filter onto the layer, or take it off again.
    void refreshCanvasPreview();
    /// Ask the engine for the wireframe again, if this dialog shows one.
    void refreshDistortGrid();
    float slotValue(int slot) const;

    Engine *m_engine = nullptr;
    QString m_filterName;

    FilterPreviewPane *m_pane = nullptr;
    /// The row of controls under the thumbnail, hidden with it.
    QWidget *m_zoomRow = nullptr;
    QLabel *m_zoomLabel = nullptr;
    /// OK and Cancel. They sit beside the thumbnail when there is one and drop
    /// to the foot of the dialog when there is not — a column of two buttons
    /// alone at the top of a dialog with nothing beside it reads as a mistake.
    QDialogButtonBox *m_buttons = nullptr;
    QVBoxLayout *m_sideColumn = nullptr;
    QCheckBox *m_preview = nullptr;
    QGridLayout *m_params = nullptr;
    /// Where the titled boxes go: the radio boxes stack in `m_boxColumn` on
    /// the left, the Blur Center box sits beside it. CS6's Radial Blur
    /// arrangement, and the only dialog that uses either.
    QHBoxLayout *m_boxRow = nullptr;
    QVBoxLayout *m_boxColumn = nullptr;
    /// The tab strip, once `beginTab` has been called. Null until then, and
    /// the dialogs that never call it are laid out exactly as before.
    QTabWidget *m_tabs = nullptr;
    /// Controls whose enabled state follows a tick box elsewhere in the
    /// dialog, re-asked whenever anything changes.
    QList<QPair<QWidget *, std::function<bool()>>> m_conditional;
    /// The Blur Center boxes, so they can be redrawn when the method they
    /// follow changes. Kept by hand because they are not `QObject`s with the
    /// meta-object `findChildren` needs.
    QList<BlurCenterWidget *> m_pickers;
    /// The distortion wireframe, if this dialog has one.
    DistortGridWidget *m_distortGrid = nullptr;

    /// One entry per parameter, in the order they were added — each just a
    /// way of reading the control that owns it. A closure rather than a widget
    /// pointer because the controls are not all of one kind, and because the
    /// Blur Center box is a single widget filling two slots.
    QList<std::function<double()>> m_slots;

    /// Whether the thumbnail is shown at all. See `setPreviewPaneVisible`.
    bool m_paneVisible = true;
    /// Index into the zoom ladder in the .cpp.
    int m_zoomStep = 0;
    /// Where the thumbnail is looking, in document coordinates.
    QPointF m_center;

    /// Redoing the thumbnail is cheap; redoing the whole-layer canvas preview
    /// is not, so a drag of the slider coalesces into one of each.
    QTimer *m_thumbTimer = nullptr;
    QTimer *m_canvasTimer = nullptr;

    /// Set for the filters that do not go through `applyFilter`. See
    /// `setCanvasPreviewDriver`.
    PreviewDriver m_canvasDriver;
};
