#include "DuplicateLayerDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QComboBox>
#include <QGridLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QVBoxLayout>

DuplicateLayerDialog::DuplicateLayerDialog(Engine *engine, int layerIndex, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Duplicate Layer"));

    auto *outer = new QHBoxLayout(this);
    auto *left = new QVBoxLayout;
    auto *top = new QGridLayout;

    const QString source = engine ? engine->layerName(layerIndex) : QString();
    top->addWidget(new QLabel(tr("Duplicate:")), 0, 0, Qt::AlignRight);
    top->addWidget(new QLabel(source), 0, 1);

    top->addWidget(new QLabel(tr("As:")), 1, 0, Qt::AlignRight);
    m_name = new QLineEdit(source.isEmpty() ? QString() : source + tr(" copy"));
    m_name->setMinimumWidth(220);
    top->addWidget(m_name, 1, 1);
    left->addLayout(top);

    // --- Destination ------------------------------------------------------
    auto *destBox = new QGroupBox(tr("Destination"));
    auto *destGrid = new QGridLayout(destBox);
    destGrid->addWidget(new QLabel(tr("Document:")), 0, 0, Qt::AlignRight);
    m_destination = new QComboBox;
    if (engine) {
        // Every open document, this one first, then a document of its own.
        // The tab index rides along as the item's data, so the order shown
        // never has to match the order the engine keeps.
        const int active = engine->activeDocument();
        m_destination->addItem(engine->documentTitleAt(active), active);
        for (int i = 0; i < engine->documentCount(); ++i) {
            if (i != active) {
                m_destination->addItem(engine->documentTitleAt(i), i);
            }
        }
    }
    m_destination->addItem(tr("New"), -1);
    destGrid->addWidget(m_destination, 0, 1);

    m_documentNameLabel = new QLabel(tr("Name:"));
    destGrid->addWidget(m_documentNameLabel, 1, 0, Qt::AlignRight);
    m_documentName = new QLineEdit;
    destGrid->addWidget(m_documentName, 1, 1);
    left->addWidget(destBox);

    left->addStretch();
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
    connect(m_destination, &QComboBox::currentIndexChanged, this,
            [this] { onDestinationChanged(); });

    // Seed the new-document name from the copy's, the way CS6 does, and keep
    // it following until the destination is actually New.
    m_documentName->setText(m_name->text());
    connect(m_name, &QLineEdit::textChanged, this, [this](const QString &text) {
        if (destination() != -1) {
            m_documentName->setText(text);
        }
    });

    onDestinationChanged();

    m_name->setFocus();
    m_name->selectAll();
}

void DuplicateLayerDialog::onDestinationChanged()
{
    // The name box is for the document being created, so it means nothing
    // until "New" is the destination.
    const bool toNew = destination() == -1;
    m_documentNameLabel->setEnabled(toNew);
    m_documentName->setEnabled(toNew);
}

QString DuplicateLayerDialog::copyName() const
{
    return m_name->text().trimmed();
}

int DuplicateLayerDialog::destination() const
{
    return m_destination->currentData().toInt();
}

QString DuplicateLayerDialog::newDocumentName() const
{
    return destination() == -1 ? m_documentName->text().trimmed() : QString();
}
