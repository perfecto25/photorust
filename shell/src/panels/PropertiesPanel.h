#pragma once

#include <QColor>
#include <QList>
#include <QSlider>
#include <QString>
#include <QWidget>

class CurveWidget;
class Engine;
class SpectrumBar;
class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QLabel;
class QStackedWidget;
class QTimer;
class QToolButton;
class QVBoxLayout;

/// A slider whose groove is a colour ramp.
///
/// CS6 draws the Hue/Saturation sliders this way — a rainbow under Hue, the
/// current colour fading out under Saturation, black to white under Lightness —
/// so the slider says what it does before you move it.
///
/// The ramp is a stylesheet on the groove rather than custom painting: the
/// theme is a stylesheet too, and a stylesheet paints its own background over
/// whatever a `paintEvent` drew first, which leaves the handle floating over
/// nothing. Going through the same machinery also keeps the handle, the hit
/// testing and the disabled state exactly as they are on every other slider.
class RampSlider : public QSlider
{
    Q_OBJECT

public:
    explicit RampSlider(QWidget *parent = nullptr);

    /// Colours across the groove, evenly spaced. Fewer than two puts the
    /// ordinary groove back.
    void setRamp(const QList<QColor> &stops);

private:
    QList<QColor> m_stops;
};

/// The Properties panel.
///
/// CS6's Properties shows the settings of whatever is selected and changes its
/// whole contents to suit. This is the same idea: the active layer decides what
/// the panel builds.
///
/// **Every** adjustment layer is editable here. Rather than eighteen hand-built
/// pages, the controls are described per adjustment and built from that
/// description, and each control knows only the name of the parameter it edits
/// — `Adjustment::value` and `Adjustment::set_value` in the engine are the
/// other end. Adding a parameter to an adjustment therefore means adding a key
/// there and a row here, not a new page.
///
/// Editing goes through the engine's adjustment-edit session
/// (`beginAdjustmentEdit` … `endAdjustmentEdit`), so the canvas updates on
/// every tick while the History panel only gains an entry when the gesture
/// ends — one undo step per drag, not per pixel of slider travel.
class PropertiesPanel : public QWidget
{
    Q_OBJECT

public:
    explicit PropertiesPanel(Engine *engine, QWidget *parent = nullptr);

public slots:
    /// Re-read the active layer and show the controls that fit it.
    void refresh();

signals:
    /// A layer was deleted from here, so the window should re-read everything.
    void documentChanged();

private:
    /// One slider and its number box, editing one engine parameter.
    ///
    /// The engine works in its own units — a fraction of a turn, a multiplier,
    /// a 0-1 level — and CS6 shows degrees and percentages. `scale` and
    /// `offset` are that conversion: `engine = shown * scale + offset`.
    struct SliderRow {
        RampSlider *slider = nullptr;
        QDoubleSpinBox *spin = nullptr;
        /// The parameter's key. Where a group combo repoints the controls
        /// (Channel Mixer's output channel, Selective Color's colour range),
        /// this carries a `%1` filled in from the combo.
        QString keyTemplate;
        int keyBase = 0;
        int keyStride = 0;
        double scale = 1.0;
        double offset = 0.0;
    };

    struct CheckRow {
        QCheckBox *box = nullptr;
        QString key;
    };

    /// A menu whose index *is* the value of a parameter — Selective Color's
    /// method, Hue/Saturation's colour range.
    struct ComboRow {
        QComboBox *combo = nullptr;
        QString key;
    };

    void buildUi();
    QWidget *buildEmptyPage();
    QWidget *buildParametersPage();
    QWidget *buildLayerPage();
    QWidget *buildFooter();

    /// Replace the controls with the ones this adjustment takes.
    void buildParameters(const QString &adjustment);
    /// The Hue/Saturation controls, which Colorize shares on different scales.
    void buildHueSaturation(bool colorize);
    /// Tear down whatever the last adjustment left behind.
    void clearParameters();
    /// Fill the controls from the layer, without writing anything back.
    void loadParameters();
    void loadLayerProperties();

