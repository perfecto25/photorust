#pragma once

#include <QDialog>
#include <QString>

class Engine;
class QCheckBox;
class QLineEdit;

/// Photoshop's Image ▸ Duplicate.
///
/// Names the copy and says whether it takes the whole layer stack or just the
/// flattened image. The copy opens in its own tab.
class DuplicateImageDialog : public QDialog
{
    Q_OBJECT
public:
    explicit DuplicateImageDialog(Engine *engine, QWidget *parent = nullptr);

    QString copyName() const;
    bool mergedOnly() const;

private:
    QLineEdit *m_name = nullptr;
    QCheckBox *m_merged = nullptr;
};
