#include "LayersPanel.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QHBoxLayout>
#include <QPainter>
#include <QVBoxLayout>

namespace {

/// Thumbnail edge length in the layer row, in pixels.
constexpr int kThumbSize = 32;
/// Row height. CS6 uses 40px at the default thumbnail size.
constexpr int kRowHeight = 40;

/// Roles carrying per-row state on the list items.
constexpr int kVisibleRole = Qt::UserRole + 1;

} // namespace

LayersPanel::LayersPanel(Engine *engine, QWidget *parent)
    : QWidget(parent)
    , m_engine(engine)
{
    buildUi();
    populateBlendModes();
    refresh();
}

void LayersPanel::buildUi()
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);

    // -- header: blend mode + opacity ---------------------------------------
    auto *header = new QWidget(this);
    header->setObjectName(QStringLiteral("panelHeader"));
    auto *headerLayout = new QVBoxLayout(header);
    headerLayout->setContentsMargins(5, 4, 5, 4);
    headerLayout->setSpacing(3);

    auto *modeRow = new QHBoxLayout();
    modeRow->setSpacing(4);
    m_blendMode = new QComboBox(header);
    m_blendMode->setToolTip(tr("Blending mode"));
    modeRow->addWidget(m_blendMode, 1);

    m_opacityLabel = new QLabel(tr("Opacity: 100%"), header);
    m_opacityLabel->setMinimumWidth(78);
    modeRow->addWidget(m_opacityLabel);
    headerLayout->addLayout(modeRow);

    m_opacity = new QSlider(Qt::Horizontal, header);
    m_opacity->setRange(0, 100);
    m_opacity->setValue(100);
    headerLayout->addWidget(m_opacity);

    auto *fillRow = new QHBoxLayout();
    fillRow->setSpacing(4);
    m_fillLabel = new QLabel(tr("Fill: 100%"), header);
    m_fillLabel->setMinimumWidth(78);
    fillRow->addWidget(m_fillLabel);
    m_fillOpacity = new QSlider(Qt::Horizontal, header);
    m_fillOpacity->setRange(0, 100);
    m_fillOpacity->setValue(100);
    fillRow->addWidget(m_fillOpacity, 1);
    headerLayout->addLayout(fillRow);

    root->addWidget(header);

    // -- the layer list -----------------------------------------------------
    m_list = new QListWidget(this);
    m_list->setObjectName(QStringLiteral("layerList"));
    m_list->setIconSize(QSize(kThumbSize, kThumbSize));
    m_list->setSelectionMode(QAbstractItemView::SingleSelection);
    // Photoshop reorders layers by dragging rows.
    m_list->setDragDropMode(QAbstractItemView::InternalMove);
    m_list->setDefaultDropAction(Qt::MoveAction);
    m_list->setUniformItemSizes(true);
    root->addWidget(m_list, 1);

    // -- footer buttons -----------------------------------------------------
    auto *footer = new QWidget(this);
    footer->setObjectName(QStringLiteral("panelFooter"));
    auto *footerLayout = new QHBoxLayout(footer);
    footerLayout->setContentsMargins(4, 2, 4, 2);
    footerLayout->setSpacing(2);

    auto makeButton = [&](const QString &glyph, const QString &tip) {
        auto *b = new QToolButton(footer);
        b->setText(glyph);
        b->setToolTip(tip);
        b->setAutoRaise(true);
        footerLayout->addWidget(b);
        return b;
    };

    m_maskButton = makeButton(QStringLiteral("▣"), tr("Add layer mask"));
    m_duplicateButton = makeButton(QStringLiteral("⧉"), tr("Duplicate layer"));
    m_mergeButton = makeButton(QStringLiteral("⤓"), tr("Merge down"));
    footerLayout->addStretch(1);
    m_addButton = makeButton(QStringLiteral("＋"), tr("Create a new layer"));
    m_deleteButton = makeButton(QStringLiteral("🗑"), tr("Delete layer"));

    root->addWidget(footer);

    // -- wiring -------------------------------------------------------------
    connect(m_list, &QListWidget::itemSelectionChanged,
            this, &LayersPanel::onSelectionChanged);
    connect(m_list, &QListWidget::itemChanged, this, &LayersPanel::onItemChanged);
    connect(m_list->model(), &QAbstractItemModel::rowsMoved,
            this, &LayersPanel::onRowsMoved);

    connect(m_blendMode, &QComboBox::currentIndexChanged,
            this, &LayersPanel::onBlendModeChanged);
    connect(m_opacity, &QSlider::valueChanged, this, &LayersPanel::onOpacityChanged);
    connect(m_fillOpacity, &QSlider::valueChanged,
            this, &LayersPanel::onFillOpacityChanged);

    connect(m_addButton, &QToolButton::clicked, this, &LayersPanel::addLayer);
    connect(m_deleteButton, &QToolButton::clicked, this, &LayersPanel::deleteLayer);
    connect(m_duplicateButton, &QToolButton::clicked, this, &LayersPanel::duplicateLayer);
    connect(m_maskButton, &QToolButton::clicked, this, &LayersPanel::addMask);
    connect(m_mergeButton, &QToolButton::clicked, this, &LayersPanel::mergeDown);
}

