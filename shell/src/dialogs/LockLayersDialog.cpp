#include "LockLayersDialog.h"

#include "panels/LayerIcons.h"

#include <QCheckBox>
#include <QDialogButtonBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QVBoxLayout>

namespace {

/// The glyphs the Layers panel's Lock row uses, so the dialog names the same
/// four locks in the same language.
QCheckBox *lockBox(LayerIcons::Glyph glyph, const QString &text, QWidget *parent,
                   QBoxLayout *into)
{
    auto *row = new QWidget(parent);
    auto *layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(6);

    auto *icon = new QLabel(row);
    icon->setPixmap(LayerIcons::pixmap(glyph, QColor(0xcf, 0xcf, 0xcf), 16));
    icon->setFixedWidth(18);
    layout->addWidget(icon);

    auto *box = new QCheckBox(text, row);
    layout->addWidget(box, 1);
    into->addWidget(row);
    return box;
}

} // namespace

LockLayersDialog::LockLayersDialog(bool transparency, bool pixels, bool position,
                                   QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle(tr("Lock Layers"));

    auto *layout = new QVBoxLayout(this);
    layout->setSpacing(6);

    m_transparency = lockBox(LayerIcons::Glyph::LockTransparency,
                             tr("Lock Transparent Pixels"), this, layout);
    m_pixels = lockBox(LayerIcons::Glyph::LockImage, tr("Lock Image Pixels"), this, layout);
    m_position = lockBox(LayerIcons::Glyph::LockPosition, tr("Lock Position"), this, layout);
    layout->addSpacing(4);
    m_all = lockBox(LayerIcons::Glyph::LockAll, tr("Lock All"), this, layout);

    m_transparency->setChecked(transparency);
    m_pixels->setChecked(pixels);
    m_position->setChecked(position);
    syncLockAll();

    // Lock All is the other three together, not a lock of its own.
    connect(m_all, &QCheckBox::toggled, this, [this](bool on) {
        if (m_syncing) {
            return;
        }
        m_syncing = true;
        m_transparency->setChecked(on);
        m_pixels->setChecked(on);
        m_position->setChecked(on);
        m_syncing = false;
    });
    for (QCheckBox *box : {m_transparency, m_pixels, m_position}) {
        connect(box, &QCheckBox::toggled, this, [this] {
            if (!m_syncing) {
                syncLockAll();
            }
        });
    }

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel,
                                         this);
    connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addWidget(buttons);
}

void LockLayersDialog::syncLockAll()
{
    m_syncing = true;
    m_all->setChecked(m_transparency->isChecked() && m_pixels->isChecked()
                      && m_position->isChecked());
    m_syncing = false;
}

bool LockLayersDialog::lockTransparency() const
{
    return m_transparency->isChecked();
}

bool LockLayersDialog::lockPixels() const
{
    return m_pixels->isChecked();
}

bool LockLayersDialog::lockPosition() const
{
    return m_position->isChecked();
}
