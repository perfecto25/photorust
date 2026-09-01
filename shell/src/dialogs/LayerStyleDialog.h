#pragma once

#include <QDialog>
#include <QString>
#include <QStringList>

class Engine;
class AngleDial;
class BlendIfSlider;
class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QLabel;
class QListWidget;
class QListWidgetItem;
class QPushButton;
class QSlider;
class QSpinBox;
class QStackedWidget;
class QWidget;

/// Photoshop's Layer Style dialog.
///
/// The effect list on the left, one settings page each on the right, and a live
/// preview on the canvas: every control writes straight into the engine as it
/// moves, without adding a history step. OK commits the lot as one; Cancel puts
/// back the values the dialog opened with.
class LayerStyleDialog : public QDialog
{
    Q_OBJECT
public:
    /// `effect` is the key prefix of the page to open on — "dropShadow",
    /// "stroke" and so on — matching the menu entry the user picked.
    LayerStyleDialog(Engine *engine, int layerIndex, const QString &effect,
                     QWidget *parent = nullptr);

public slots:
    /// Put back everything the dialog changed.
    ///
    /// Overridden rather than wired to the Cancel button, because Cancel is
    /// only one of the ways out: Escape and the window's close button both
    /// reject the dialog directly, and each of those left the live preview's
    /// edits on the layer.
    void reject() override;

private:
    /// Build one effect's page and its row in the list.
    void addEffect(const QString &key, const QString &title, QWidget *page);
    QWidget *buildBlendingOptionsPage();
    QWidget *buildBevelPage();
    QWidget *buildSatinPage();
    QWidget *buildShadowPage(const QString &key, bool inner);
    QWidget *buildGlowPage(const QString &key);
    QWidget *buildColorOverlayPage();
    QWidget *buildGradientOverlayPage();
    QWidget *buildPatternOverlayPage();
    QWidget *buildStrokePage();
    /// A row for an effect the engine cannot draw yet.
    void addPendingEffect(const QString &title);
    /// A page that is always available and has nothing to switch on.
    void addFixedPage(const QString &title, QWidget *page);

    /// Controls are wired by key: each one reads its starting value from the
    /// engine and writes back to the same key as it changes.
    void bindCheck(QCheckBox *box, const QString &key);
    void bindSpin(QDoubleSpinBox *spin, const QString &key, double scale = 1.0);
    /// A CS6 slider-and-number pair on one setting: drag it or type it, and
    /// the two follow each other.
    QWidget *sliderRow(const QString &key, double min, double max, double scale,
                       const QString &suffix);
    void bindBlendMode(QComboBox *combo, const QString &key);
    /// CS6's angle control: the dial and the number beside it, both on one
    /// setting.
    QWidget *angleRow(const QString &key);
    /// A combo whose row index is the setting's value.
    void bindChoice(QComboBox *combo, const QString &key);
    void bindColor(QPushButton *button, const QString &key);

    void setValue(const QString &key, float value);
    float value(const QString &key) const;
    /// Repaint the canvas behind the dialog.
    void previewChanged();
    void onListChanged();

    Engine *m_engine = nullptr;
    int m_layerIndex = 0;
    /// Key prefix per list row, so ticking a row enables that effect.
    QStringList m_effectKeys;

    QListWidget *m_list = nullptr;
    QStackedWidget *m_pages = nullptr;
    bool m_updating = false;
};
