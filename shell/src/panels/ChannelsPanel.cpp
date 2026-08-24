#include "ChannelsPanel.h"

#include "LayerIcons.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QHBoxLayout>
#include <QInputDialog>
#include <QMouseEvent>
#include <QPainter>
#include <QStyledItemDelegate>
#include <QVBoxLayout>

namespace {
constexpr int kThumbSize = 32;
constexpr int kRowHeight = 40;
constexpr int kEyeColumn = 24;
constexpr int kEyeSize = 15;
constexpr int kFooterGlyph = 18;

const QColor kRowText(0xd4, 0xd4, 0xd4);
const QColor kShortcutText(0x8a, 0x8a, 0x8a);
const QColor kGlyph(0xcf, 0xcf, 0xcf);
const QColor kGlyphOff(0x8a, 0x8a, 0x8a);
const QColor kDivider(0x2a, 0x2a, 0x2a);
const QColor kRowSelected(0x4a, 0x63, 0x83);
const QColor kRowHover(0x46, 0x46, 0x46);

constexpr int kVisibleRole = Qt::UserRole + 1;
constexpr int kThumbRole = Qt::UserRole + 2;
constexpr int kShortcutRole = Qt::UserRole + 3;
constexpr int kIsAlphaRole = Qt::UserRole + 4;

int nameLeft()
{
    return kEyeColumn + 8 + kThumbSize + 8;
}

class ChannelRowDelegate : public QStyledItemDelegate
{
public:
    using QStyledItemDelegate::QStyledItemDelegate;

    void paint(QPainter *painter, const QStyleOptionViewItem &option,
               const QModelIndex &index) const override
    {
        painter->save();
        painter->setRenderHint(QPainter::Antialiasing, true);

        const QRect row = option.rect;
        if (option.state & QStyle::State_Selected)
            painter->fillRect(row, kRowSelected);
        else if (option.state & QStyle::State_MouseOver)
            painter->fillRect(row, kRowHover);

        painter->setPen(kDivider);
        const int divider = row.left() + kEyeColumn;
        painter->drawLine(divider, row.top(), divider, row.bottom());

        if (index.data(kVisibleRole).toBool()) {
            const QPixmap eye = LayerIcons::pixmap(LayerIcons::Glyph::Eye, kGlyph, kEyeSize);
            painter->drawPixmap(row.left() + (kEyeColumn - kEyeSize) / 2,
                                row.top() + (row.height() - kEyeSize) / 2, eye);
        }

        const QPixmap pixmap = index.data(kThumbRole).value<QPixmap>();
        if (!pixmap.isNull()) {
            const QSize size = pixmap.deviceIndependentSize().toSize();
            const QRect thumb(row.left() + kEyeColumn + 8 + (kThumbSize - size.width()) / 2,
                              row.top() + (row.height() - size.height()) / 2,
                              size.width(), size.height());
            painter->drawPixmap(thumb, pixmap);
            painter->setPen(kDivider);
            painter->drawRect(thumb.adjusted(0, 0, -1, -1));
        }

        painter->setFont(option.font);
        painter->setPen(kRowText);
        const int left = row.left() + nameLeft();
        const QString name = index.data(Qt::DisplayRole).toString();
        const QRect textRect(left, row.top(), row.width() - left - 6, row.height());
        painter->drawText(textRect, Qt::AlignVCenter | Qt::AlignLeft, name);

        const QString shortcut = index.data(kShortcutRole).toString();
        if (!shortcut.isEmpty()) {
            painter->setPen(kShortcutText);
            const QRect scRect(left, row.top(), row.right() - left - 6, row.height());
            painter->drawText(scRect, Qt::AlignVCenter | Qt::AlignRight, shortcut);
        }

        painter->restore();
    }

