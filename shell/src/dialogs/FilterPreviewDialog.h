#pragma once

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
class QComboBox;
class QDoubleSpinBox;
class QGridLayout;
class QHBoxLayout;
class QLabel;
class QSlider;
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
    int addParameter(const QString &label, double min, double max, double value,
                     int decimals = 0, const QString &suffix = QString());

    /// Add a parameter chosen from a list rather than dialled — Radial Blur's
    /// Spin and Zoom. `values` says what each item is worth as a filter
    /// parameter, since the engine takes floats.
    int addChoice(const QString &label, const QStringList &items, const QList<double> &values,
                  int index);

    /// The same choice as a titled box of radio buttons, which is how CS6
    /// shows Radial Blur's Blur Method and Quality. Two or three options that
    /// all need to be visible at once are a box of radios there, not a
    /// dropdown.
    int addRadioChoice(const QString &title, const QStringList &items,
                       const QList<double> &values, int index);

    /// Add CS6's Blur Center box. Fills **two** slots, x then y, in normalized
    /// 0..1 coordinates, and returns the first of them.
    int addCenterPicker(const QString &title, std::function<bool()> spinning);

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
    float slotValue(int slot) const;

    Engine *m_engine = nullptr;
    QString m_filterName;

    FilterPreviewPane *m_pane = nullptr;
    /// The row of controls under the thumbnail, hidden with it.
    QWidget *m_zoomRow = nullptr;
    QLabel *m_zoomLabel = nullptr;
    QCheckBox *m_preview = nullptr;
    QGridLayout *m_params = nullptr;
    /// Where the titled boxes go: the radio boxes stack in `m_boxColumn` on
    /// the left, the Blur Center box sits beside it. CS6's Radial Blur
    /// arrangement, and the only dialog that uses either.
    QHBoxLayout *m_boxRow = nullptr;
    QVBoxLayout *m_boxColumn = nullptr;
    /// The Blur Center boxes, so they can be redrawn when the method they
    /// follow changes. Kept by hand because they are not `QObject`s with the
    /// meta-object `findChildren` needs.
    QList<BlurCenterWidget *> m_pickers;

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
};