void LayersPanel::populateBlendModes()
{
    if (!m_engine) {
        return;
    }
    m_updating = true;

    // The engine owns the canonical list and its order, so the combo box can
    // never drift out of sync with the BlendMode discriminants.
    const QString joined = m_engine->blendModeNames();
    const QStringList names = joined.split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    m_blendMode->addItems(names);

    // Insert the group separators CS6 draws in this dropdown. Walking the
    // positions in reverse keeps the earlier indices valid as rows shift down.
    const QString separatorSpec = m_engine->blendModeSeparators();
    QList<int> positions;
    for (const QString &part : separatorSpec.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
        bool ok = false;
        const int at = part.toInt(&ok);
        if (ok) {
            positions.append(at);
        }
    }
    std::sort(positions.begin(), positions.end(), std::greater<int>());
    for (int at : positions) {
        if (at > 0 && at < m_blendMode->count()) {
            m_blendMode->insertSeparator(at);
        }
    }

    m_updating = false;
}

int LayersPanel::currentIndex() const
{
    return m_list->currentRow();
}

void LayersPanel::refresh()
{
    if (!m_engine) {
        return;
    }
    m_updating = true;

    const int count = m_engine->getLayerCount();
    const int active = m_engine->getActiveLayerIndex();

    m_list->clear();
    for (int i = 0; i < count; ++i) {
        auto *item = new QListWidgetItem(m_engine->layerName(i));
        item->setSizeHint(QSize(0, kRowHeight));
        item->setFlags(item->flags() | Qt::ItemIsEditable | Qt::ItemIsUserCheckable);

        const bool visible = m_engine->layerVisible(i);
        item->setCheckState(visible ? Qt::Checked : Qt::Unchecked);
        item->setData(kVisibleRole, visible);

        // Compose the thumbnail over a checkerboard so transparent layers read
        // as transparent rather than black.
        const QImage thumb = m_engine->layerThumbnail(i, kThumbSize);
        QPixmap canvas(kThumbSize, kThumbSize);
        canvas.fill(QColor(0x3c, 0x3c, 0x3c));
        {
            QPainter p(&canvas);
            for (int y = 0; y < kThumbSize; y += 8) {
                for (int x = 0; x < kThumbSize; x += 8) {
                    const bool lightSquare = ((x / 8) + (y / 8)) % 2 == 0;
                    p.fillRect(x, y, 8, 8,
                               lightSquare ? QColor(0xcc, 0xcc, 0xcc)
                                           : QColor(0x99, 0x99, 0x99));
                }
            }
            if (!thumb.isNull()) {
                // Centre it — the engine preserves aspect ratio, so a
                // non-square layer yields a non-square thumbnail.
                p.drawImage((kThumbSize - thumb.width()) / 2,
                            (kThumbSize - thumb.height()) / 2, thumb);
            }
        }
        item->setIcon(QIcon(canvas));

        QString tip = m_engine->layerName(i);
        if (m_engine->layerIsClipping(i)) {
            tip += tr(" (clipped)");
        }
        item->setToolTip(tip);

        m_list->addItem(item);
    }

    if (active >= 0 && active < count) {
        m_list->setCurrentRow(active);

        m_blendMode->setCurrentIndex(
            m_blendMode->findText(QString(), Qt::MatchExactly) == -1
                ? m_blendMode->currentIndex()
                : m_blendMode->currentIndex());
        // Blend mode discriminants are dense and in list order, but separators
        // occupy rows too, so map through the item text.
        const int modeValue = m_engine->layerBlendMode(active);
        int comboRow = 0;
        int seen = 0;
        for (int row = 0; row < m_blendMode->count(); ++row) {
            if (m_blendMode->itemText(row).isEmpty()) {
                continue; // separator
            }
            if (seen == modeValue) {
                comboRow = row;
                break;
            }
            ++seen;
        }
        m_blendMode->setCurrentIndex(comboRow);

        const int opacity = m_engine->layerOpacity(active);
        m_opacity->setValue(opacity);
        m_opacityLabel->setText(tr("Opacity: %1%").arg(opacity));

        const int fill = m_engine->layerFillOpacity(active);
        m_fillOpacity->setValue(fill);
        m_fillLabel->setText(tr("Fill: %1%").arg(fill));
    }

    // Deleting the last layer is refused by the engine; grey the button out
    // rather than letting the user click a no-op.
    m_deleteButton->setEnabled(count > 1);
    m_mergeButton->setEnabled(count > 1 && active < count - 1);

    m_updating = false;
}

