#include "FileInfoDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QDialogButtonBox>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QImageReader>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QLocale>
#include <QMessageBox>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QScrollArea>
#include <QStackedWidget>
#include <QVBoxLayout>

namespace {

/// CS6's category list, in its order. The engine reads the first three; the
/// rest are named so the dialog says what it is not showing.
const char *const kCategories[] = {
    "Basic",       "Camera Data", "Origin",     "IPTC",  "IPTC Extension", "GPS Data",
    "Audio Data",  "Video Data",  "Photoshop",  "DICOM", "Raw Data",
};

/// Why a pane is empty. The two cases are different and worth telling apart:
/// a standard we do not read at all, versus one we do read and this file has
/// nothing under.
QString emptyReason(const QString &category)
{
    static const QSet<QString> read = {
        QStringLiteral("Basic"),
        QStringLiteral("Camera Data"),
        QStringLiteral("Raw Data"),
    };
    if (read.contains(category)) {
        return FileInfoDialog::tr("This file carries no %1 information.").arg(category);
    }
    return FileInfoDialog::tr("%1 is not read yet. The file may still contain it.")
        .arg(category);
}

} // namespace

FileInfoDialog::FileInfoDialog(Engine *engine, const QString &path, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
    , m_path(path)
{
    setWindowTitle(QFileInfo(path).fileName());
    resize(760, 560);
    load();
    buildUi();
}

void FileInfoDialog::show(Engine *engine, const QString &path, QWidget *parent)
{
    if (path.isEmpty()) {
        QMessageBox::information(
            parent, tr("File Info"),
            tr("This document has not been saved yet, so there is no file to read "
               "information from."));
        return;
    }

    FileInfoDialog dialog(engine, path, parent);
    dialog.exec();
}

void FileInfoDialog::load()
{
    addFileFields();

    if (!m_engine) {
        return;
    }

    // One record per line, `category<TAB>label<TAB>value`.
    const QString records = m_engine->fileMetadata(m_path);
    const QStringList lines = records.split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    for (const QString &line : lines) {
        const QStringList parts = line.split(QLatin1Char('\t'));
        if (parts.size() < 3) {
            continue;
        }
        m_fields[parts.at(0)].append({parts.at(1), parts.mid(2).join(QLatin1Char('\t'))});
    }

    m_xmp = m_engine->fileXmp(m_path);
}

void FileInfoDialog::addFileFields()
{
    const QFileInfo info(m_path);
    if (!info.exists()) {
        return;
    }

    auto &basic = m_fields[QStringLiteral("Basic")];
    basic.append({tr("File Name"), info.fileName()});
    basic.append({tr("Location"), info.absolutePath()});
    basic.append({tr("File Size"), QLocale().formattedDataSize(info.size())});
    basic.append({tr("Created"), QLocale().toString(info.birthTime(), QLocale::ShortFormat)});
    basic.append(
        {tr("Modified"), QLocale().toString(info.lastModified(), QLocale::ShortFormat)});

    // The image's own shape, from the file rather than from the open document:
    // File Info describes what is on disk, which may be several edits behind.
    QImageReader reader(m_path);
    const QSize size = reader.size();
    if (size.isValid()) {
        basic.append({tr("Dimensions"), tr("%1 × %2 px").arg(size.width()).arg(size.height())});
    }
    const QString format = QString::fromLatin1(reader.format()).toUpper();
    if (!format.isEmpty()) {
        basic.append({tr("Format"), format});
    }
}

void FileInfoDialog::buildUi()
{
    auto *root = new QVBoxLayout(this);
    auto *columns = new QHBoxLayout();
    columns->setSpacing(10);

    m_categories = new QListWidget(this);
    m_categories->setFixedWidth(150);
    columns->addWidget(m_categories);

    m_panes = new QStackedWidget(this);
    columns->addWidget(m_panes, 1);
    root->addLayout(columns, 1);

    for (const char *name : kCategories) {
        const QString category = QString::fromLatin1(name);
        m_categories->addItem(category);
        m_panes->addWidget(category == QLatin1String("Raw Data") ? buildRawPane()
                                                                 : buildPane(category));
    }

    connect(m_categories, &QListWidget::currentRowChanged, m_panes,
            &QStackedWidget::setCurrentIndex);
    m_categories->setCurrentRow(0);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Close, this);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    root->addWidget(buttons);
}

QWidget *FileInfoDialog::buildPane(const QString &category) const
{
    auto *pane = new QWidget();
    auto *layout = new QVBoxLayout(pane);
    layout->setContentsMargins(4, 4, 4, 4);

    const auto rows = m_fields.value(category);
    if (rows.isEmpty()) {
        auto *empty = new QLabel(emptyReason(category), pane);
        empty->setWordWrap(true);
        empty->setEnabled(false);
        layout->addWidget(empty);
        layout->addStretch(1);
        return pane;
    }

    auto *form = new QFormLayout();
    form->setLabelAlignment(Qt::AlignRight);
    for (const auto &row : rows) {
        auto *value = new QLabel(row.second, pane);
        // Selectable so a serial number or a date can be copied out, which is
        // half of what anyone opens this dialog for.
        value->setTextInteractionFlags(Qt::TextSelectableByMouse);
        value->setWordWrap(true);
        form->addRow(row.first + QLatin1Char(':'), value);
    }
    layout->addLayout(form);
    layout->addStretch(1);
    return pane;
}

QWidget *FileInfoDialog::buildRawPane()
{
    auto *pane = new QWidget();
    auto *layout = new QVBoxLayout(pane);
    layout->setContentsMargins(4, 4, 4, 4);

    if (m_xmp.isEmpty()) {
        auto *empty = new QLabel(emptyReason(QStringLiteral("Raw Data")), pane);
        empty->setWordWrap(true);
        empty->setEnabled(false);
        layout->addWidget(empty);
        layout->addStretch(1);
        return pane;
    }

    // CS6 puts a search box over the raw packet, which is the only way to find
    // anything in a few hundred lines of XML.
    auto *row = new QHBoxLayout();
    row->addWidget(new QLabel(tr("Find:"), pane));
    m_rawFilter = new QLineEdit(pane);
    m_rawFilter->setPlaceholderText(tr("Show only lines containing…"));
    row->addWidget(m_rawFilter, 1);
    layout->addLayout(row);

    m_raw = new QPlainTextEdit(pane);
    m_raw->setReadOnly(true);
    m_raw->setLineWrapMode(QPlainTextEdit::NoWrap);
    m_raw->setPlainText(m_xmp);
    layout->addWidget(m_raw, 1);

    connect(m_rawFilter, &QLineEdit::textChanged, this, [this](const QString &needle) {
        if (needle.isEmpty()) {
            m_raw->setPlainText(m_xmp);
            return;
        }
        QStringList kept;
        for (const QString &line : m_xmp.split(QLatin1Char('\n'))) {
            if (line.contains(needle, Qt::CaseInsensitive)) {
                kept.append(line);
            }
        }
        m_raw->setPlainText(kept.join(QLatin1Char('\n')));
    });

    return pane;
}
