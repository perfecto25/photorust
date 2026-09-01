#include "LayersPanel.h"

#include "LayerIcons.h"

#include <algorithm>

#include "photorust_core/src/bridge.cxxqt.h"

#include <QHBoxLayout>
#include <QMenu>
#include <QMessageBox>
#include <QMouseEvent>
#include <QPainter>
#include <QSlider>
#include <QIcon>
#include <QPainterPath>
#include <QPixmap>
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
/// The plate behind a locked row's padlock.
const QColor kLockPlate(0x1e, 0x1e, 0x1e);

/// Roles carrying per-row state on the list items.
constexpr int kVisibleRole = Qt::UserRole + 1;
/// 0 = unlocked, 1 = partly locked, 2 = Lock All.
constexpr int kLockRole = Qt::UserRole + 2;
/// Composed thumbnail, drawn by the delegate rather than by Qt's decoration.
constexpr int kThumbRole = Qt::UserRole + 3;
/// True for the Background layer, whose name CS6 sets in italics.
constexpr int kBackgroundRole = Qt::UserRole + 4;
constexpr int kClippingRole = Qt::UserRole + 5;
/// True when the layer carries a Layer Style, which CS6 marks with an `fx`.
constexpr int kEffectsRole = Qt::UserRole + 6;
/// The layer a row belongs to. Every row carries it, including the effect rows
/// under a layer, so nothing has to work back from a row's position.
constexpr int kLayerRole = Qt::UserRole + 7;
/// What kind of row this is: see `RowKind`.
constexpr int kRowKindRole = Qt::UserRole + 8;
/// The effect a row stands for, e.g. "stroke". Empty on the others.
constexpr int kEffectKeyRole = Qt::UserRole + 9;
/// Whether a layer row has an Effects branch to open. Not set on a group:
/// that has its own triangle at the left of the row.
constexpr int kExpandableRole = Qt::UserRole + 10;
/// The layer's row colour, 0 for none.
constexpr int kLabelRole = Qt::UserRole + 11;
/// The mask's own thumbnail, drawn beside the layer's.
constexpr int kMaskThumbRole = Qt::UserRole + 12;
/// Whether the chain between the two thumbnails is joined.
constexpr int kMaskLinkedRole = Qt::UserRole + 13;
/// How deep in the group tree a row sits: 0 loose, 1 inside a group.
constexpr int kDepthRole = Qt::UserRole + 14;
/// True on a group folder's row.
constexpr int kGroupRole = Qt::UserRole + 15;
/// Whether that folder is open. Kept apart from the tree's own expansion,
/// which belongs to the Effects branch — a group's members are siblings in the
/// view, hidden rather than collapsed, so that every row's position still is
/// its layer index.
constexpr int kGroupOpenRole = Qt::UserRole + 16;

/// The chain between a layer's thumbnail and its mask's.
constexpr int kChainWidth = 12;

/// How far a group's members are indented.
constexpr int kGroupIndent = 14;

/// The band at the top and bottom of a folder's row that still means "put it
/// above/below this group" rather than "put it inside".
constexpr int kDropEdge = 9;

/// The slot before every layer row's thumbnail, where a group's disclosure
/// triangle goes. Reserved on all of them so the thumbnails stay in one
/// column whether or not the document has any groups in it.
constexpr int kDiscloseSlot = 13;

/// How far a row's contents start from the left, past the eye column and the
/// disclosure slot, for a row at this depth.
int rowIndent(int depth)
{
    return kDiscloseSlot + depth * kGroupIndent;
}

/// The three kinds of row in the tree.
enum RowKind {
    LayerRow = 0,
    /// The "Effects" branch heading under a styled layer.
    EffectsGroupRow = 1,
    /// One effect under that heading.
    EffectRow = 2,
};

/// Height of an effect row — CS6's are shorter than a layer's.
constexpr int kChildRowHeight = 20;
/// The eye on a child row, and the box it sits in.
constexpr int kChildEyeSize = 12;
constexpr int kChildEyeColumn = 20;
/// The expander triangle CS6 puts at the right of a styled layer's row.
constexpr int kExpanderSize = 9;
/// The reorder arrows that appear on the selected row.
constexpr int kArrowSize = 11;

