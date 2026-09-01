#pragma once

#include <QDialog>
#include <QString>

class Engine;
class QComboBox;
class QLabel;
class QLineEdit;

/// Photoshop's Layer ▸ Duplicate Layer.
///
/// Names the copy and says where it goes: back into this document, into
/// another open one, or into a document of its own.
class DuplicateLayerDialog : public QDialog
{
    Q_OBJECT
public:
    /// `layerIndex` is a Layers-panel index, top-first, as the engine's layer
    /// calls take.
    DuplicateLayerDialog(Engine *engine, int layerIndex, QWidget *parent = nullptr);

    QString copyName() const;
    /// Document tab index to duplicate into, or -1 for a new document.
    int destination() const;
    /// Name for that new document. Empty unless the destination is New.
    QString newDocumentName() const;

private:
    void onDestinationChanged();

    Engine *m_engine = nullptr;
    QLineEdit *m_name = nullptr;
    QComboBox *m_destination = nullptr;
    QLabel *m_documentNameLabel = nullptr;
    QLineEdit *m_documentName = nullptr;
};
