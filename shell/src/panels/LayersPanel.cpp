#include "LayersPanel.h"

#include "LayerIcons.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QHBoxLayout>
#include <QMenu>
#include <QMessageBox>
#include <QMouseEvent>
#include <QPainter>
#include <QSlider>
#include <QStyledItemDelegate>
#include <QVBoxLayout>
#include <QWidgetAction>

namespace {

/// Thumbnail edge length in the layer row, in pixels.
constexpr int kThumbSize = 32;
/// Row height. CS6 uses 40px at the default thumbnail size.
constexpr int kRowHeight = 40;
/// Width of the eye column at the left of a row, up to CS6's divider line.
constexpr int kEyeColumn = 24;
/// The eye itself, centred in that column.
constexpr int kEyeSize = 15;
/// The padlock badge at the right of a row.
constexpr int kBadgeSize = 13;
/// Glyph size in the Lock row and the footer.
constexpr int kButtonGlyph = 16;
constexpr int kFooterGlyph = 18;

/// Panel colours, taken from the CS6 dark theme so the rows the delegate paints
/// sit in the same palette as the QSS-styled widgets around them.
const QColor kRowText(0xd4, 0xd4, 0xd4);
const QColor kGlyph(0xcf, 0xcf, 0xcf);
const QColor kGlyphOff(0x8a, 0x8a, 0x8a);
const QColor kDivider(0x2a, 0x2a, 0x2a);
/// CS6's selected row: a desaturated slate blue, not the vivid accent the rest
/// of the theme uses for selection.
const QColor kRowSelected(0x4a, 0x63, 0x83);
const QColor kRowHover(0x46, 0x46, 0x46);
/// The filter switch glows red when filtering is on, as CS6's does.
const QColor kSwitchOn(0xd0, 0x50, 0x4a);

/// Roles carrying per-row state on the list items.
constexpr int kVisibleRole = Qt::UserRole + 1;
/// 0 = unlocked, 1 = partly locked, 2 = Lock All.
constexpr int kLockRole = Qt::UserRole + 2;
/// Composed thumbnail, drawn by the delegate rather than by Qt's decoration.
constexpr int kThumbRole = Qt::UserRole + 3;
/// True for the Background layer, whose name CS6 sets in italics.
constexpr int kBackgroundRole = Qt::UserRole + 4;
constexpr int kClippingRole = Qt::UserRole + 5;

/// Where the name starts: past the eye column and the thumbnail.
int nameLeft()
{
    return kEyeColumn + 8 + kThumbSize + 8;
}

/// Paints a row the way CS6 does: eye column, divider, bordered thumbnail,
/// name, and a padlock badge when the layer is locked.
class LayerRowDelegate : public QStyledItemDelegate
{
public:
    using QStyledItemDelegate::QStyledItemDelegate;