/// Where the name starts: past the eye column and the thumbnail — and past the
/// chain and the mask's thumbnail, on a layer that has one.
int nameLeft(bool hasMask = false)
{
    const int masked = hasMask ? kChainWidth + kThumbSize + 4 : 0;
    return kEyeColumn + 8 + kThumbSize + masked + 8;
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
        // A tagged row is tinted, and keeps its tint under the pointer — the
        // point of the colour is to find the layer again, so it should not
        // vanish as soon as the mouse is near it. Selection still wins, since
        // that is the more urgent thing to see.
        if (option.state & QStyle::State_Selected) {
            painter->fillRect(row, kRowSelected);
        } else if (option.state & QStyle::State_MouseOver) {
            painter->fillRect(row, kRowHover);
        }

        // The row colour paints the eye column, as CS6 does — which is also
        // why it survives selection: a colour you tagged a layer with to find
        // it again is no use if it disappears the moment you find it.
        const QColor label = LayerIcons::labelColor(index.data(kLabelRole).toInt());
        if (label.isValid()) {
            painter->fillRect(QRect(row.left(), row.top(), kEyeColumn, row.height()), label);
        }

        if (index.data(kRowKindRole).toInt() != LayerRow) {
            paintChildRow(painter, option, index);
            painter->restore();
            return;
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
        // A layer inside a group is stepped in from the ones around it, which
        // is the only thing on the row that says it is in there. The eye
        // column stays where it is, as CS6 keeps it lined up down the panel.
        const int indent = rowIndent(index.data(kDepthRole).toInt());

        // A group's disclosure triangle, at the left of its folder — where
        // CS6 puts it, and clear of the reorder arrows at the other end of the
        // row, which are also triangles.
        if (index.data(kGroupRole).toBool()) {
            paintExpander(painter, discloseRect(row), index.data(kGroupOpenRole).toBool());
        }

        const QPixmap pixmap = index.data(kThumbRole).value<QPixmap>();
        const int thumbLeft = row.left() + kEyeColumn + 8 + indent;
        paintThumbnail(painter, pixmap, thumbLeft, row);

        // A masked layer carries a second square, with the chain between them
        // that says whether the two travel together.
        const QPixmap maskThumb = index.data(kMaskThumbRole).value<QPixmap>();
        if (!maskThumb.isNull()) {
            const int maskLeft = thumbLeft + kThumbSize + kChainWidth;
            paintThumbnail(painter, maskThumb, maskLeft, row);
            paintChain(painter, chainRect(row, indent),
                       index.data(kMaskLinkedRole).toBool());
        }

        // Everything on the right of a row is stacked from the edge inward, in
        // one place, so nothing lands on top of anything else however many of
        // them a layer happens to have.
        int textRight = row.right() - 6;

        // CS6 puts the branch's expander at the right of the row rather than
        // at the left, which is why the tree draws no decoration of its own.
        if (index.data(kExpandableRole).toBool()) {
            paintExpander(painter, expanderRect(row), option.state & QStyle::State_Open);
            textRight = expanderRect(row).left() - 4;
        }

        // The reorder arrows, on the selected row only: the same move a drag
        // makes, for anyone who would rather click than drag.
        if (option.state & QStyle::State_Selected) {
            const int siblings = index.model()->rowCount(index.parent());
            QRect down;
            QRect up;
            moveArrowRects(row, index.data(kExpandableRole).toBool(), down, up);
            paintArrow(painter, down, false, index.row() < siblings - 1);
            paintArrow(painter, up, true, index.row() > 0);
            textRight = down.left() - 4;
        }

        const int lock = index.data(kLockRole).toInt();
        if (lock > 0) {
            const auto glyph = lock == 2 ? LayerIcons::Glyph::LockSolid
                                         : LayerIcons::Glyph::LockOutline;
            const QPixmap badge = LayerIcons::pixmap(glyph, kGlyph, kBadgeSize);
            const int x = textRight - kBadgeSize;
            const int y = row.top() + (row.height() - kBadgeSize) / 2;
            // On a dark plate, so a lock reads at a glance rather than
            // disappearing into a selected row's blue.
            painter->setPen(Qt::NoPen);
            painter->setBrush(kLockPlate);
            painter->drawRoundedRect(QRect(x - 3, y - 3, kBadgeSize + 6, kBadgeSize + 6),
                                     3, 3);
            painter->drawPixmap(x, y, badge);
            textRight = x - 7;
        }

        // The `fx` mark. It is the only thing on the row that says a layer
        // carries a style, and CS6 puts it over here for the same reason.
        if (index.data(kEffectsRole).toBool()) {
            const QPixmap badge =
                LayerIcons::pixmap(LayerIcons::Glyph::Effects, kGlyph, kBadgeSize);
            const int x = textRight - kBadgeSize;
            painter->drawPixmap(x, row.top() + (row.height() - kBadgeSize) / 2, badge);
            textRight = x - 4;
        }

        QFont font = option.font;
        // Photoshop italicises the Background layer's name, which is how you
        // tell at a glance that it is not an ordinary layer.
        font.setItalic(index.data(kBackgroundRole).toBool());
        painter->setFont(font);
        painter->setPen(kRowText);

        // CS6 rules a line under each layer row. It used to come from the
        // stylesheet, which cannot tell a layer row from an effect row now that
        // both live in the same view.
        painter->setPen(QColor(0x4a, 0x4a, 0x4a));
        painter->drawLine(row.left(), row.bottom(), row.right(), row.bottom());
        painter->setPen(kRowText);

        const int left = row.left() + nameLeft(!maskThumb.isNull()) + indent
            + (index.data(kClippingRole).toBool() ? 10 : 0);
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
        return QSize(0, index.data(kRowKindRole).toInt() == LayerRow ? kRowHeight
                                                                    : kChildRowHeight);
    }

    /// Where the chain sits, between the two thumbnails. `indent` is the row's
    /// own, from `rowIndent` — the chain travels with the thumbnails.
    static QRect chainRect(const QRect &row, int indent = 0)
    {
        const int left = row.left() + kEyeColumn + 8 + indent + kThumbSize;
        return QRect(left, row.top() + (row.height() - 18) / 2, kChainWidth, 18);
    }

    /// Where a group's disclosure triangle sits: at the left of its folder,
    /// in the slot `rowIndent` reserves before every thumbnail.
    static QRect discloseRect(const QRect &row)
    {
        const int size = kExpanderSize;
        return QRect(row.left() + kEyeColumn + 5, row.top() + (row.height() - size) / 2,
                     size, size);
    }

    /// Where the expander sits on a layer row that has one.
    static QRect expanderRect(const QRect &row)
    {
        return QRect(row.right() - kExpanderSize - 6,
                     row.top() + (row.height() - kExpanderSize) / 2, kExpanderSize,
                     kExpanderSize);
    }

    /// Where the two reorder arrows sit on the selected row. Shared with the
    /// view, which has to hit-test exactly what was drawn.
    static void moveArrowRects(const QRect &row, bool expandable, QRect &down, QRect &up)
    {
        const int right = expandable ? expanderRect(row).left() - 4 : row.right() - 6;
        const int y = row.top() + (row.height() - kArrowSize) / 2;
        up = QRect(right - kArrowSize, y, kArrowSize, kArrowSize);
        down = QRect(up.left() - kArrowSize - 4, y, kArrowSize, kArrowSize);
    }

private:
    /// One of a row's squares, centred in its 32px box with CS6's hairline —
    /// without which a white layer has no edge against the row.
    static void paintThumbnail(QPainter *painter, const QPixmap &pixmap, int left,
                               const QRect &row)
    {
        if (pixmap.isNull()) {
            return;
        }
        const QSize size = pixmap.deviceIndependentSize().toSize();
        const QRect box(left + (kThumbSize - size.width()) / 2,
                        row.top() + (row.height() - size.height()) / 2, size.width(),
                        size.height());
        painter->drawPixmap(box, pixmap);
        painter->setPen(kDivider);
        painter->setBrush(Qt::NoBrush);
        painter->drawRect(box.adjusted(0, 0, -1, -1));
    }

    /// The chain, drawn joined or broken.
    static void paintChain(QPainter *painter, const QRect &box, bool linked)
    {
        const QPointF centre = QRectF(box).center();
        painter->setBrush(Qt::NoBrush);
        painter->setPen(QPen(linked ? kGlyph : kGlyphOff, 1.2));
        // Two links, one above the other; joined they meet, broken they do not.
        const qreal gap = linked ? 0.0 : 2.0;
        painter->drawRoundedRect(
            QRectF(centre.x() - 3.0, centre.y() - 7.0 - gap, 6.0, 7.0), 3.0, 3.0);
        painter->drawRoundedRect(
            QRectF(centre.x() - 3.0, centre.y() + gap, 6.0, 7.0), 3.0, 3.0);
    }

    static void paintExpander(QPainter *painter, const QRect &box, bool open)
    {
        const QPointF centre = QRectF(box).center();
        QPainterPath arrow;
        if (open) {
            // Pointing down: the branch is open.
            arrow.moveTo(centre.x() - 4, centre.y() - 2);
            arrow.lineTo(centre.x() + 4, centre.y() - 2);
            arrow.lineTo(centre.x(), centre.y() + 3);
        } else {
            arrow.moveTo(centre.x() - 2, centre.y() - 4);
            arrow.lineTo(centre.x() + 3, centre.y());
            arrow.lineTo(centre.x() - 2, centre.y() + 4);
        }
        arrow.closeSubpath();
        painter->setPen(Qt::NoPen);
        painter->setBrush(kGlyph);
        painter->drawPath(arrow);
    }

    /// One reorder arrow. `usable` is false at the ends of the stack, where it
    /// is drawn dim rather than left out — a control that comes and goes is
    /// harder to aim at than one that greys.
    static void paintArrow(QPainter *painter, const QRect &box, bool up, bool usable)
    {
        const QPointF centre = QRectF(box).center();
        QPainterPath arrow;
        const qreal tip = up ? -4.0 : 4.0;
        arrow.moveTo(centre.x(), centre.y() + tip);
        arrow.lineTo(centre.x() - 4.0, centre.y() - tip * 0.5);
        arrow.lineTo(centre.x() + 4.0, centre.y() - tip * 0.5);
        arrow.closeSubpath();
        painter->setPen(Qt::NoPen);
        painter->setBrush(usable ? kGlyph : kGlyphOff);
        painter->drawPath(arrow);
    }

    /// The "Effects" heading and the effect rows beneath it: an eye and a name,
    /// no thumbnail and no badges.
    static void paintChildRow(QPainter *painter, const QStyleOptionViewItem &option,
                              const QModelIndex &index)
    {
        const QRect row = option.rect;
        if (index.data(kVisibleRole).toBool()) {
            const QPixmap eye =
                LayerIcons::pixmap(LayerIcons::Glyph::Eye, kGlyph, kChildEyeSize);
            painter->drawPixmap(row.left() + (kChildEyeColumn - kChildEyeSize) / 2,
                                row.top() + (row.height() - kChildEyeSize) / 2, eye);
        }

        painter->setPen(kRowText);
        const QRect textRect(row.left() + kChildEyeColumn, row.top(),
                             row.width() - kChildEyeColumn - 6, row.height());
        painter->drawText(textRect, Qt::AlignVCenter | Qt::AlignLeft,
                          painter->fontMetrics().elidedText(
                              index.data(Qt::DisplayRole).toString(), Qt::ElideRight,
                              textRect.width()));
    }

public:

    /// Renaming edits the name where the name is drawn, not across the whole
    /// row — otherwise the editor covers the thumbnail and the eye.
    void updateEditorGeometry(QWidget *editor, const QStyleOptionViewItem &option,
                              const QModelIndex &index) const override
    {
        QRect rect = option.rect;
        rect.setLeft(rect.left()
                     + nameLeft(!index.data(kMaskThumbRole).value<QPixmap>().isNull()));
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

/// The layer tree.
///
/// Three behaviours on top of `QTreeWidget`. A click in the eye column toggles
/// visibility without moving the selection — as in CS6, where hiding a layer is
/// not the same as choosing it. A click on the expander at the right of a
/// styled row opens or closes its Effects branch, since CS6 puts that control
/// there rather than at the left where Qt would draw it. And a drag either
/// reorders a layer or drops it into a group, which the view says which of by
/// drawing its own indicator: a line in the gap between rows, or the folder
/// lit up when the layer would go inside it.
class LayerTreeWidget : public QTreeWidget
{
public:
    using QTreeWidget::QTreeWidget;

    /// Called with the row whose eye was clicked.
    std::function<void(QTreeWidgetItem *)> onEyeClicked;
    /// Called with the selected row whose reorder arrow was clicked.
    std::function<void(QTreeWidgetItem *, bool up)> onMoveClicked;
    /// Called when a row is dropped: the panel index it came from, and the one
    /// it should end up at.
    std::function<void(int from, int to)> onLayerDropped;
    /// Called when a row is dropped onto a group folder rather than between
    /// two rows: the layer, and the group it should go into.
    std::function<void(int from, int group)> onLayerDroppedIntoGroup;
    /// Called when an effect row is clicked anywhere but its eye.
    std::function<void(QTreeWidgetItem *)> onEffectClicked;
    /// Called when a group folder's expander is clicked.
    std::function<void(QTreeWidgetItem *)> onGroupToggled;
    /// Called when the chain between a layer's two thumbnails is clicked.
    std::function<void(QTreeWidgetItem *)> onChainClicked;

private:
    /// The folder a drop would go into, as a top-level row. -1 for none.
    int m_dropGroup = -1;

public:

protected:
    /// Start the drag as a **copy**, never a move.
    ///
    /// This is the whole of the fix for a row vanishing after a reorder.
    /// `QAbstractItemView::startDrag` ends with
    ///
    /// ```text
    /// if (drag->exec(supportedActions, defaultDropAction) == Qt::MoveAction)
    ///     d->clearOrRemove();
    /// ```
    ///
    /// — it deletes the row it dragged, because with an internal move the base
    /// class expects to have inserted a copy elsewhere. This view does not use
    /// that mechanism: `dropEvent` works out where the row landed and tells the
    /// engine, and the panel is rebuilt from the engine afterwards. Offering
    /// only `CopyAction` means `exec` cannot come back as a move, so nothing is
    /// deleted behind the rebuild's back.
    ///
    /// Setting the action on the drop event is not enough on its own:
    /// `QDropEvent::setDropAction` is ignored unless the action is one the drag
    /// was started with, and an internal move starts with Move alone.
    void startDrag(Qt::DropActions supportedActions) override
    {
        Q_UNUSED(supportedActions);
        QTreeWidget::startDrag(Qt::CopyAction);
    }

    void mousePressEvent(QMouseEvent *event) override
    {
        const QPoint pos = event->position().toPoint();
        QTreeWidgetItem *item = itemAt(pos);
        if (event->button() == Qt::LeftButton && item) {
            const QRect rect = visualItemRect(item);
            const bool child = item->parent() != nullptr;
            const int eyeWidth = child ? kChildEyeColumn : kEyeColumn;
            if (pos.x() >= rect.left() && pos.x() < rect.left() + eyeWidth) {
                if (onEyeClicked) {
                    onEyeClicked(item);
                }
                event->accept();
                return;
            }
            // A group's disclosure triangle opens and closes the folder. Ahead
            // of everything else on the row, since it sits over the space the
            // thumbnail would otherwise start in.
            if (!child && item->data(0, kGroupRole).toBool()
                && LayerRowDelegate::discloseRect(rect).contains(pos)) {
                if (onGroupToggled) {
                    onGroupToggled(item);
                }
                event->accept();
                return;
            }
            // The chain between the thumbnails toggles rather than selects.
            if (!child && onChainClicked
                && !item->data(0, kMaskThumbRole).value<QPixmap>().isNull()
                && LayerRowDelegate::chainRect(
                       rect, rowIndent(item->data(0, kDepthRole).toInt()))
                       .contains(pos)) {
                onChainClicked(item);
                event->accept();
                return;
            }
            // An effect row is a button, not a place to put the selection:
            // clicking it opens the Layer Style dialog on that effect. Handled
            // on the press rather than through `itemClicked`, because
            // selecting the row rebuilds the tree — moving the selection to
            // the layer it belongs to — and the click never survives to be
            // delivered on release.
            if (child && onEffectClicked) {
                onEffectClicked(item);
                event->accept();
                return;
            }
            if (!child && item->data(0, kExpandableRole).toBool()
                && LayerRowDelegate::expanderRect(rect).contains(pos)) {
                item->setExpanded(!item->isExpanded());
                event->accept();
                return;
            }
            // The arrows are drawn on the selected row only, so they are only
            // clickable there — the first click on a row selects it, and the
            // second can move it.
            if (!child && item->isSelected() && onMoveClicked) {
                QRect down;
                QRect up;
                LayerRowDelegate::moveArrowRects(
                    rect, item->data(0, kExpandableRole).toBool(), down, up);
                const int row = indexOfTopLevelItem(item);
                if (down.contains(pos) && row < topLevelItemCount() - 1) {
                    onMoveClicked(item, false);
                    event->accept();
                    return;
                }
                if (up.contains(pos) && row > 0) {
                    onMoveClicked(item, true);
                    event->accept();
                    return;
                }
            }
        }
        QTreeWidget::mousePressEvent(event);
    }

    void dragEnterEvent(QDragEnterEvent *event) override
    {
        // The base class asks the *model* whether it would accept the drop,
        // and an internal move's model only accepts a move — so with the drag
        // started as a copy (see `startDrag`) it refuses here, and a refused
        // enter means no drag-move events arrive at all and nothing can be
        // dropped. This view decides where a row lands itself, so it accepts
        // on the model's behalf.
        QTreeWidget::dragEnterEvent(event);
        event->acceptProposedAction();
    }

    void dragMoveEvent(QDragMoveEvent *event) override
    {
        // The base class would refuse anything it calls a drop *on* a row —
        // which is most of a 40px row, leaving only a few pixels at each edge
        // to aim at. A layer dropped on another is not ambiguous here: it goes
        // above or below it depending on which half was hit, unless the row is
        // a group, where the middle of it means "inside".
        QTreeWidget::dragMoveEvent(event);
        event->acceptProposedAction();
        updateDropTarget(event->position().toPoint());
    }

    void dragLeaveEvent(QDragLeaveEvent *event) override
    {
        clearDropTarget();
        QTreeWidget::dragLeaveEvent(event);
    }

    /// The group a drop at this point would go into, or -1.
    ///
    /// Only the middle of a folder's row counts: the bands at its top and
    /// bottom edges are for dropping above and below it, which is how a layer
    /// gets past a group instead of into it.
    int groupTargetAt(const QPoint &pos) const
    {
        QTreeWidgetItem *over = itemAt(pos);
        if (!over || over->parent() || !over->data(0, kGroupRole).toBool()) {
            return -1;
        }
        const QRect rect = visualItemRect(over);
        if (pos.y() < rect.top() + kDropEdge || pos.y() > rect.bottom() - kDropEdge) {
            return -1;
        }
        // Not into itself, and not into a group already being dragged.
        QTreeWidgetItem *dragged = currentItem();
        while (dragged && dragged->parent()) {
            dragged = dragged->parent();
        }
        if (dragged == over || (dragged && dragged->data(0, kGroupRole).toBool())) {
            return -1;
        }
        return indexOfTopLevelItem(over);
    }

    void updateDropTarget(const QPoint &pos)
    {
        const int group = groupTargetAt(pos);
        if (group != m_dropGroup) {
            m_dropGroup = group;
            viewport()->update();
        }
    }

    void clearDropTarget()
    {
        if (m_dropGroup != -1) {
            m_dropGroup = -1;
            viewport()->update();
        }
    }

    void paintEvent(QPaintEvent *event) override
    {
        QTreeWidget::paintEvent(event);
        if (m_dropGroup < 0) {
            return;
        }
        QTreeWidgetItem *item = topLevelItem(m_dropGroup);
        if (!item) {
            return;
        }
        // CS6 lights the whole folder row up rather than drawing a line, which
        // is the difference between "in here" and "between these two".
        QRect rect = visualItemRect(item);
        rect.setRight(viewport()->width() - 1);
        QPainter painter(viewport());
        painter.fillRect(rect, QColor(0x4a, 0x63, 0x83, 110));
        painter.setPen(QPen(QColor(0x9a, 0xbc, 0xe8), 2));
        painter.drawRect(rect.adjusted(1, 1, -1, -1));
    }

    void dropEvent(QDropEvent *event) override
    {
        // Deliberately not chaining to the base class: letting Qt reorder its
        // own items would leave the view and the engine each holding a
        // different stack until the next refresh, and the row the drag left
        // behind visible in the meantime.
        QTreeWidgetItem *dragged = currentItem();
        while (dragged && dragged->parent()) {
            dragged = dragged->parent();
        }
        if (!dragged || !onLayerDropped) {
            clearDropTarget();
            event->ignore();
            return;
        }

        const int from = indexOfTopLevelItem(dragged);
        const QPoint pos = event->position().toPoint();

        // Onto a folder: into the group, rather than anywhere in the order.
        const int group = groupTargetAt(pos);
        clearDropTarget();
        if (group >= 0 && onLayerDroppedIntoGroup) {
            // Accepted as a copy for the same reason the reorder path is —
            // see below.
            event->setDropAction(Qt::CopyAction);
            event->accept();
            onLayerDroppedIntoGroup(from, group);
            return;
        }

        // Where the row would be inserted, counting the dragged row as still
        // in place.
        int insertAt = topLevelItemCount();
        if (QTreeWidgetItem *over = itemAt(pos)) {
            QTreeWidgetItem *top = over;
            while (top->parent()) {
                top = top->parent();
            }
            const int row = indexOfTopLevelItem(top);
            const QRect rect = visualItemRect(top);
            insertAt = pos.y() > rect.center().y() ? row + 1 : row;
        }

        // Taking the row out first shifts everything below it up by one.
        const int to = insertAt > from ? insertAt - 1 : insertAt;

        // Accepted as a *copy*, though nothing is copied. After the drag ends,
        // `QAbstractItemView::startDrag` deletes the source row itself if the
        // drop came back as a MoveAction and the base `dropEvent` did not
        // handle it — and this one does not chain to the base. That deletion
        // lands after the panel has rebuilt, so the moved layer vanishes from
        // the view while the engine still has it, until the next refresh puts
        // it back. Any action but Move leaves the rows alone.
        event->setDropAction(Qt::CopyAction);
        event->accept();
        if (to != from) {
            onLayerDropped(from, to);
        }
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
        {LayerIcons::Glyph::KindType, tr("Show type layers"), true},
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
    m_tree = new LayerTreeWidget(this);
    m_tree->setObjectName(QStringLiteral("layerList"));
    m_tree->setItemDelegate(new LayerRowDelegate(m_tree));
    m_tree->setHeaderHidden(true);
    m_tree->setColumnCount(1);
    // No branch decoration: the delegate draws CS6's expander at the right of
    // the row instead, and a decoration at the left would indent the layers'
    // eye column away from the edge.
    m_tree->setRootIsDecorated(false);
    m_tree->setIndentation(22);
    m_tree->setMouseTracking(true);
    m_tree->setSelectionMode(QAbstractItemView::ExtendedSelection);
    // Photoshop reorders layers by dragging rows. The drag is for the visuals
    // only: the drop is worked out from where the cursor landed and applied to
    // the engine, so the view is never asked to move a row itself — hence a
    // copy action rather than a move, which is what tells Qt not to try.
    m_tree->setDragDropMode(QAbstractItemView::InternalMove);
    m_tree->setDefaultDropAction(Qt::CopyAction);
    m_tree->setContextMenuPolicy(Qt::CustomContextMenu);
    m_tree->onLayerDropped = [this](int from, int to) {
        if (!m_engine) {
            return;
        }
        // Deferred: rebuilding the tree deletes the very item Qt is delivering
        // the drop to, and the drag has not finished with it yet. Posting the
        // work keeps the item alive until the delivery is over.
        QMetaObject::invokeMethod(
            this,
            [this, from, to] {
                m_engine->moveLayer(from, to);
                emit documentChanged();
                refresh();
            },
            Qt::QueuedConnection);
    };
    m_tree->onLayerDroppedIntoGroup = [this](int from, int group) {
        if (!m_engine) {
            return;
        }
        // Deferred for the same reason as the reorder above.
        QMetaObject::invokeMethod(
            this,
            [this, from, group] {
                if (m_engine->moveLayerIntoGroup(from, group)) {
                    emit documentChanged();
                }
                refresh();
            },
            Qt::QueuedConnection);
    };
    m_tree->onMoveClicked = [this](QTreeWidgetItem *item, bool up) {
        moveLayerBy(layerIndexOf(item), up);
    };
    m_tree->onEyeClicked = [this](QTreeWidgetItem *item) {
        if (item->data(0, kRowKindRole).toInt() == LayerRow) {
            toggleVisibility(layerIndexOf(item));
        } else {
            toggleEffectVisibility(item);
        }
    };
    m_tree->onGroupToggled = [this](QTreeWidgetItem *item) {
        const int index = layerIndexOf(item);
        if (index >= 0 && m_engine) {
            m_engine->setLayerGroupExpanded(index, !m_engine->layerGroupExpanded(index));
            refresh();
        }
    };
    root->addWidget(m_tree, 1);

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

    // CS6's order, left to right. Linking and layer effects are not
    // implemented; they are shown disabled so the footer keeps its shape rather
    // than silently losing glyphs.
    m_linkButton = makeButton(LayerIcons::Glyph::Link, tr("Link layers"), false);
    m_effectsButton = makeButton(LayerIcons::Glyph::Effects, tr("Add a layer style"), false);
    m_maskButton = makeButton(LayerIcons::Glyph::Mask, tr("Add layer mask"), true);
    m_adjustmentButton =
        makeButton(LayerIcons::Glyph::Adjustment, tr("New adjustment layer"), true);
    m_groupButton = makeButton(LayerIcons::Glyph::Group, tr("Create a new group"), true);
    m_addButton = makeButton(LayerIcons::Glyph::NewLayer, tr("Create a new layer"), true);
    m_deleteButton = makeButton(LayerIcons::Glyph::Delete, tr("Delete layer"), true);

    root->addWidget(footer);

    // -- wiring -------------------------------------------------------------
    connect(m_tree, &QTreeWidget::itemSelectionChanged,
            this, &LayersPanel::onSelectionChanged);
    connect(m_tree, &QTreeWidget::itemChanged, this, &LayersPanel::onItemChanged);
    connect(m_tree, &QTreeWidget::customContextMenuRequested,
            this, &LayersPanel::onRowContextMenu);
    // Remember which Effects branches the user closed, so a refresh — and
    // there is one after every edit — does not reopen them.
    // A click on an effect row opens the Layer Style dialog on that effect.
    // CS6 wants a double-click there; one is less to explain, and the rows do
    // nothing else that a click could be confused with.
    m_tree->onChainClicked = [this](QTreeWidgetItem *item) {
        const int layer = layerIndexOf(item);
        if (layer < 0 || !m_engine) {
            return;
        }
        m_engine->setLayerMaskLinked(layer, !m_engine->layerMaskLinked(layer));
        refresh();
    };
    m_tree->onEffectClicked = [this](QTreeWidgetItem *item) {
        const int layer = layerIndexOf(item);
        if (layer < 0) {
            return;
        }
        const QString key = item->data(0, kEffectKeyRole).toString();
        // Posted rather than opened here: a modal dialog raised from inside a
        // mouse press leaves the press without its release.
        QMetaObject::invokeMethod(
            this, [this, layer, key] { emit editLayerStyle(layer, key); },
            Qt::QueuedConnection);
    };
    connect(m_tree, &QTreeWidget::itemCollapsed, this, [this](QTreeWidgetItem *item) {
        if (!m_updating) {
            m_collapsed.insert(layerIndexOf(item));
        }
    });
    connect(m_tree, &QTreeWidget::itemExpanded, this, [this](QTreeWidgetItem *item) {
        if (!m_updating) {
            m_collapsed.remove(layerIndexOf(item));
        }
    });

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
    connect(m_groupButton, &QToolButton::clicked, this, &LayersPanel::addGroup);
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

int LayersPanel::layerIndexOf(const QTreeWidgetItem *item) const
{
    if (!item) {
        return -1;
    }
    // Every row carries its layer, so an effect row answers with the layer it
    // hangs from rather than with nothing.
    const QVariant layer = item->data(0, kLayerRole);
    return layer.isValid() ? layer.toInt() : -1;
}

int LayersPanel::currentIndex() const
{
    return layerIndexOf(m_tree->currentItem());
}

QList<int> LayersPanel::selectedIndices() const
{
    QList<int> indices;
    const auto items = m_tree->selectedItems();
    for (const auto *item : items) {
        const int layer = layerIndexOf(item);
        // Selecting a layer and one of its effect rows is one layer, not two.
        if (layer >= 0 && !indices.contains(layer)) {
            indices.append(layer);
        }
    }
    std::sort(indices.begin(), indices.end());
    return indices;
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
    for (int row = 0; row < m_tree->topLevelItemCount(); ++row) {
        // Two reasons a row does not show, decided together: this is the only
        // place that sets `hidden`, so neither can quietly undo the other.
        //
        // A closed group's members are hidden rather than made child rows,
        // which is what keeps every row's position equal to its layer index.
        const int group = m_engine ? m_engine->layerGroupIndex(row) : -1;
        const bool inClosedGroup = group >= 0 && !m_engine->layerGroupExpanded(group);
        // Hiding a layer takes its effect rows with it, which is what a tree
        // gives for free.
        m_tree->topLevelItem(row)->setHidden(inClosedGroup || !passesFilter(row));
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

    const QList<int> prevSelected = selectedIndices();
    if (count != m_tree->topLevelItemCount()) {
        // The indices in `m_collapsed` no longer name the same layers.
        m_collapsed.clear();
    }

    m_tree->clear();
    for (int i = 0; i < count; ++i) {
        const QString name = m_engine->layerName(i);
        auto *item = new QTreeWidgetItem(m_tree);
        item->setText(0, name);
        item->setFlags(item->flags() | Qt::ItemIsEditable);

        item->setData(0, kRowKindRole, int(LayerRow));
        item->setData(0, kLayerRole, i);
        item->setData(0, kVisibleRole, m_engine->layerVisible(i));
        item->setData(0, kBackgroundRole, name == QLatin1String("Background"));
        item->setData(0, kClippingRole, m_engine->layerIsClipping(i));
        item->setData(0, kLabelRole, m_engine->layerLabel(i));
        item->setData(0, kMaskLinkedRole, m_engine->layerMaskLinked(i));
        if (m_engine->layerHasMask(i)) {
            const QImage mask = m_engine->layerMaskThumbnail(i, kThumbSize);
            if (!mask.isNull()) {
                item->setData(0, kMaskThumbRole, QPixmap::fromImage(mask));
            }
        }
        item->setData(0, kLockRole, m_engine->layerIsFullyLocked(i)
                          ? 2
                          : (m_engine->layerIsLocked(i) ? 1 : 0));

        // Compose the thumbnail over a checkerboard so transparent layers read
        // as transparent rather than black. The engine preserves aspect ratio,
        // so this is the shape of the layer, not a 32px square — the delegate
        // centres it.
        const QImage thumb = m_engine->layerThumbnail(i, kThumbSize);
        const int kind = m_engine->layerKind(i);

        // Two kinds are shown by what they *are* rather than by what they look
        // like: an adjustment layer has no pixels of its own, and a type layer's
        // pixels say nothing about it being editable text. CS6 puts a glyph on
        // white for both, and the T is how a type layer is told apart from a
        // bitmap one at a glance.
        const bool adjustment = kind == 1;
        const bool type = kind == 2;
        // A group has no pixels either — its row shows the folder, as CS6's
        // does.
        const bool group = m_engine->layerIsGroup(i);
        const bool glyphOnly = adjustment || type || group;

        const QSize size = glyphOnly || thumb.isNull() ? QSize(kThumbSize, kThumbSize)
                                                       : thumb.size();
        QPixmap canvas(size);
        canvas.fill(Qt::transparent);
        {
            QPainter p(&canvas);
            if (glyphOnly) {
                p.fillRect(canvas.rect(), group ? QColor(0x5a, 0x5a, 0x5a) : Qt::white);
                const LayerIcons::Glyph glyph =
                    group ? LayerIcons::Glyph::Group
                          : (adjustment ? LayerIcons::Glyph::Adjustment
                                        : LayerIcons::Glyph::KindType);
                const QPixmap art = LayerIcons::pixmap(
                    glyph, group ? QColor(0xd0, 0xd0, 0xd0) : QColor(0x22, 0x22, 0x22), 24);
                p.drawPixmap((size.width() - 24) / 2, (size.height() - 24) / 2, art);
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
        item->setData(0, kThumbRole, canvas);

        const bool styled = m_engine->layerHasEffects(i);
        item->setData(0, kEffectsRole, styled);
        item->setData(0, kGroupRole, group);
        item->setData(0, kGroupOpenRole, group && m_engine->layerGroupExpanded(i));
        item->setData(0, kDepthRole, m_engine->layerGroupIndex(i) >= 0 ? 1 : 0);
        // The Effects branch only: a group's own triangle is at the left of
        // its folder, drawn and hit-tested separately.
        item->setData(0, kExpandableRole, styled);

        QStringList notes;
        if (styled) {
            // Which effects, not merely that there are some: the row has space
            // for one badge, and the tooltip is where the detail goes until
            // the panel can nest them the way CS6 does.
            const QString effects = m_engine->layerEffectNames(i);
            notes << (effects.isEmpty()
                          ? tr("has a layer style")
                          : tr("effects: %1").arg(effects.split(QLatin1Char('\n'),
                                                                Qt::SkipEmptyParts)
                                                     .join(QStringLiteral(", "))));
        }
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
        item->setToolTip(0, notes.isEmpty()
                                ? name
                                : tr("%1 (%2)").arg(name, notes.join(QStringLiteral(", "))));

        if (styled) {
            buildEffectRows(item, i);
            item->setExpanded(!m_collapsed.contains(i));
        }
    }


    if (active >= 0 && active < count) {
        m_tree->setCurrentItem(m_tree->topLevelItem(active));

        for (int idx : prevSelected) {
            if (idx >= 0 && idx < count) {
                m_tree->topLevelItem(idx)->setSelected(true);
            }
        }

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
    //
    // The icon is repainted in the dim colour rather than left to Qt, which
    // does not visibly grey a QIcon built from a pixmap under this theme —
    // the same reason the footer's unimplemented glyphs are drawn in
    // `kGlyphOff` to begin with.
    const bool canDelete = count > 1;
    m_deleteButton->setEnabled(canDelete);
    m_deleteButton->setIcon(LayerIcons::icon(LayerIcons::Glyph::Delete,
                                             canDelete ? kGlyph : kGlyphOff, kFooterGlyph));
    m_deleteButton->setToolTip(canDelete
                                   ? tr("Delete layer")
                                   : tr("A document must keep at least one layer"));

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

void LayersPanel::buildEffectRows(QTreeWidgetItem *parent, int layerIndex)
{
    // The branch heading, whose eye is CS6's "hide this layer's effects".
    auto *group = new QTreeWidgetItem(parent);
    group->setText(0, tr("Effects"));
    group->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable);
    group->setData(0, kRowKindRole, int(EffectsGroupRow));
    group->setData(0, kLayerRole, layerIndex);
    group->setData(0, kVisibleRole,
                   m_engine->layerEffectValue(layerIndex, QStringLiteral("hidden")) < 0.5f);

    const QStringList names = m_engine->layerEffectNames(layerIndex)
                                  .split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    // The engine lists the names; the keys they map to are the dialog's, and
    // this is the one place the two are tied together.
    static const QHash<QString, QString> keys = {
        {QStringLiteral("Bevel & Emboss"), QStringLiteral("bevel")},
        {QStringLiteral("Stroke"), QStringLiteral("stroke")},
        {QStringLiteral("Inner Shadow"), QStringLiteral("innerShadow")},
        {QStringLiteral("Inner Glow"), QStringLiteral("innerGlow")},
        {QStringLiteral("Satin"), QStringLiteral("satin")},
        {QStringLiteral("Color Overlay"), QStringLiteral("colorOverlay")},
        {QStringLiteral("Gradient Overlay"), QStringLiteral("gradientOverlay")},
        {QStringLiteral("Pattern Overlay"), QStringLiteral("patternOverlay")},
        {QStringLiteral("Outer Glow"), QStringLiteral("outerGlow")},
        {QStringLiteral("Drop Shadow"), QStringLiteral("dropShadow")},
    };

    for (const QString &name : names) {
        const QString key = keys.value(name);
        auto *row = new QTreeWidgetItem(group);
        row->setText(0, name);
        row->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable);
        row->setData(0, kRowKindRole, int(EffectRow));
        row->setData(0, kLayerRole, layerIndex);
        row->setData(0, kEffectKeyRole, key);
        // An effect switched off keeps its row — the eye is what says so.
        const bool on =
            m_engine->layerEffectValue(layerIndex, key + QStringLiteral(".on")) >= 0.5f;
        row->setData(0, kVisibleRole, on);
        row->setToolTip(0, on ? tr("%1 — click the eye to switch it off").arg(name)
                              : tr("%1 (off) — click the eye to switch it back on").arg(name));
    }
    group->setExpanded(true);
}

void LayersPanel::toggleVisibility(int index)
{
    if (!m_engine || index < 0 || index >= m_tree->topLevelItemCount()) {
        return;
    }
    QTreeWidgetItem *item = m_tree->topLevelItem(index);
    const bool wantVisible = !item->data(0, kVisibleRole).toBool();
    item->setData(0, kVisibleRole, wantVisible);
    m_engine->setLayerVisible(index, wantVisible);
    emit documentChanged();
}

void LayersPanel::moveLayerBy(int index, bool up)
{
    if (!m_engine || index < 0) {
        return;
    }
    // Rows run top-first, so "up" is toward the front of the list — the same
    // move dragging the row there would make.
    const int destination = up ? index - 1 : index + 1;
    if (destination < 0 || destination >= m_engine->getLayerCount()) {
        return;
    }
    m_engine->moveLayer(index, destination);
    emit documentChanged();
    refresh();
    // The layer keeps the selection, so the arrows stay under the pointer and
    // a run of clicks walks it up or down the stack.
    if (QTreeWidgetItem *moved = m_tree->topLevelItem(destination)) {
        m_tree->setCurrentItem(moved);
    }
}

void LayersPanel::toggleEffectVisibility(QTreeWidgetItem *item)
{
    if (!m_engine || !item) {
        return;
    }
    const int layer = layerIndexOf(item);
    if (layer < 0) {
        return;
    }

    if (item->data(0, kRowKindRole).toInt() == EffectsGroupRow) {
        // The heading's eye hides every effect on the layer and keeps their
        // settings, which is what the engine's `hidden` flag is for.
        const bool nowHidden = item->data(0, kVisibleRole).toBool();
        m_engine->setLayerEffectValue(layer, QStringLiteral("hidden"),
                                      nowHidden ? 1.0f : 0.0f);
    } else {
        // An effect's own eye is its switch, and it works both ways: the row
        // stays either way, so a closed eye can be opened again.
        const QString key = item->data(0, kEffectKeyRole).toString();
        if (key.isEmpty()) {
            return;
        }
        const bool on = item->data(0, kVisibleRole).toBool();
        m_engine->setLayerEffectValue(layer, key + QStringLiteral(".on"), on ? 0.0f : 1.0f);
    }
    m_engine->commitLayerEffects();
    emit documentChanged();
    refresh();
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

void LayersPanel::onItemChanged(QTreeWidgetItem *item, int column)
{
    Q_UNUSED(column);
    if (m_updating || !m_engine || !item) {
        return;
    }
    // Only a layer's own row carries a name the engine keeps; the effect rows
    // are not editable.
    if (item->data(0, kRowKindRole).toInt() != LayerRow) {
        return;
    }
    const int index = layerIndexOf(item);
    if (index < 0) {
        return;
    }
    const QString name = item->text(0);
    if (!name.isEmpty() && name != m_engine->layerName(index)) {
        m_engine->setLayerName(index, name);
    }
}

void LayersPanel::onRowContextMenu(const QPoint &pos)
{
    QTreeWidgetItem *item = m_tree->itemAt(pos);
    if (!item || !m_engine) {
        return;
    }
    // Right-clicking an effect row acts on the layer it belongs to, which is
    // the layer whose menu the user is expecting.
    const int layer = layerIndexOf(item);
    if (layer < 0) {
        return;
    }
    if (layer != currentIndex()) {
        m_tree->setCurrentItem(m_tree->topLevelItem(layer));
    }

    // CS6's row menu, cut down to what the engine can do. Duplicate and Merge
    // Down live here rather than in the footer, which is where Photoshop keeps
    // them too.
    QMenu menu(this);
    QAction *duplicate = menu.addAction(tr("Duplicate Layer"));
    QAction *remove = menu.addAction(tr("Delete Layer"));
    QAction *unlock = menu.addAction(tr("Unlock Layer"));
    menu.addSeparator();
    QAction *mask = menu.addAction(tr("Add Layer Mask"));
    QAction *clip = menu.addAction(m_engine->layerIsClipping(layer)
                                      ? tr("Release Clipping Mask")
                                      : tr("Create Clipping Mask"));
    // Set after the fact as well as at creation, or a row colour would be a
    // decision you could only make once.
    QMenu *colour = menu.addMenu(tr("Layer Color"));
    const QStringList labels = LayerIcons::labelNames();
    const int current = m_engine->layerLabel(layer);
    QList<QAction *> colourActions;
    for (int i = 0; i < labels.size(); ++i) {
        QAction *entry = colour->addAction(labels.at(i));
        const QColor swatchColour = LayerIcons::labelColor(i);
        if (swatchColour.isValid()) {
            QPixmap swatch(14, 14);
            swatch.fill(swatchColour);
            entry->setIcon(QIcon(swatch));
        }
        entry->setCheckable(true);
        entry->setChecked(i == current);
        colourActions.append(entry);
    }

    menu.addSeparator();
    QAction *merge = menu.addAction(tr("Merge Down"));

    const int count = m_tree->topLevelItemCount();
    remove->setEnabled(count > 1);
    // Every route to a locked layer is refused by the engine, so this is the
    // way back out of one — greyed when there is nothing to unlock.
    unlock->setEnabled(m_engine->layerIsLocked(layer));
    merge->setEnabled(layer < count - 1);
    // The bottom layer has nothing to clip to.
    clip->setEnabled(layer < count - 1);

    QAction *chosen = menu.exec(m_tree->viewport()->mapToGlobal(pos));
    if (!chosen) {
        return;
    }
    if (const int picked = colourActions.indexOf(chosen); picked >= 0) {
        m_engine->setLayerLabel(layer, picked);
        refresh();
    } else if (chosen == duplicate) {
        duplicateLayer();
    } else if (chosen == unlock) {
        m_engine->setLayerLocks(layer, false, false, false);
        emit documentChanged();
        refresh();
    } else if (chosen == remove) {
        deleteLayer();
    } else if (chosen == mask) {
        addMask();
    } else if (chosen == merge) {
        mergeDown();
    } else if (chosen == clip) {
        m_engine->setLayerClipping(layer, !m_engine->layerIsClipping(layer));
        emit documentChanged();
        refresh();
    }
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

void LayersPanel::addGroup()
{
    if (!m_engine) {
        return;
    }
    // An empty folder, as CS6's button makes: layers go in by being dragged
    // in. Grouping what is selected in one step is Layer ▸ Group Layers.
    m_engine->addLayerGroup(QString());
    emit documentChanged();
    refresh();
}

void LayersPanel::deleteLayer()
{
    if (!m_engine) {
        return;
    }
    QList<int> indices = selectedIndices();
    if (indices.isEmpty()) {
        const int cur = currentIndex();
        if (cur >= 0)
            indices.append(cur);
    }
    if (indices.isEmpty()) {
        return;
    }

    const int totalLayers = m_engine->getLayerCount();
    if (indices.size() >= totalLayers) {
        QMessageBox::warning(this, tr("Delete Layer"),
                             tr("Cannot delete all layers."));
        return;
    }

    for (int idx : indices) {
        if (m_engine->layerIsFullyLocked(idx)) {
            warnLocked(tr("delete the layer \"%1\"").arg(m_engine->layerName(idx)));
            return;
        }
    }

    std::sort(indices.begin(), indices.end(), std::greater<int>());
    for (int idx : indices)
        m_engine->deleteLayer(idx);

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
        || (index + 1 < m_tree->topLevelItemCount()
            && m_engine->layerIsFullyLocked(index + 1))) {
        warnLocked(tr("merge the layers"));
        return;
    }
    m_engine->mergeLayerDown(index);
    emit documentChanged();
    refresh();
}
