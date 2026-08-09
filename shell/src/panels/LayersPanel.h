#pragma once

#include <QBoxLayout>
#include <QComboBox>
#include <QLabel>
#include <QListWidget>
#include <QSpinBox>
#include <QToolButton>
#include <QWidget>

class Engine;
class LayerListWidget;

/// The Layers panel.
///
/// Rows are listed **top layer first**, matching Photoshop. The engine's bridge
/// already speaks in these panel indices, so no flipping happens here.
///
/// The layout follows CS6 row for row: the filter row (Kind and the layer-type
/// buttons), the blend mode and Opacity, the Lock row and Fill, the list, and
/// the seven glyphs along the foot. Rows are painted by a delegate rather than
/// left to Qt, because CS6's row is an eye column, a bordered thumbnail, the
/// name and a padlock badge — none of which a stock list item draws.
class LayersPanel : public QWidget
{
    Q_OBJECT

public:
    explicit LayersPanel(Engine *engine, QWidget *parent = nullptr);

public slots:
    /// Rebuild the list from the engine.
    void refresh();

signals:
    /// Something changed that requires the canvas to repaint.
    void documentChanged();

private slots:
    void onSelectionChanged();
    void onBlendModeChanged(int index);
    void onOpacityChanged(int value);
    void onFillOpacityChanged(int value);
    void onItemChanged(QListWidgetItem *item);
    void onRowsMoved();
    void onRowContextMenu(const QPoint &pos);

    void addLayer();
    void deleteLayer();
    void duplicateLayer();
    void addMask();
    void addAdjustmentLayer();
    void mergeDown();

private:
    void buildUi();
    /// The filter row CS6 puts above the blend mode: Kind, the layer-type
    /// buttons, and the switch that turns filtering on.
    void buildFilterRow(QWidget *parent, QBoxLayout *into);
    /// The Lock row: transparency, image pixels, position, all.
    void buildLockRow(QWidget *parent, QBoxLayout *into);
    void populateBlendModes();
    /// Row index currently selected, or -1.
    int currentIndex() const;

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

    LayerListWidget *m_list = nullptr;

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