    void paint(QPainter *painter, const QStyleOptionViewItem &option,
               const QModelIndex &index) const override
    {
        painter->save();
        painter->setRenderHint(QPainter::Antialiasing, true);

        const QRect row = option.rect;
        if (option.state & QStyle::State_Selected) {
            painter->fillRect(row, kRowSelected);
        } else if (option.state & QStyle::State_MouseOver) {
            painter->fillRect(row, kRowHover);
        }

        // The divider between the eye column and the thumbnail, which is what
        // makes the eye read as its own column rather than an icon.
        painter->setPen(kDivider);
        const int divider = row.left() + kEyeColumn;
        painter->drawLine(divider, row.top(), divider, row.bottom());

        if (index.data(kVisibleRole).toBool()) {
            const QPixmap eye = LayerIcons::pixmap(LayerIcons::Glyph::Eye, kGlyph, kEyeSize);
            painter->drawPixmap(row.left() + (kEyeColumn - kEyeSize) / 2,
                                row.top() + (row.height() - kEyeSize) / 2, eye);
        }

        // The thumbnail is only as big as the layer's own aspect ratio allows,
        // and is centred in the 32px box. Photoshop shows the checkerboard
        // *inside* that shape, not across the whole box — the padding is panel,
        // not transparency.
        const QPixmap pixmap = index.data(kThumbRole).value<QPixmap>();
        if (!pixmap.isNull()) {
            const QSize size = pixmap.deviceIndependentSize().toSize();
            const QRect thumb(row.left() + kEyeColumn + 8 + (kThumbSize - size.width()) / 2,
                              row.top() + (row.height() - size.height()) / 2, size.width(),
                              size.height());
            painter->drawPixmap(thumb, pixmap);
            // CS6 rules a hairline around the thumbnail; without it a white
            // layer has no edge against the row.
            painter->setPen(kDivider);
            painter->drawRect(thumb.adjusted(0, 0, -1, -1));
        }

        int textRight = row.right() - 6;
        const int lock = index.data(kLockRole).toInt();
        if (lock > 0) {
            const auto glyph = lock == 2 ? LayerIcons::Glyph::LockSolid
                                         : LayerIcons::Glyph::LockOutline;
            const QPixmap badge = LayerIcons::pixmap(glyph, kGlyph, kBadgeSize);
            const int x = row.right() - kBadgeSize - 7;
            painter->drawPixmap(x, row.top() + (row.height() - kBadgeSize) / 2, badge);
            textRight = x - 4;
        }

        QFont font = option.font;
        // Photoshop italicises the Background layer's name, which is how you
        // tell at a glance that it is not an ordinary layer.
        font.setItalic(index.data(kBackgroundRole).toBool());
        painter->setFont(font);
        painter->setPen(kRowText);

        const int left = row.left() + nameLeft() + (index.data(kClippingRole).toBool() ? 10 : 0);
        const QRect textRect(left, row.top(), textRight - left, row.height());
        const QString name = painter->fontMetrics().elidedText(
            index.data(Qt::DisplayRole).toString(), Qt::ElideRight, textRect.width());
        painter->drawText(textRect, Qt::AlignVCenter | Qt::AlignLeft, name);

        painter->restore();
    }

    QSize sizeHint(const QStyleOptionViewItem &option,
                   const QModelIndex &index) const override
    {
        Q_UNUSED(option);
        Q_UNUSED(index);
        return QSize(0, kRowHeight);
    }

    /// Renaming edits the name where the name is drawn, not across the whole
    /// row — otherwise the editor covers the thumbnail and the eye.
    void updateEditorGeometry(QWidget *editor, const QStyleOptionViewItem &option,
                              const QModelIndex &index) const override
    {
        Q_UNUSED(index);
        QRect rect = option.rect;
        rect.setLeft(rect.left() + nameLeft());
        rect.setHeight(20);
        rect.moveTop(option.rect.top() + (option.rect.height() - 20) / 2);
        editor->setGeometry(rect);
    }
};

/// A slider that drops out of a spin box's arrow, the way CS6's Opacity and
/// Fill fields work. Returned as a menu so it closes on click-away for free.
QMenu *sliderPopup(QSpinBox *field)
{
    auto *menu = new QMenu(field);
    auto *slider = new QSlider(Qt::Horizontal, menu);
    slider->setRange(field->minimum(), field->maximum());
    slider->setFixedWidth(120);
    auto *action = new QWidgetAction(menu);
    action->setDefaultWidget(slider);
    menu->addAction(action);

    QObject::connect(menu, &QMenu::aboutToShow, slider,
                     [slider, field] { slider->setValue(field->value()); });
    QObject::connect(slider, &QSlider::valueChanged, field, &QSpinBox::setValue);
    return menu;
}

} // namespace

/// The layer list.
///
/// Only two behaviours are added to `QListWidget`: a click in the eye column
/// toggles visibility without moving the selection — as it does in CS6, where
/// hiding a layer is not the same as choosing it — and rows can be dragged to
/// reorder.
class LayerListWidget : public QListWidget
{
public:
    using QListWidget::QListWidget;

