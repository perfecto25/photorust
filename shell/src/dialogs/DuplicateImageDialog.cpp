#include "DuplicateImageDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QCheckBox>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QVBoxLayout>

DuplicateImageDialog::DuplicateImageDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle(tr("Duplicate Image"));

    auto *outer = new QHBoxLayout(this);
    auto *left = new QGridLayout;

    // The document being copied, as a readout — this row is not editable in
    // CS6 either.
    left->addWidget(new QLabel(tr("Duplicate:")), 0, 0, Qt::AlignRight);
    left->addWidget(new QLabel(engine ? engine->documentName() : QString()), 0, 1);

    left->addWidget(new QLabel(tr("As:")), 1, 0, Qt::AlignRight);
    m_name = new QLineEdit;
    if (engine) {
        m_name->setText(engine->documentCopyName());
    }
    m_name->setMinimumWidth(240);
    left->addWidget(m_name, 1, 1);

    m_merged = new QCheckBox(tr("Duplicate Merged Layers Only"));
    left->addWidget(m_merged, 2, 0, 1, 2);

    outer->addLayout(left, 1);

    auto *buttons = new QVBoxLayout;
    auto *ok = new QPushButton(tr("OK"));
    ok->setDefault(true);
    ok->setFixedWidth(90);
    auto *cancel = new QPushButton(tr("Cancel"));
    cancel->setFixedWidth(90);
    buttons->addWidget(ok);
    buttons->addWidget(cancel);
    buttons->addStretch();
    outer->addLayout(buttons);

    connect(ok, &QPushButton::clicked, this, &QDialog::accept);
    connect(cancel, &QPushButton::clicked, this, &QDialog::reject);

    // Opens with the suggested name selected, so typing replaces it — the
    // name is the one thing most people change here.
    m_name->setFocus();
    m_name->selectAll();
}

QString DuplicateImageDialog::copyName() const
{
    return m_name->text().trimmed();
}

bool DuplicateImageDialog::mergedOnly() const
{
    return m_merged->isChecked();
}
