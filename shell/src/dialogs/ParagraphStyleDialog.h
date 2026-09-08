#pragma once

#include <QColor>
#include <QDialog>
#include <QString>

class QComboBox;
class QLineEdit;
class QListWidget;
class QSpinBox;
class QStackedWidget;
class QToolButton;

/// A named bundle of type formatting — Photoshop's paragraph style.
///
/// Only what the text record can actually carry is here. CS6's style also
/// holds leading, kerning, tracking, case, position, the OpenType features,
/// indents, composition, justification and hyphenation; none of those has
/// anywhere to land yet, and a style claiming to set them would be lying
/// about what applying it does.
struct ParagraphStyle
{
    QString name = QStringLiteral("Basic Paragraph");
    QString family;
    QString style = QStringLiteral("Regular");
    qreal size = 12.0;
    QColor color = Qt::black;
    /// 1.0 being Photoshop's 100%.
    qreal hScale = 1.0;
    qreal vScale = 1.0;
    Qt::Alignment alignment = Qt::AlignLeft;
};

/// CS6's Paragraph Style Options: a list of pages down the left, the chosen
/// one on the right, with the style's name above them.
///
/// The four pages that would be entirely dead — OpenType Features,
/// Composition, Justification and Hyphenation — are left out rather than
/// shown empty. Every control this dialog does show changes the style.
class ParagraphStyleDialog : public QDialog
{
    Q_OBJECT

public:
    ParagraphStyleDialog(const ParagraphStyle &style, QWidget *parent = nullptr);

    /// The edited style, read back off the widgets. Only meaningful once the
    /// dialog has been accepted.
    ParagraphStyle style() const;

private:
    /// Build one page and the row that selects it, the way
    /// `LayerStyleDialog::addEffect` does.
    void addPage(const QString &title, QWidget *page);
    QWidget *buildBasicCharacterPage();
    QWidget *buildAdvancedCharacterPage();
    QWidget *buildIndentsPage();
    void refreshStyles(const QString &family, const QString &wanted);
    void refreshSwatch();

    QListWidget *m_pageList = nullptr;
    QStackedWidget *m_pages = nullptr;

    QLineEdit *m_name = nullptr;
    QComboBox *m_family = nullptr;
    QComboBox *m_styleCombo = nullptr;
    QComboBox *m_size = nullptr;
    QToolButton *m_colorSwatch = nullptr;
    QSpinBox *m_hScale = nullptr;
    QSpinBox *m_vScale = nullptr;
    QComboBox *m_alignment = nullptr;

    /// What the dialog opened on, and where the colour lives while it is up —
    /// the swatch is a button, so it has nowhere of its own to keep one.
    ParagraphStyle m_style;
};
