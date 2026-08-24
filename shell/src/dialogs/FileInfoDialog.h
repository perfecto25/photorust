#pragma once

#include <QDialog>
#include <QHash>
#include <QString>
#include <QStringList>

class Engine;
class QLineEdit;
class QListWidget;
class QPlainTextEdit;
class QStackedWidget;

/// File ▸ File Info: what a file says about itself.
///
/// Laid out like CS6's — a list of categories down the left, the chosen one's
/// fields on the right — and listing all of its categories, including the ones
/// nothing is read for yet. An empty pane that names the standard it would show
/// is more use than a pane that is not there: it says the file may well carry
/// that data and we are not reading it, rather than implying there is none.
///
/// Read-only, deliberately. Photoshop's File Info edits metadata and writes it
/// back; ours reads. Offering editable fields that cannot be saved would be a
/// worse lie than not offering them.
class FileInfoDialog : public QDialog
{
    Q_OBJECT

public:
    /// \param path  The file to describe. Its name becomes the caption, as in
    ///              Photoshop, where the dialog is titled after the document.
    FileInfoDialog(Engine *engine, const QString &path, QWidget *parent = nullptr);

    /// Show it for `path`. A document that has never been saved has no file to
    /// read, and says so rather than opening an empty dialog.
    static void show(Engine *engine, const QString &path, QWidget *parent);

private:
    void buildUi();
    /// Read the file and sort what comes back into panes.
    void load();
    /// The pane for one category: its rows, or a line saying why there are
    /// none.
    QWidget *buildPane(const QString &category) const;
    /// The Raw Data pane — the XMP packet, with a filter over it.
    QWidget *buildRawPane();
    /// What the file itself says: name, size, dates, and the image's own size.
    void addFileFields();

    Engine *m_engine = nullptr;
    QString m_path;

    /// Fields read from the file, grouped by the pane they belong on.
    QHash<QString, QList<QPair<QString, QString>>> m_fields;
    QString m_xmp;

    QListWidget *m_categories = nullptr;
    QStackedWidget *m_panes = nullptr;
    QPlainTextEdit *m_raw = nullptr;
    QLineEdit *m_rawFilter = nullptr;
};