    // -- control builders, used by `buildParameters` --
    //
    // Each adds one row to the page and registers it, so loading and pushing
    // can walk the list without knowing what adjustment is on show.
    void addSlider(const QString &label, const QString &keyTemplate, double minimum,
                   double maximum, double scale, double offset = 0.0, int decimals = 0,
                   const QList<QColor> &ramp = {}, int keyBase = 0, int keyStride = 0);
    void addCheck(const QString &label, const QString &key);
    /// A combo that repoints the sliders' keys rather than setting one.
    void addGroupCombo(const QStringList &options);
    /// A combo whose index *is* the value of a parameter.
    void addValueCombo(const QString &label, const QStringList &options, const QString &key);
    /// A line of explanation where an adjustment has no controls.
    void addNote(const QString &text);
    /// Photo Filter's colour swatch.
    void addColorButton(const QString &label);
    /// Gradient Map's ramp strip and preset menu.
    void addGradientMapControls();
    /// Color Lookup's preset menu.
    void addColorLookupControls();
    /// The curve editor, for a Curves layer.
    void addCurveEditor();

    /// The key a row edits, with any group index filled in.
    QString resolvedKey(const SliderRow &row) const;
    SliderRow *rowForKey(const QString &key);

    bool beginEdit();
    void commitEdit();
    void pushValue(const QString &key, float value);
    void pushRow(const SliderRow &row);
    void onColorizeToggled(bool on);
    void pushCurve();
    void resetAdjustment();
    /// Repaint the Hue/Saturation grooves for the current hue.
    void updateHueRamps();
    void applyHueSaturationPreset(int index);
    void markCustomPreset();

    Engine *m_engine = nullptr;

    /// The layer the panel is showing, as a Layers-panel index. -1 for none.
    int m_layer = -1;
    /// The adjustment the current controls were built for, so they are only
    /// rebuilt when the panel is pointed at something different.
    QString m_builtFor;
    /// True while an uncommitted edit session is open on `m_layer`.
    bool m_editing = false;
    /// True while the panel is writing to the engine, so the change signals
    /// that come straight back do not reload the controls under the user's
    /// finger.
    bool m_applying = false;
    /// True while the panel is loading values into its controls, so their
    /// change signals do not bounce straight back at the engine.
    bool m_loading = false;
    /// Ends a session no gesture closed — a mouse wheel or an arrow key,
    /// which have no release to hang the commit on.
    QTimer *m_commitTimer = nullptr;

    QStackedWidget *m_stack = nullptr;

    // -- the header every page carries --
    QLabel *m_headerIcon = nullptr;
    QLabel *m_headerMask = nullptr;
    QLabel *m_headerTitle = nullptr;

    // -- the built-per-adjustment page --
    QWidget *m_parametersPage = nullptr;
    QVBoxLayout *m_parameterLayout = nullptr;
    QList<SliderRow> m_rows;
    QList<CheckRow> m_checks;
    QList<ComboRow> m_valueCombos;
    /// The combo that repoints keys, if this adjustment has one.
    QComboBox *m_groupCombo = nullptr;
    /// Hue/Saturation's preset menu, and the two spectrum bars beneath it.
    QComboBox *m_presetCombo = nullptr;
    SpectrumBar *m_spectrumBottom = nullptr;
    /// Photo Filter's colour.
    QToolButton *m_colorButton = nullptr;
    /// Gradient Map's strip, and the preset menu that sets it.
    QLabel *m_gradientStrip = nullptr;
    QComboBox *m_gradientPreset = nullptr;
    QCheckBox *m_gradientReverse = nullptr;
    /// Color Lookup's preset menu.
    QComboBox *m_lookupPreset = nullptr;
    /// The Curves editor.
    CurveWidget *m_curve = nullptr;
    QComboBox *m_curveChannel = nullptr;

    // -- footer --
    QToolButton *m_clipButton = nullptr;
    QToolButton *m_resetButton = nullptr;
    QToolButton *m_visibleButton = nullptr;
    QToolButton *m_deleteButton = nullptr;

    // -- the read-only page --
    QLabel *m_kindValue = nullptr;
    QLabel *m_sizeValue = nullptr;
    QLabel *m_positionValue = nullptr;
    QLabel *m_blendValue = nullptr;
    QLabel *m_opacityValue = nullptr;
    QLabel *m_fillValue = nullptr;
    QLabel *m_maskValue = nullptr;
    QLabel *m_lockValue = nullptr;
};