    QSize sizeHint(const QStyleOptionViewItem &, const QModelIndex &) const override
    {
        return QSize(0, kRowHeight);
    }
};

struct ChannelDef {
    QString name;
    QString shortcut;
    QColor tint;
};

QList<ChannelDef> channelsForMode(int mode)
{
    QList<ChannelDef> entries;
    switch (mode) {
    case 0:
        entries.append({QObject::tr("Bitmap"), QStringLiteral("Ctrl+1"), Qt::white});
        break;
    case 1:
        entries.append({QObject::tr("Gray"), QStringLiteral("Ctrl+1"), Qt::white});
        break;
    case 2:
        entries.append({QObject::tr("Duotone"), QStringLiteral("Ctrl+1"), Qt::white});
        break;
    case 3:
        entries.append({QObject::tr("Indexed"), QStringLiteral("Ctrl+1"), Qt::white});
        break;
    case 4:
        entries.append({QObject::tr("RGB"),   QStringLiteral("Ctrl+2"), Qt::white});
        entries.append({QObject::tr("Red"),   QStringLiteral("Ctrl+3"), Qt::red});
        entries.append({QObject::tr("Green"), QStringLiteral("Ctrl+4"), QColor(0, 200, 0)});
        entries.append({QObject::tr("Blue"),  QStringLiteral("Ctrl+5"), Qt::blue});
        break;
    case 5:
        entries.append({QObject::tr("CMYK"),    QStringLiteral("Ctrl+2"), Qt::white});
        entries.append({QObject::tr("Cyan"),    QStringLiteral("Ctrl+3"), Qt::cyan});
        entries.append({QObject::tr("Magenta"), QStringLiteral("Ctrl+4"), Qt::magenta});
        entries.append({QObject::tr("Yellow"),  QStringLiteral("Ctrl+5"), Qt::yellow});
        entries.append({QObject::tr("Black"),   QStringLiteral("Ctrl+6"), Qt::white});
        break;
    case 6:
        entries.append({QObject::tr("Lab"),       QStringLiteral("Ctrl+2"), Qt::white});
        entries.append({QObject::tr("Lightness"), QStringLiteral("Ctrl+3"), Qt::white});
        entries.append({QObject::tr("a"),         QStringLiteral("Ctrl+4"), Qt::white});
        entries.append({QObject::tr("b"),         QStringLiteral("Ctrl+5"), Qt::white});
        break;
    case 7:
        entries.append({QObject::tr("Multichannel"), QStringLiteral("Ctrl+1"), Qt::white});
        break;
    default:
        entries.append({QObject::tr("RGB"),   QStringLiteral("Ctrl+2"), Qt::white});
        entries.append({QObject::tr("Red"),   QStringLiteral("Ctrl+3"), Qt::red});
        entries.append({QObject::tr("Green"), QStringLiteral("Ctrl+4"), QColor(0, 200, 0)});
        entries.append({QObject::tr("Blue"),  QStringLiteral("Ctrl+5"), Qt::blue});
        break;
    }
    return entries;
}

QPixmap channelThumb(const QImage &composite, int channelIndex, int mode, int size)
{
    if (composite.isNull())
        return QPixmap(size, size);

    QImage scaled = composite.scaled(size, size, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    QImage gray(scaled.size(), QImage::Format_Grayscale8);
    gray.fill(0);

    for (int y = 0; y < scaled.height(); ++y) {
        for (int x = 0; x < scaled.width(); ++x) {
            const QRgb px = scaled.pixel(x, y);
            int value = 0;
            if (channelIndex == 0) {
                value = qGray(px);
            } else if (mode == 4) {
                switch (channelIndex) {
                case 1: value = qRed(px); break;
                case 2: value = qGreen(px); break;
                case 3: value = qBlue(px); break;
                }
            } else if (mode == 5) {
                int r = qRed(px), g = qGreen(px), b = qBlue(px);
                float rf = r / 255.0f, gf = g / 255.0f, bf = b / 255.0f;
                float k = 1.0f - qMax(rf, qMax(gf, bf));
                float c = 0, m = 0, yy = 0;
                if (k < 1.0f) {
                    float inv = 1.0f / (1.0f - k);
                    c = (1.0f - rf - k) * inv;
                    m = (1.0f - gf - k) * inv;
                    yy = (1.0f - bf - k) * inv;
                }
                switch (channelIndex) {
                case 1: value = 255 - int(c * 255 + 0.5f); break;
                case 2: value = 255 - int(m * 255 + 0.5f); break;
                case 3: value = 255 - int(yy * 255 + 0.5f); break;
                case 4: value = 255 - int(k * 255 + 0.5f); break;
                }
            } else {
                value = qGray(px);
            }
            gray.setPixel(x, y, qRgb(value, value, value));
        }
    }
    return QPixmap::fromImage(gray);
}
} // namespace

class ChannelListWidget : public QListWidget
{
public:
    using QListWidget::QListWidget;
    std::function<void(int)> onEyeClicked;

protected:
    void mousePressEvent(QMouseEvent *event) override
    {
        if (event->button() == Qt::LeftButton && event->position().x() < kEyeColumn) {
            const QModelIndex index = indexAt(event->position().toPoint());
            if (index.isValid() && onEyeClicked) {
                onEyeClicked(index.row());
                event->accept();
                return;
            }
        }
        QListWidget::mousePressEvent(event);
    }
};

ChannelsPanel::ChannelsPanel(Engine *engine, QWidget *parent)
    : QWidget(parent)
    , m_engine(engine)
{
    buildUi();
    refresh();
}

void ChannelsPanel::buildUi()
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);

    m_list = new ChannelListWidget(this);
    m_list->setObjectName(QStringLiteral("channelList"));
    m_list->setIconSize(QSize(kThumbSize, kThumbSize));
    m_list->setSelectionMode(QAbstractItemView::ExtendedSelection);
    m_list->setItemDelegate(new ChannelRowDelegate(m_list));
    m_list->onEyeClicked = [this](int row) { toggleVisibility(row); };
    root->addWidget(m_list, 1);

    auto *footer = new QWidget(this);
    footer->setObjectName(QStringLiteral("panelFooter"));
    auto *footerLayout = new QHBoxLayout(footer);
    footerLayout->setContentsMargins(4, 2, 4, 2);
    footerLayout->setSpacing(2);

    auto makeButton = [&](LayerIcons::Glyph glyph, const QColor &color,
                          const QString &tip) {
        auto *b = new QToolButton(footer);
        b->setIconSize(QSize(kFooterGlyph, kFooterGlyph));
        b->setIcon(LayerIcons::icon(glyph, color, kFooterGlyph));
        b->setToolTip(tip);
        b->setAutoRaise(true);
        footerLayout->addWidget(b);
        return b;
    };

    m_loadSelectionButton = makeButton(LayerIcons::Glyph::Mask, kGlyphOff,
                                       tr("Load channel as selection"));
    m_loadSelectionButton->setEnabled(false);
    footerLayout->addStretch(1);
    m_addButton = makeButton(LayerIcons::Glyph::NewLayer, kGlyph,
                             tr("Create new channel"));
    m_deleteButton = makeButton(LayerIcons::Glyph::Delete, kGlyph,
                                tr("Delete channel"));

    connect(m_addButton, &QToolButton::clicked, this, &ChannelsPanel::addChannel);
    connect(m_deleteButton, &QToolButton::clicked, this, &ChannelsPanel::deleteChannel);

    root->addWidget(footer);
}