    /// Called with the row whose eye was clicked.
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

LayersPanel::LayersPanel(Engine *engine, QWidget *parent)
    : QWidget(parent)
    , m_engine(engine)
{
    buildUi();
    populateBlendModes();
    refresh();
}

void LayersPanel::buildFilterRow(QWidget *parent, QBoxLayout *into)
{
    auto *row = new QHBoxLayout();
    row->setSpacing(3);

    auto *search = new QLabel(parent);
    search->setPixmap(LayerIcons::pixmap(LayerIcons::Glyph::Search, kGlyph, kButtonGlyph));
    row->addWidget(search);

    // CS6 offers six things to filter on. Only Kind is implemented — the others
    // need layer effects, names to search and colour labels — so they are listed
    // for the panel's shape and disabled rather than quietly doing nothing.
    m_filterKind = new QComboBox(parent);
    m_filterKind->addItem(tr("Kind"));
    const QStringList unimplemented = {tr("Name"), tr("Effect"), tr("Mode"), tr("Attribute"),
                                       tr("Color")};
    for (const QString &name : unimplemented) {
        m_filterKind->addItem(name);
        const int at = m_filterKind->count() - 1;
        m_filterKind->setItemData(at, false, Qt::UserRole - 1); // disables the row
        m_filterKind->setItemData(at, tr("Not implemented yet"), Qt::ToolTipRole);
    }
    m_filterKind->setFixedWidth(76);
    row->addWidget(m_filterKind);

    struct Kind {
        LayerIcons::Glyph glyph;
        QString tip;
        bool implemented;
    };
    const Kind kinds[] = {
        {LayerIcons::Glyph::KindPixel, tr("Show pixel layers"), true},
        {LayerIcons::Glyph::KindAdjustment, tr("Show adjustment layers"), true},
        {LayerIcons::Glyph::KindType, tr("Show type layers"), false},
        {LayerIcons::Glyph::KindShape, tr("Show shape layers"), false},
        {LayerIcons::Glyph::KindSmartObject, tr("Show smart objects"), false},
    };
    for (const Kind &kind : kinds) {
        auto *button = new QToolButton(parent);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setIconSize(QSize(kButtonGlyph, kButtonGlyph));
        button->setIcon(LayerIcons::icon(kind.glyph, kind.implemented ? kGlyph : kGlyphOff,
                                         kButtonGlyph));
        button->setEnabled(kind.implemented);
        button->setToolTip(kind.implemented
                               ? kind.tip
                               : tr("Not implemented yet: there are no such layers"));
        row->addWidget(button);
        m_kindButtons.append(button);
        connect(button, &QToolButton::toggled, this, [this] { applyFilter(); });
    }

    row->addStretch(1);

    m_filterSwitch = new QToolButton(parent);
    m_filterSwitch->setCheckable(true);
    m_filterSwitch->setAutoRaise(true);
    m_filterSwitch->setIconSize(QSize(kButtonGlyph, kButtonGlyph));
    m_filterSwitch->setIcon(
        LayerIcons::icon(LayerIcons::Glyph::FilterSwitch, kGlyph, kButtonGlyph));
    m_filterSwitch->setToolTip(tr("Turn layer filtering on or off"));
    row->addWidget(m_filterSwitch);
    connect(m_filterSwitch, &QToolButton::toggled, this, [this](bool on) {
        // Red while filtering, exactly as CS6 lights this switch up.
        m_filterSwitch->setIcon(LayerIcons::icon(LayerIcons::Glyph::FilterSwitch,
                                                 on ? kSwitchOn : kGlyph, kButtonGlyph));
        applyFilter();
    });

    into->addLayout(row);
}

void LayersPanel::buildLockRow(QWidget *parent, QBoxLayout *into)
{
    auto *row = new QHBoxLayout();
    row->setSpacing(3);
    row->addWidget(new QLabel(tr("Lock:"), parent));

    struct Entry {
        LayerIcons::Glyph glyph;
        QString tip;
        QToolButton **slot;
    };
    const Entry entries[] = {
        {LayerIcons::Glyph::LockTransparency,
         tr("Lock transparent pixels: painting may recolour what is there but cannot "
            "give an empty pixel any coverage"),
         &m_lockTransparency},
        {LayerIcons::Glyph::LockImage,
         tr("Lock image pixels: no tool may edit this layer's pixels"), &m_lockImage},
        {LayerIcons::Glyph::LockPosition, tr("Lock position: the layer cannot be moved"),
         &m_lockPosition},
        {LayerIcons::Glyph::LockAll,
         tr("Lock all: the layer cannot be painted on, moved, deleted or merged"),
         &m_lockAll},
    };
    for (const Entry &entry : entries) {
        auto *button = new QToolButton(parent);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setIconSize(QSize(kButtonGlyph, kButtonGlyph));
        button->setIcon(LayerIcons::icon(entry.glyph, kGlyph, kButtonGlyph));
        button->setToolTip(entry.tip);
        button->setStatusTip(entry.tip);
        row->addWidget(button);
        *entry.slot = button;
    }

    // Lock All is a shorthand for the other three, so it sets them rather than
    // being a fourth flag the engine has to know about.
    connect(m_lockAll, &QToolButton::clicked, this, [this](bool on) {
        if (m_updating) {
            return;
        }
        const QSignalBlocker b1(m_lockTransparency);
        const QSignalBlocker b2(m_lockImage);
        const QSignalBlocker b3(m_lockPosition);
        m_lockTransparency->setChecked(on);
        m_lockImage->setChecked(on);
        m_lockPosition->setChecked(on);
        applyLocks();
    });
    for (QToolButton *button : {m_lockTransparency, m_lockImage, m_lockPosition}) {
        connect(button, &QToolButton::clicked, this, [this] { applyLocks(); });
    }

    row->addStretch(1);
    row->addWidget(new QLabel(tr("Fill:"), parent));
    m_fillOpacity = new QSpinBox(parent);
    m_fillOpacity->setRange(0, 100);
    m_fillOpacity->setValue(100);
    m_fillOpacity->setSuffix(QStringLiteral("%"));
    m_fillOpacity->setFixedWidth(58);
    // CS6 shows the number alone; the arrow beside it opens the slider, so the
    // stock spin arrows would be a second control for the same value.
    m_fillOpacity->setButtonSymbols(QAbstractSpinBox::NoButtons);
    row->addWidget(m_fillOpacity);

    auto *arrow = new QToolButton(parent);
    arrow->setAutoRaise(true);
    arrow->setArrowType(Qt::DownArrow);
    arrow->setPopupMode(QToolButton::InstantPopup);
    arrow->setMenu(sliderPopup(m_fillOpacity));
    arrow->setToolTip(tr("Fill opacity slider"));
    row->addWidget(arrow);

    into->addLayout(row);
}

void LayersPanel::buildUi()
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);

    // -- header: filter row, blend mode + opacity, lock row + fill ------------
    auto *header = new QWidget(this);
    header->setObjectName(QStringLiteral("panelHeader"));
    auto *headerLayout = new QVBoxLayout(header);
    headerLayout->setContentsMargins(5, 4, 5, 4);
    headerLayout->setSpacing(3);

    buildFilterRow(header, headerLayout);

    auto *modeRow = new QHBoxLayout();
    modeRow->setSpacing(4);
    m_blendMode = new QComboBox(header);
    m_blendMode->setToolTip(tr("Blending mode"));
    modeRow->addWidget(m_blendMode, 1);

    modeRow->addWidget(new QLabel(tr("Opacity:"), header));
    m_opacity = new QSpinBox(header);
    m_opacity->setRange(0, 100);
    m_opacity->setValue(100);
    m_opacity->setSuffix(QStringLiteral("%"));
    m_opacity->setFixedWidth(58);
    m_opacity->setButtonSymbols(QAbstractSpinBox::NoButtons);
    modeRow->addWidget(m_opacity);

    auto *opacityArrow = new QToolButton(header);
    opacityArrow->setAutoRaise(true);
    opacityArrow->setArrowType(Qt::DownArrow);
    opacityArrow->setPopupMode(QToolButton::InstantPopup);
    opacityArrow->setMenu(sliderPopup(m_opacity));
    opacityArrow->setToolTip(tr("Opacity slider"));
    modeRow->addWidget(opacityArrow);
    headerLayout->addLayout(modeRow);

    buildLockRow(header, headerLayout);

    root->addWidget(header);

    // -- the layer list -----------------------------------------------------
    m_list = new LayerListWidget(this);
    m_list->setObjectName(QStringLiteral("layerList"));
    m_list->setItemDelegate(new LayerRowDelegate(m_list));
    m_list->setMouseTracking(true);
    m_list->setSelectionMode(QAbstractItemView::SingleSelection);
    // Photoshop reorders layers by dragging rows.
    m_list->setDragDropMode(QAbstractItemView::InternalMove);
    m_list->setDefaultDropAction(Qt::MoveAction);
    m_list->setUniformItemSizes(true);
    m_list->setContextMenuPolicy(Qt::CustomContextMenu);
    m_list->onEyeClicked = [this](int row) { toggleVisibility(row); };
    root->addWidget(m_list, 1);

    // -- footer buttons -----------------------------------------------------
    auto *footer = new QWidget(this);
    footer->setObjectName(QStringLiteral("panelFooter"));
    auto *footerLayout = new QHBoxLayout(footer);
    footerLayout->setContentsMargins(4, 2, 4, 2);
    footerLayout->setSpacing(2);

    auto makeButton = [&](LayerIcons::Glyph glyph, const QString &tip, bool implemented) {
        auto *b = new QToolButton(footer);
        b->setIconSize(QSize(kFooterGlyph, kFooterGlyph));
        b->setIcon(LayerIcons::icon(glyph, implemented ? kGlyph : kGlyphOff, kFooterGlyph));
        b->setToolTip(implemented ? tip : tr("Not implemented yet"));
        b->setEnabled(implemented);
        b->setAutoRaise(true);
        footerLayout->addWidget(b);
        return b;
    };

    // CS6's order, left to right. Linking, layer effects and groups are not
    // implemented; they are shown disabled so the footer keeps its shape rather
    // than silently losing four of its seven glyphs.
    m_linkButton = makeButton(LayerIcons::Glyph::Link, tr("Link layers"), false);
    m_effectsButton = makeButton(LayerIcons::Glyph::Effects, tr("Add a layer style"), false);
    m_maskButton = makeButton(LayerIcons::Glyph::Mask, tr("Add layer mask"), true);
    m_adjustmentButton =
        makeButton(LayerIcons::Glyph::Adjustment, tr("New adjustment layer"), true);
    m_groupButton = makeButton(LayerIcons::Glyph::Group, tr("New group"), false);
    m_addButton = makeButton(LayerIcons::Glyph::NewLayer, tr("Create a new layer"), true);
    m_deleteButton = makeButton(LayerIcons::Glyph::Delete, tr("Delete layer"), true);

    root->addWidget(footer);

    // -- wiring -------------------------------------------------------------
    connect(m_list, &QListWidget::itemSelectionChanged,
            this, &LayersPanel::onSelectionChanged);
    connect(m_list, &QListWidget::itemChanged, this, &LayersPanel::onItemChanged);
    connect(m_list, &QListWidget::customContextMenuRequested,
            this, &LayersPanel::onRowContextMenu);
    connect(m_list->model(), &QAbstractItemModel::rowsMoved,
            this, &LayersPanel::onRowsMoved);

    connect(m_blendMode, &QComboBox::currentIndexChanged,
            this, &LayersPanel::onBlendModeChanged);
    connect(m_opacity, &QSpinBox::valueChanged, this, &LayersPanel::onOpacityChanged);
    connect(m_fillOpacity, &QSpinBox::valueChanged,
            this, &LayersPanel::onFillOpacityChanged);

    connect(m_addButton, &QToolButton::clicked, this, &LayersPanel::addLayer);
    connect(m_deleteButton, &QToolButton::clicked, this, &LayersPanel::deleteLayer);
    connect(m_maskButton, &QToolButton::clicked, this, &LayersPanel::addMask);
    connect(m_adjustmentButton, &QToolButton::clicked,
            this, &LayersPanel::addAdjustmentLayer);
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

