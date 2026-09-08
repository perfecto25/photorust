#include "ParagraphStyleDialog.h"

#include "ColorPickerDialog.h"

#include <QComboBox>
#include <QDialogButtonBox>
#include <QFontDatabase>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QIntValidator>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QPainter>
#include <QSpinBox>
#include <QStackedWidget>
#include <QToolButton>
#include <QVBoxLayout>

namespace {

/// The same preset sizes the Type options bar and Character panel offer.
const int kSizePresets[] = {6, 7, 8, 9, 10, 11, 12, 14, 18, 24, 30, 36,
                            48, 60, 72, 96, 144, 192, 288};

} // namespace

ParagraphStyleDialog::ParagraphStyleDialog(const ParagraphStyle &style, QWidget *parent)
    : QDialog(parent)
    , m_style(style)
{
    setWindowTitle(tr("Paragraph Style Options"));

    auto *root = new QVBoxLayout(this);
    auto *body = new QHBoxLayout();

    m_pageList = new QListWidget(this);
    m_pageList->setFixedWidth(190);
    body->addWidget(m_pageList);

    auto *right = new QVBoxLayout();
    auto *nameRow = new QHBoxLayout();
    nameRow->addWidget(new QLabel(tr("Style Name:"), this));
    m_name = new QLineEdit(m_style.name, this);
    nameRow->addWidget(m_name, 1);
    right->addLayout(nameRow);

    m_pages = new QStackedWidget(this);
    right->addWidget(m_pages, 1);
    body->addLayout(right, 1);
    root->addLayout(body, 1);

    addPage(tr("Basic Character Formats"), buildBasicCharacterPage());
    addPage(tr("Advanced Character Formats"), buildAdvancedCharacterPage());
    addPage(tr("Indents and Spacing"), buildIndentsPage());

    connect(m_pageList, &QListWidget::currentRowChanged, m_pages,
            &QStackedWidget::setCurrentIndex);
    m_pageList->setCurrentRow(0);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    root->addWidget(buttons);

    resize(620, 380);
}

void ParagraphStyleDialog::addPage(const QString &title, QWidget *page)
{
    new QListWidgetItem(title, m_pageList);
    m_pages->addWidget(page);
}

QWidget *ParagraphStyleDialog::buildBasicCharacterPage()
{
    auto *page = new QWidget(this);
    auto *form = new QFormLayout(page);

    m_family = new QComboBox(page);
    QStringList families;
    for (const QString &name : QFontDatabase::families()) {
        // Left out for the same reason as everywhere else type is chosen here:
        // CS6 predates colour emoji fonts.
        if (!name.contains(QLatin1String("Emoji"), Qt::CaseInsensitive)) {
            families << name;
        }
    }
    m_family->addItems(families);
    const int familyIdx = m_family->findText(m_style.family);
    m_family->setCurrentIndex(familyIdx >= 0 ? familyIdx : 0);
    form->addRow(tr("Font Family:"), m_family);

    m_styleCombo = new QComboBox(page);
    form->addRow(tr("Font Style:"), m_styleCombo);
    refreshStyles(m_family->currentText(), m_style.style);
    connect(m_family, &QComboBox::currentTextChanged, this, [this](const QString &family) {
        refreshStyles(family, m_styleCombo->currentText());
    });

    m_size = new QComboBox(page);
    m_size->setEditable(true);
    m_size->setValidator(new QIntValidator(1, 1296, m_size));
    for (int pt : kSizePresets) {
        m_size->addItem(QString::number(pt));
    }
    m_size->setCurrentText(QString::number(int(m_style.size)));
    form->addRow(tr("Size:"), m_size);

    m_colorSwatch = new QToolButton(page);
    m_colorSwatch->setFixedSize(60, 22);
    refreshSwatch();
    connect(m_colorSwatch, &QToolButton::clicked, this, [this] {
        const QColor picked = ColorPickerDialog::getColor(m_style.color, this, tr("Text Color"));
        if (picked.isValid()) {
            m_style.color = picked;
            refreshSwatch();
        }
    });
    form->addRow(tr("Color:"), m_colorSwatch);

    return page;
}

QWidget *ParagraphStyleDialog::buildAdvancedCharacterPage()
{
    auto *page = new QWidget(this);
    auto *form = new QFormLayout(page);

    // Both of these the text record carries per run, so a style can set them.
    // CS6's Baseline Shift and Language sit on this page too; neither has
    // anywhere to land, so neither is offered.
    m_hScale = new QSpinBox(page);
    m_hScale->setRange(1, 1000);
    m_hScale->setSuffix(QStringLiteral("%"));
    m_hScale->setValue(qRound(m_style.hScale * 100.0));
    form->addRow(tr("Horizontal Scale:"), m_hScale);

    m_vScale = new QSpinBox(page);
    m_vScale->setRange(1, 1000);
    m_vScale->setSuffix(QStringLiteral("%"));
    m_vScale->setValue(qRound(m_style.vScale * 100.0));
    form->addRow(tr("Vertical Scale:"), m_vScale);

    return page;
}

QWidget *ParagraphStyleDialog::buildIndentsPage()
{
    auto *page = new QWidget(this);
    auto *form = new QFormLayout(page);

    // Alignment is the whole of this page that the engine can honour; the
    // five indent and spacing fields beside it in CS6 are not offered,
    // because text layout here has no notion of either.
    m_alignment = new QComboBox(page);
    m_alignment->addItem(tr("Left"), int(Qt::AlignLeft));
    m_alignment->addItem(tr("Center"), int(Qt::AlignHCenter));
    m_alignment->addItem(tr("Right"), int(Qt::AlignRight));
    const int idx = m_alignment->findData(int(m_style.alignment));
    m_alignment->setCurrentIndex(idx >= 0 ? idx : 0);
    form->addRow(tr("Alignment:"), m_alignment);

    return page;
}

void ParagraphStyleDialog::refreshStyles(const QString &family, const QString &wanted)
{
    const QSignalBlocker blocker(m_styleCombo);
    m_styleCombo->clear();
    QStringList styles = QFontDatabase::styles(family);
    if (styles.isEmpty()) {
        styles << tr("Regular");
    }
    m_styleCombo->addItems(styles);
    const int idx = m_styleCombo->findText(wanted);
    m_styleCombo->setCurrentIndex(idx >= 0 ? idx : 0);
}

void ParagraphStyleDialog::refreshSwatch()
{
    QPixmap pm(48, 14);
    pm.fill(m_style.color);
    QPainter p(&pm);
    p.setPen(QColor(0, 0, 0, 160));
    p.drawRect(pm.rect().adjusted(0, 0, -1, -1));
    m_colorSwatch->setIcon(QIcon(pm));
    m_colorSwatch->setIconSize(pm.size());
}

ParagraphStyle ParagraphStyleDialog::style() const
{
    ParagraphStyle out = m_style;
    out.name = m_name->text().trimmed().isEmpty() ? m_style.name : m_name->text().trimmed();
    out.family = m_family->currentText();
    out.style = m_styleCombo->currentText();

    bool ok = false;
    const double size = m_size->currentText().toDouble(&ok);
    out.size = (ok && size > 0) ? size : m_style.size;

    out.hScale = m_hScale->value() / 100.0;
    out.vScale = m_vScale->value() / 100.0;
    out.alignment = Qt::Alignment(m_alignment->currentData().toInt());
    // Colour is not read back from a widget: the swatch is a button, so
    // picking one writes straight to `m_style` above.
    return out;
}