void ChannelsPanel::toggleVisibility(int row)
{
    if (row < 0 || row >= m_list->count())
        return;

    auto *item = m_list->item(row);
    const bool wasVisible = item->data(kVisibleRole).toBool();

    if (row == 0) {
        // Composite channel: toggle all channels at once
        const bool newVis = !wasVisible;
        for (int i = 0; i < m_list->count(); ++i)
            m_list->item(i)->setData(kVisibleRole, newVis);
    } else {
        item->setData(kVisibleRole, !wasVisible);
        // Update composite eye: it's on when all component channels are visible
        bool allVisible = true;
        for (int i = 1; i < m_builtinCount; ++i) {
            if (!m_list->item(i)->data(kVisibleRole).toBool()) {
                allVisible = false;
                break;
            }
        }
        m_list->item(0)->setData(kVisibleRole, allVisible);
    }

    m_list->viewport()->update();
    updateMask();
}

void ChannelsPanel::updateMask()
{
    if (!m_engine)
        return;

    const int mode = m_engine->colorMode();
    uint8_t mask = 0xFF;

    if (mode == 4 && m_builtinCount >= 4) {
        // RGB: bit 0=R, 1=G, 2=B
        mask = 0;
        if (m_list->item(1)->data(kVisibleRole).toBool()) mask |= 0x01;
        if (m_list->item(2)->data(kVisibleRole).toBool()) mask |= 0x02;
        if (m_list->item(3)->data(kVisibleRole).toBool()) mask |= 0x04;
    } else if (mode == 5 && m_builtinCount >= 5) {
        // CMYK: bit 0=C, 1=M, 2=Y, 3=K
        mask = 0;
        if (m_list->item(1)->data(kVisibleRole).toBool()) mask |= 0x01;
        if (m_list->item(2)->data(kVisibleRole).toBool()) mask |= 0x02;
        if (m_list->item(3)->data(kVisibleRole).toBool()) mask |= 0x04;
        if (m_list->item(4)->data(kVisibleRole).toBool()) mask |= 0x08;
    } else if (mode == 6 && m_builtinCount >= 4) {
        // Lab: bit 0=L, 1=a, 2=b
        mask = 0;
        if (m_list->item(1)->data(kVisibleRole).toBool()) mask |= 0x01;
        if (m_list->item(2)->data(kVisibleRole).toBool()) mask |= 0x02;
        if (m_list->item(3)->data(kVisibleRole).toBool()) mask |= 0x04;
    } else if (m_builtinCount == 1) {
        // Grayscale / Bitmap / etc: one channel
        if (!m_list->item(0)->data(kVisibleRole).toBool())
            mask = 0;
    }

    emit channelMaskChanged(mask);
}

