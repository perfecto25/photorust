#pragma once

#include <QBoxLayout>
#include <QComboBox>
#include <QLabel>
#include <QTreeWidget>
#include <QSpinBox>
#include <QToolButton>
#include <QSet>
#include <QWidget>

class Engine;
class LayerTreeWidget;

/// The Layers panel.
///
/// Rows are listed **top layer first**, matching Photoshop. The engine's bridge
/// already speaks in these panel indices, so no flipping happens here.
///
/// The layout follows CS6 row for row: the filter row (Kind and the layer-type
/// buttons), the blend mode and Opacity, the Lock row and Fill, the list, and
/// the seven glyphs along the foot. Rows are painted by a delegate rather than
/// left to Qt, because CS6's row is an eye column, a bordered thumbnail, the
/// name and a padlock badge — none of which a stock item draws.
///
/// It is a **tree**, not a list: a styled layer carries an "Effects" branch
/// with one row per effect, as CS6 does. Everything below therefore works from
/// the layer index stored on each row rather than from the row's position —
/// with children in the way, the two are no longer the same number.
class LayersPanel : public QWidget
{
    Q_OBJECT

public:
    explicit LayersPanel(Engine *engine, QWidget *parent = nullptr);

    QList<int> selectedIndices() const;

public slots:
    /// Rebuild the list from the engine.
    void refresh();

signals:
    /// Something changed that requires the canvas to repaint.
    void documentChanged();
    /// A double-click on an effect row: open Layer Style on that effect, the
    /// way CS6 does. The panel cannot open dialogs itself, so the window does.
    void editLayerStyle(int layerIndex, const QString &effectKey);

private slots:
    void onSelectionChanged();
    void onBlendModeChanged(int index);
    void onOpacityChanged(int value);
    void onFillOpacityChanged(int value);
    void onItemChanged(QTreeWidgetItem *item, int column);
    void onRowContextMenu(const QPoint &pos);

    void addLayer();
    void deleteLayer();
    void duplicateLayer();
    void addMask();
    void addAdjustmentLayer();
    /// The folder button: a new, empty group above the active layer.
    void addGroup();
    void mergeDown();

private:
    void buildUi();
    /// The filter row CS6 puts above the blend mode: Kind, the layer-type
    /// buttons, and the switch that turns filtering on.
    void buildFilterRow(QWidget *parent, QBoxLayout *into);
    /// The Lock row: transparency, image pixels, position, all.
    void buildLockRow(QWidget *parent, QBoxLayout *into);
    void populateBlendModes();
    /// The layer the selection is on, or -1. Selecting an effect row counts as
    /// selecting the layer it belongs to, as it does in CS6.
    int currentIndex() const;
    /// The layer a row belongs to, whether it is the layer's own row or one of
    /// its effect rows.
    int layerIndexOf(const QTreeWidgetItem *item) const;
    /// Build the "Effects" branch under a styled layer's row.
    void buildEffectRows(QTreeWidgetItem *parent, int layerIndex);
    /// The eye on an effect row, or on the Effects branch itself.
    void toggleEffectVisibility(QTreeWidgetItem *item);
    /// The reorder arrows on the selected row: one step up or down the stack.
    void moveLayerBy(int index, bool up);

    /// Toggle a layer's visibility — the eye column.
    void toggleVisibility(int index);
    /// Push the Lock row's four buttons into the engine.
    void applyLocks();
    /// Redraw the Lock row from the active layer's flags.
    void syncLockRow();
    /// Whether a row passes the filter row's current kind selection.
    bool passesFilter(int index) const;
    /// Re-apply the filter to rows already built.
    void applyFilter();
    /// Tell the user why an action was refused, in Photoshop's own words.
    void warnLocked(const QString &action);

    Engine *m_engine = nullptr;

    QComboBox *m_filterKind = nullptr;
    QList<QToolButton *> m_kindButtons;
    QToolButton *m_filterSwitch = nullptr;

    QComboBox *m_blendMode = nullptr;
    QSpinBox *m_opacity = nullptr;
    QSpinBox *m_fillOpacity = nullptr;

    /// The Lock row, in CS6's order: transparency, image, position, all.
    QToolButton *m_lockTransparency = nullptr;
    QToolButton *m_lockImage = nullptr;
    QToolButton *m_lockPosition = nullptr;
    QToolButton *m_lockAll = nullptr;

    LayerTreeWidget *m_tree = nullptr;
    /// Layers whose Effects branch the user has collapsed. Cleared whenever the
    /// stack changes size, since the indices would no longer mean the same
    /// layers.
    QSet<int> m_collapsed;

    QToolButton *m_linkButton = nullptr;
    QToolButton *m_effectsButton = nullptr;
    QToolButton *m_maskButton = nullptr;
    QToolButton *m_adjustmentButton = nullptr;
    QToolButton *m_groupButton = nullptr;
    QToolButton *m_addButton = nullptr;
    QToolButton *m_deleteButton = nullptr;

    /// Guards against re-entrancy: refresh() writes to the widgets, whose
    /// change signals would otherwise write straight back to the engine.
    bool m_updating = false;
};