bool LayersPanel::passesFilter(int index) const
{
    if (!m_filterSwitch->isChecked()) {
        return true;
    }
    // Nothing selected filters nothing, which is how CS6 behaves with the
    // switch on and no kind chosen.
    bool anyChecked = false;
    for (QToolButton *button : m_kindButtons) {
        anyChecked = anyChecked || button->isChecked();
    }
    if (!anyChecked) {
        return true;
    }
    // The buttons are in engine `layerKind` order: pixel, adjustment, then the
    // three kinds we have no layers of.
    const int kind = m_engine ? m_engine->layerKind(index) : 0;
    return kind < m_kindButtons.size() && m_kindButtons.at(kind)->isChecked();
}

void LayersPanel::applyFilter()
{
    for (int row = 0; row < m_list->count(); ++row) {
        m_list->setRowHidden(row, !passesFilter(row));
    }
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
        const QString name = m_engine->layerName(i);
        auto *item = new QListWidgetItem(name);
        item->setSizeHint(QSize(0, kRowHeight));
        item->setFlags(item->flags() | Qt::ItemIsEditable);

        item->setData(kVisibleRole, m_engine->layerVisible(i));
        item->setData(kBackgroundRole, name == QLatin1String("Background"));
        item->setData(kClippingRole, m_engine->layerIsClipping(i));
        item->setData(kLockRole, m_engine->layerIsFullyLocked(i)
                          ? 2
                          : (m_engine->layerIsLocked(i) ? 1 : 0));

        // Compose the thumbnail over a checkerboard so transparent layers read
        // as transparent rather than black. The engine preserves aspect ratio,
        // so this is the shape of the layer, not a 32px square — the delegate
        // centres it.
        const QImage thumb = m_engine->layerThumbnail(i, kThumbSize);
        const bool adjustment = m_engine->layerKind(i) == 1;
        const QSize size = adjustment || thumb.isNull() ? QSize(kThumbSize, kThumbSize)
                                                        : thumb.size();
        QPixmap canvas(size);
        canvas.fill(Qt::transparent);
        {
            QPainter p(&canvas);
            if (adjustment) {
                // An adjustment layer has no pixels of its own, so CS6 shows the
                // adjustment's own glyph on white instead of a thumbnail.
                p.fillRect(canvas.rect(), Qt::white);
                const QPixmap glyph = LayerIcons::pixmap(LayerIcons::Glyph::Adjustment,
                                                         QColor(0x22, 0x22, 0x22), 24);
                p.drawPixmap((size.width() - 24) / 2, (size.height() - 24) / 2, glyph);
            } else {
                for (int y = 0; y < size.height(); y += 8) {
                    for (int x = 0; x < size.width(); x += 8) {
                        const bool lightSquare = ((x / 8) + (y / 8)) % 2 == 0;
                        p.fillRect(x, y, 8, 8,
                                   lightSquare ? QColor(0xcc, 0xcc, 0xcc)
                                               : QColor(0x99, 0x99, 0x99));
                    }
                }
                if (!thumb.isNull()) {
                    p.drawImage(0, 0, thumb);
                }
            }
        }
        item->setData(kThumbRole, canvas);

        QStringList notes;
        if (m_engine->layerIsClipping(i)) {
            notes << tr("clipped");
        }
        if (m_engine->layerHasMask(i)) {
            notes << tr("masked");
        }
        if (m_engine->layerIsFullyLocked(i)) {
            notes << tr("locked");
        } else if (m_engine->layerIsLocked(i)) {
            QStringList locks;
            if (m_engine->layerLockTransparency(i)) {
                locks << tr("transparency");
            }
            if (m_engine->layerLockPixels(i)) {
                locks << tr("pixels");
            }
            if (m_engine->layerLockPosition(i)) {
                locks << tr("position");
            }
            notes << tr("locked: %1").arg(locks.join(QStringLiteral(", ")));
        }
        item->setToolTip(notes.isEmpty() ? name
                                         : tr("%1 (%2)").arg(name, notes.join(
                                               QStringLiteral(", "))));

        m_list->addItem(item);
    }

    if (active >= 0 && active < count) {
        m_list->setCurrentRow(active);

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

        m_opacity->setValue(m_engine->layerOpacity(active));
        m_fillOpacity->setValue(m_engine->layerFillOpacity(active));
    }

    syncLockRow();
    applyFilter();

    // Deleting the last layer is refused by the engine; grey the button out
    // rather than letting the user click a no-op.
    m_deleteButton->setEnabled(count > 1);

    m_updating = false;
}

