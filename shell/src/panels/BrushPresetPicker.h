#pragma once

#include <QWidget>

class Engine;

class QLabel;
class QListWidget;
class QSlider;
class QSpinBox;

/// CS6's Brush Preset Picker: the popup behind the brush tip button in the
/// options bar.
///
/// Holds the two settings that define a tip — Size and Hardness — as a slider
/// and a number each, a preview of the resulting tip, and a grid of presets.
///
/// A preset is the whole tip: size, hardness, roundness, angle, scatter, count
/// and the jitter amounts — which is what the engine's brush is (see
/// core/src/brush.rs). That covers CS6's round, chisel, spatter and grass
/// brushes. What it does not cover is the ones built from a bitmap tip image;
/// those are approximated with scatter and jitter rather than left out.
///
/// Thumbnails are rendered by the **engine**, by asking the brush to lay one
/// step into a small image. A thumbnail therefore cannot drift from what the
/// brush paints, which drawing it separately in the shell would allow.
class BrushPresetPicker : public QWidget
{
    Q_OBJECT

public:
    /// One entry in the preset grid: everything that defines a tip.
    struct Preset {
        const char *name;
        double size;
        int hardness;
        /// Minor axis as a percentage of the major one; 100 is round.
        int roundness;
        /// Tip rotation, degrees.
        int angle;
        /// Scatter as a percentage of diameter.
        int scatter;
        /// Dabs per step.
        int count;
        int sizeJitter;
        int angleJitter;
        int roundnessJitter;
        /// Dab spacing as a percentage of diameter.
        int spacing;
    };

    explicit BrushPresetPicker(Engine *engine, QWidget *parent = nullptr);

    /// Show the picker beneath `anchor`, as a popup that closes on an outside
    /// click.
    void popUpUnder(QWidget *anchor);

    double brushSize() const { return m_current.size; }
    int hardness() const { return m_current.hardness; }
    /// The whole current tip.
    const Preset &current() const { return m_current; }

    /// Set the size and hardness without emitting — for syncing the popup to
    /// state that changed elsewhere.
    void setValues(double size, int hardness);

    /// Render the current tip at `edge` pixels square, for the options-bar
    /// button. Goes through the engine, so it matches what will be painted.
    QPixmap tipPreview(int edge);

signals:
    /// The tip changed: a slider moved, a number was typed, or a preset picked.
    void tipChanged(const BrushPresetPicker::Preset &preset);

private:
    /// Adopt a preset, updating the controls and preview, and announce it once.
    void apply(const Preset &preset, bool announce);
    void refreshPreview();
    /// Fill the grid, rendering each thumbnail through the engine.
    void buildPresetGrid();
    /// Point the engine's brush at `preset`, so a preview reflects it.
    void pushToEngine(const Preset &preset) const;

    Engine *m_engine = nullptr;
    Preset m_current{};
    /// Guards the slider/spin-box round trip, so setting one does not bounce
    /// back through the other.
    bool m_updating = false;

    QLabel *m_preview = nullptr;
    QSlider *m_sizeSlider = nullptr;
    QSpinBox *m_sizeValue = nullptr;
    QSlider *m_hardnessSlider = nullptr;
    QSpinBox *m_hardnessValue = nullptr;
    QLabel *m_currentLabel = nullptr;
    QListWidget *m_presets = nullptr;
};
