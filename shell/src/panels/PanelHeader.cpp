#include "PanelHeader.h"

#include "../tools/ToolIcons.h"

#include <QHBoxLayout>
#include <QPainter>
#include <QToolButton>

namespace {

/// Header height, matching the slim bar CS6 puts above a panel's contents.
constexpr int kHeaderHeight = 18;
/// The chevron and cross are square and fill the header's height.
constexpr int kButtonSize = 16;

/// The grip texture: two rows of dots, as CS6 draws them.
constexpr int kDotSpacing = 4;
constexpr int kDotSize = 2;

const QColor kIconColor(0xd4, 0xd4, 0xd4);
const QColor kGripColor(0x8a, 0x8a, 0x8a);

} // namespace

PanelHeader::PanelHeader(QWidget *parent)
    : QWidget(parent)
{
    setObjectName(QStringLiteral("panelTitleBar"));
    setFixedHeight(kHeaderHeight);
    setCursor(Qt::SizeAllCursor);

    auto *row = new QHBoxLayout(this);
    row->setContentsMargins(0, 0, 1, 0);
    row->setSpacing(0);
    row->addStretch(1);

    m_collapse = new QToolButton(this);
    m_collapse->setObjectName(QStringLiteral("panelTitleBarButton"));
    m_collapse->setAutoRaise(true);
    m_collapse->setFixedSize(kButtonSize, kButtonSize);
    m_collapse->setIconSize(QSize(11, 11));
    m_collapse->setFocusPolicy(Qt::NoFocus);
    m_collapse->setIcon(ToolIcons::fromSvgBody(ToolIcons::columnToggleSvg(false), kIconColor));
    m_collapse->setToolTip(tr("Expand to two columns"));
    connect(m_collapse, &QToolButton::clicked, this, &PanelHeader::collapseClicked);
    row->addWidget(m_collapse);

    m_close = new QToolButton(this);
    m_close->setObjectName(QStringLiteral("panelTitleBarButton"));
    m_close->setAutoRaise(true);
    m_close->setFixedSize(kButtonSize, kButtonSize);
    m_close->setIconSize(QSize(9, 9));
    m_close->setFocusPolicy(Qt::NoFocus);
    m_close->setIcon(ToolIcons::fromSvgBody(ToolIcons::closeSvg(), kIconColor));
    m_close->setToolTip(tr("Close"));
    connect(m_close, &QToolButton::clicked, this, &PanelHeader::closeClicked);
    row->addWidget(m_close);
}

void PanelHeader::setCollapsePointsLeft(bool pointsLeft)
{
    m_collapse->setIcon(
        ToolIcons::fromSvgBody(ToolIcons::columnToggleSvg(pointsLeft), kIconColor));
    m_collapse->setToolTip(pointsLeft ? tr("Collapse to one column")
                                      : tr("Expand to two columns"));
}

void PanelHeader::setCollapseVisible(bool visible)
{
    m_collapse->setVisible(visible);
}

QSize PanelHeader::sizeHint() const
{
    return QSize(2 * kButtonSize, kHeaderHeight);
}

void PanelHeader::paintEvent(QPaintEvent *event)
{
    Q_UNUSED(event)
    QPainter p(this);

    // The grip runs from the left edge up to the buttons. Two rows of dots,
    // centred vertically — CS6's panel grip.
    const int right = m_collapse->isVisible() ? m_collapse->x() : m_close->x();
    const int left = 3;
    const int available = right - left - 2;
    if (available < kDotSpacing) {
        return;
    }

    const int columns = available / kDotSpacing;
    const int rowTop = height() / 2 - kDotSpacing / 2 - kDotSize / 2;

    p.setPen(Qt::NoPen);
    p.setBrush(kGripColor);
    for (int r = 0; r < 2; ++r) {
        for (int c = 0; c < columns; ++c) {
            p.drawRect(left + c * kDotSpacing, rowTop + r * kDotSpacing, kDotSize, kDotSize);
        }
    }
}