void LayersPanel::syncLockRow()
{
    const int active = currentIndex();
    const bool valid = m_engine && active >= 0;
    const bool transparency = valid && m_engine->layerLockTransparency(active);
    const bool pixels = valid && m_engine->layerLockPixels(active);
    const bool position = valid && m_engine->layerLockPosition(active);

    const QSignalBlocker b1(m_lockTransparency);
    const QSignalBlocker b2(m_lockImage);
    const QSignalBlocker b3(m_lockPosition);
    const QSignalBlocker b4(m_lockAll);
    m_lockTransparency->setChecked(transparency);
    m_lockImage->setChecked(pixels);
    m_lockPosition->setChecked(position);
    m_lockAll->setChecked(transparency && pixels && position);

    for (QToolButton *button : {m_lockTransparency, m_lockImage, m_lockPosition, m_lockAll}) {
        button->setEnabled(valid);
    }
}

void LayersPanel::applyLocks()
{
    if (m_updating || !m_engine) {
        return;
    }
    const int index = currentIndex();
    if (index < 0) {
        return;
    }
    m_engine->setLayerLocks(index, m_lockTransparency->isChecked(),
                            m_lockImage->isChecked(), m_lockPosition->isChecked());
    // The row grows or loses its padlock, and Lock All follows the other three.
    refresh();
}

