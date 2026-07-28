#pragma once

#include <QComboBox>
#include <QLabel>
#include <QListWidget>
#include <QSlider>
#include <QToolButton>
#include <QWidget>

class Engine;

/// The Layers panel.
///
/// Rows are listed **top layer first**, matching Photoshop. The engine's bridge
/// already speaks in these panel indices, so no flipping happens here.
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

    void addLayer();
    void deleteLayer();
    void duplicateLayer();
    void addMask();
    void mergeDown();

private:
    void buildUi();
    void populateBlendModes();
    /// Row index currently selected, or -1.
    int currentIndex() const;

    Engine *m_engine = nullptr;

    QComboBox *m_blendMode = nullptr;
    QSlider *m_opacity = nullptr;
    QLabel *m_opacityLabel = nullptr;
    QSlider *m_fillOpacity = nullptr;
    QLabel *m_fillLabel = nullptr;
    QListWidget *m_list = nullptr;

    QToolButton *m_addButton = nullptr;
    QToolButton *m_deleteButton = nullptr;
    QToolButton *m_duplicateButton = nullptr;
    QToolButton *m_maskButton = nullptr;
    QToolButton *m_mergeButton = nullptr;

    /// Guards against re-entrancy: refresh() writes to the widgets, whose
    /// change signals would otherwise write straight back to the engine.
    bool m_updating = false;
};