void LayersPanel::onSelectionChanged()
{
    if (m_updating || !m_engine) {
        return;
    }
    const int index = currentIndex();
    if (index < 0) {
        return;
    }
    m_engine->setActiveLayer(index);
    refresh();
}

void LayersPanel::onBlendModeChanged(int index)
{
    if (m_updating || !m_engine || index < 0) {
        return;
    }
    // Translate the combo row back to a BlendMode discriminant by counting
    // only the non-separator rows above it.
    int value = 0;
    for (int row = 0; row < index; ++row) {
        if (!m_blendMode->itemText(row).isEmpty()) {
            ++value;
        }
    }
    const int layer = currentIndex();
    if (layer >= 0) {
        m_engine->setLayerBlendMode(layer, value);
        emit documentChanged();
    }
}

void LayersPanel::onOpacityChanged(int value)
{
    m_opacityLabel->setText(tr("Opacity: %1%").arg(value));
    if (m_updating || !m_engine) {
        return;
    }
    const int layer = currentIndex();
    if (layer >= 0) {
        m_engine->setLayerOpacity(layer, value);
        emit documentChanged();
    }
}

void LayersPanel::onFillOpacityChanged(int value)
{
    m_fillLabel->setText(tr("Fill: %1%").arg(value));
    if (m_updating || !m_engine) {
        return;
    }
    const int layer = currentIndex();
    if (layer >= 0) {
        m_engine->setLayerFillOpacity(layer, value);
        emit documentChanged();
    }
}

void LayersPanel::onItemChanged(QListWidgetItem *item)
{
    if (m_updating || !m_engine || !item) {
        return;
    }
    const int index = m_list->row(item);
    if (index < 0) {
        return;
    }

    // One handler serves both the checkbox and the inline rename, so work out
    // which actually changed.
    const bool wantVisible = item->checkState() == Qt::Checked;
    if (wantVisible != item->data(kVisibleRole).toBool()) {
        item->setData(kVisibleRole, wantVisible);
        m_engine->setLayerVisible(index, wantVisible);
        emit documentChanged();
        return;
    }

    const QString name = item->text();
    if (!name.isEmpty() && name != m_engine->layerName(index)) {
        m_engine->setLayerName(index, name);
    }
}

void LayersPanel::onRowsMoved()
{
    if (m_updating || !m_engine) {
        return;
    }
    // Qt has already reordered its own model; replay that onto the engine by
    // finding the row whose name no longer matches, then resync from the
    // engine so both sides agree.
    const int count = m_list->count();
    for (int row = 0; row < count; ++row) {
        if (m_list->item(row)->text() != m_engine->layerName(row)) {
            // The dragged item is the first mismatch; find where it came from.
            const QString moved = m_list->item(row)->text();
            for (int from = 0; from < count; ++from) {
                if (m_engine->layerName(from) == moved) {
                    m_engine->moveLayer(from, row);
                    emit documentChanged();
                    refresh();
                    return;
                }
            }
            break;
        }
    }
    refresh();
}

void LayersPanel::addLayer()
{
    if (!m_engine) {
        return;
    }
    m_engine->addLayer();
    emit documentChanged();
    refresh();
}

void LayersPanel::deleteLayer()
{
    if (!m_engine) {
        return;
    }
    const int index = currentIndex();
    if (index >= 0) {
        m_engine->deleteLayer(index);
        emit documentChanged();
        refresh();
    }
}

void LayersPanel::duplicateLayer()
{
    if (!m_engine) {
        return;
    }
    const int index = currentIndex();
    if (index >= 0) {
        m_engine->duplicateLayer(index);
        emit documentChanged();
        refresh();
    }
}

void LayersPanel::addMask()
{
    if (!m_engine) {
        return;
    }
    const int index = currentIndex();
    if (index >= 0) {
        m_engine->addLayerMask(index, /*revealAll=*/true);
        emit documentChanged();
        refresh();
    }
}

void LayersPanel::mergeDown()
{
    if (!m_engine) {
        return;
    }
    const int index = currentIndex();
    if (index >= 0) {
        m_engine->mergeLayerDown(index);
        emit documentChanged();
        refresh();
    }
}