void LayersPanel::toggleVisibility(int index)
{
    if (!m_engine || index < 0 || index >= m_list->count()) {
        return;
    }
    QListWidgetItem *item = m_list->item(index);
    const bool wantVisible = !item->data(kVisibleRole).toBool();
    item->setData(kVisibleRole, wantVisible);
    m_engine->setLayerVisible(index, wantVisible);
    emit documentChanged();
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
    const QString name = item->text();
    if (!name.isEmpty() && name != m_engine->layerName(index)) {
        m_engine->setLayerName(index, name);
    }
}

void LayersPanel::onRowContextMenu(const QPoint &pos)
{
    const QModelIndex index = m_list->indexAt(pos);
    if (!index.isValid() || !m_engine) {
        return;
    }
    if (index.row() != currentIndex()) {
        m_list->setCurrentRow(index.row());
    }

    // CS6's row menu, cut down to what the engine can do. Duplicate and Merge
    // Down live here rather than in the footer, which is where Photoshop keeps
    // them too.
    QMenu menu(this);
    QAction *duplicate = menu.addAction(tr("Duplicate Layer"));
    QAction *remove = menu.addAction(tr("Delete Layer"));
    menu.addSeparator();
    QAction *mask = menu.addAction(tr("Add Layer Mask"));
    QAction *clip = menu.addAction(m_engine->layerIsClipping(index.row())
                                      ? tr("Release Clipping Mask")
                                      : tr("Create Clipping Mask"));
    menu.addSeparator();
    QAction *merge = menu.addAction(tr("Merge Down"));

    remove->setEnabled(m_list->count() > 1);
    merge->setEnabled(index.row() < m_list->count() - 1);
    // The bottom layer has nothing to clip to.
    clip->setEnabled(index.row() < m_list->count() - 1);

    QAction *chosen = menu.exec(m_list->viewport()->mapToGlobal(pos));
    if (!chosen) {
        return;
    }
    if (chosen == duplicate) {
        duplicateLayer();
    } else if (chosen == remove) {
        deleteLayer();
    } else if (chosen == mask) {
        addMask();
    } else if (chosen == merge) {
        mergeDown();
    } else if (chosen == clip) {
        m_engine->setLayerClipping(index.row(), !m_engine->layerIsClipping(index.row()));
        emit documentChanged();
        refresh();
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

void LayersPanel::warnLocked(const QString &action)
{
    // Photoshop's own wording for a refused edit, and its error icon.
    QMessageBox::critical(this, tr("PhotoRust"),
                          tr("Could not %1 because the layer is locked.").arg(action));
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
    if (index < 0) {
        return;
    }
    if (m_engine->layerIsFullyLocked(index)) {
        warnLocked(tr("delete the layer"));
        return;
    }
    m_engine->deleteLayer(index);
    emit documentChanged();
    refresh();
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

void LayersPanel::addAdjustmentLayer()
{
    if (!m_engine) {
        return;
    }
    // CS6 opens a menu of the adjustment kinds here. These are the ones the
    // engine implements as layer adjustments.
    QMenu menu(this);
    // Names, not translations: the string is the contract with the engine's
    // `Adjustment::default_for`, the same way the Image menu passes them.
    for (const char *kind : {"Brightness/Contrast", "Levels", "Exposure", "Hue/Saturation",
                             "Color Balance", "Black & White", "Invert", "Posterize",
                             "Threshold"}) {
        menu.addAction(QString::fromUtf8(kind));
    }
    QAction *chosen = menu.exec(m_adjustmentButton->mapToGlobal(
        QPoint(0, -menu.sizeHint().height())));
    if (!chosen) {
        return;
    }
    m_engine->addAdjustmentLayer(chosen->text());
    emit documentChanged();
    refresh();
}

void LayersPanel::mergeDown()
{
    if (!m_engine) {
        return;
    }
    const int index = currentIndex();
    if (index < 0) {
        return;
    }
    // The lower layer is the one rewritten, so either being locked stops it.
    if (m_engine->layerIsFullyLocked(index)
        || (index + 1 < m_list->count() && m_engine->layerIsFullyLocked(index + 1))) {
        warnLocked(tr("merge the layers"));
        return;
    }
    m_engine->mergeLayerDown(index);
    emit documentChanged();
    refresh();
}