void ChannelsPanel::addChannel()
{
    bool ok = false;
    QString name = QInputDialog::getText(this, tr("New Channel"),
                                         tr("Name:"), QLineEdit::Normal,
                                         tr("Alpha 1"), &ok);
    if (!ok || name.isEmpty())
        return;

    auto *item = new QListWidgetItem;
    item->setText(name);
    item->setData(kVisibleRole, true);
    item->setData(kShortcutRole, QString());
    item->setData(kIsAlphaRole, true);

    QPixmap thumb(kThumbSize, kThumbSize);
    thumb.fill(Qt::black);
    item->setData(kThumbRole, thumb);
    item->setSizeHint(QSize(0, kRowHeight));
    item->setFlags(item->flags() | Qt::ItemIsEditable);
    m_list->addItem(item);
}

void ChannelsPanel::deleteChannel()
{
    const int row = m_list->currentRow();
    if (row < 0 || row >= m_list->count())
        return;

    // Don't delete built-in channels
    if (row < m_builtinCount)
        return;

    delete m_list->takeItem(row);
}

void ChannelsPanel::refresh()
{
    m_list->clear();

    if (!m_engine)
        return;

    const int mode = m_engine->colorMode();
    const auto entries = channelsForMode(mode);
    m_builtinCount = entries.size();

    QImage composite = m_engine->compositeImage();

    for (int i = 0; i < entries.size(); ++i) {
        const auto &entry = entries[i];
        auto *item = new QListWidgetItem;
        item->setText(entry.name);
        item->setData(kVisibleRole, true);
        item->setData(kShortcutRole, entry.shortcut);
        item->setData(kIsAlphaRole, false);
        item->setData(kThumbRole, channelThumb(composite, i, mode, kThumbSize));
        item->setSizeHint(QSize(0, kRowHeight));
        item->setFlags(item->flags() & ~Qt::ItemIsEditable);
        m_list->addItem(item);
    }

    if (m_list->count() > 0)
        m_list->setCurrentRow(0);

    // Reset mask to all-visible on mode change
    emit channelMaskChanged(0xFF);
}
