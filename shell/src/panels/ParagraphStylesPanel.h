#pragma once

#include <QList>
#include <QWidget>

#include "../dialogs/ParagraphStyleDialog.h"

class QListWidget;

/// The Paragraph Styles panel: named bundles of type formatting, applied to
/// the selected type layer by clicking one.
///
/// The styles live here rather than in the document, so they are a property
/// of the session and not of the file — nothing writes them to a `.psd` yet,
/// and putting them in the engine before anything could persist them would
/// buy only undo.
///
/// CS6 marks a style whose text has since been edited away from it with a
/// trailing "+", which needs the layer's formatting compared against the
/// style on every change; that is not done here, so a name is just a name.
class ParagraphStylesPanel : public QWidget
{
    Q_OBJECT

public:
    explicit ParagraphStylesPanel(QWidget *parent = nullptr);

signals:
    /// A style was clicked, and should be applied to the selected type layer.
    void styleApplied(const ParagraphStyle &style);

private:
    void addStyle();
    void deleteSelectedStyle();
    void editStyle(int row);
    /// Put `m_styles` on screen, keeping `current` selected.
    void refreshList(int current);

    QListWidget *m_list = nullptr;
    QList<ParagraphStyle> m_styles;
};
