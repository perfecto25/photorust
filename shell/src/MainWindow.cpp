#include "MainWindow.h"

#include "canvas/CanvasView.h"
#include "dialogs/BlackWhiteDialog.h"
#include "dialogs/BrightnessContrastDialog.h"
#include "dialogs/CanvasSizeDialog.h"
#include "dialogs/ChannelMixerDialog.h"
#include "dialogs/ColorBalanceDialog.h"
#include "dialogs/ColorPickerDialog.h"
#include "dialogs/CurvesDialog.h"
#include "dialogs/LayerStyleDialog.h"
#include "dialogs/LevelsDialog.h"
#include "dialogs/ColorSettingsDialog.h"
#include "dialogs/DuplicateImageDialog.h"
#include "dialogs/DuplicateLayerDialog.h"
#include "dialogs/ExposureDialog.h"
#include "dialogs/FillDialog.h"
#include "dialogs/FillLayerDialog.h"
#include "dialogs/GradientMapDialog.h"
#include "dialogs/HdrToningDialog.h"
#include "dialogs/ExportAsDialog.h"
#include "dialogs/HueSaturationDialog.h"
#include "dialogs/FindReplaceTextDialog.h"
#include "dialogs/FileInfoDialog.h"
#include "dialogs/GifWriter.h"
#include "dialogs/ImageSizeDialog.h"
#include "dialogs/IndexedColorDialog.h"
#include "dialogs/PhotoFilterDialog.h"
#include "dialogs/PosterizeDialog.h"
#include "dialogs/KeyboardShortcutsDialog.h"
#include "dialogs/NewDocumentDialog.h"
#include "dialogs/NewLayerDialog.h"
#include "dialogs/PrintDialog.h"
#include "dialogs/ReplaceColorDialog.h"
#include "dialogs/RotateCanvasDialog.h"
#include "dialogs/TrimDialog.h"
#include "dialogs/SaveForWebDialog.h"
#include "dialogs/SelectiveColorDialog.h"
#include "dialogs/ShadowsHighlightsDialog.h"
#include "dialogs/StrokeDialog.h"
#include "dialogs/ThresholdDialog.h"
#include "dialogs/VibranceDialog.h"
#include "panels/BrushPresetPicker.h"
#include "panels/ChannelsPanel.h"
#include "panels/ColorPanel.h"
#include "panels/HistoryPanel.h"
#include "panels/InfoPanel.h"
#include "panels/LayersPanel.h"
#include "panels/PathsPanel.h"
#include "panels/PropertiesPanel.h"
#include "panels/PanelHeader.h"
#include "shortcuts/CommandRegistry.h"
#include "tools/ToolIcons.h"
#include "tools/ToolStrip.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QApplication>
#include <QButtonGroup>
#include <QCache>
#include <QCheckBox>
#include <QClipboard>
#include <QCloseEvent>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDockWidget>
#include <QDoubleSpinBox>
#include <QFileDialog>
#include <QFileInfo>
#include <QFontDatabase>
#include <QFormLayout>
#include <QGroupBox>
#include <QGridLayout>
#include <QRadioButton>
#include <QImageReader>
#include <QInputDialog>
#include <QIntValidator>
#include <QLabel>
#include <QLayout>
#include <QLineEdit>
#include <QListView>
#include <QPushButton>
#include <QMenu>
#include <QMenuBar>
#include <QPainter>
#include <QPrintDialog>
#include <QPrinter>
#include <QPrinterInfo>
#include <QMessageBox>
#include <QPointer>
#include <QSet>
#include <QSettings>
#include <QSpinBox>
#include <QSignalBlocker>
#include <QStatusBar>
#include <QStyledItemDelegate>
#include <QTabBar>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QVBoxLayout>

#include <algorithm>

namespace {

/// How many files File ▸ Open Recent remembers. Photoshop's default.
constexpr int kMaxRecentFiles = 10;

/// One entry of a file dialog's format list.
struct FormatEntry {
    const char *label;
    /// Space-separated extensions, lower case and without the dot.
    const char *extensions;
};

/// Formats accepted by File ▸ Open. PSD is ours; the rest go through Qt's
/// image plugins.
const FormatEntry kOpenFormats[] = {
    {"Photoshop", "psd"},
    {"PNG", "png"},
    {"JPEG", "jpg jpeg jpe"},
    {"GIF", "gif"},
    {"TIFF", "tif tiff"},
    {"BMP", "bmp"},
    {"WebP", "webp"},
};

/// Formats File ▸ Save As can write. GIF and BMP are absent on purpose: Qt's
/// GIF plugin only reads, and BMP would lose the alpha channel silently.
const FormatEntry kSaveFormats[] = {
    {"Photoshop", "psd"},
    {"PNG", "png"},
    {"JPEG", "jpg jpeg"},
    {"TIFF", "tif tiff"},
};

/// Wildcards for one entry's extensions, in both cases: `*.jpg *.JPG …`.
///
/// Case is a real problem here rather than a detail. Cameras write `IMG_0001.JPG`
/// while everything else writes lower case, and a file dialog that matches
/// patterns literally shows one and hides the other. Two things deal with it,
/// because neither is enough alone:
///
/// - both cases are listed here, which covers what actually exists on disk;
/// - the dialogs below use Qt's own file dialog rather than the desktop's,
///   because Qt matches name filters case-insensitively and so also catches the
///   odd `Jpg` or `jpeG`. The desktop's portal dialog matches through GLib,
///   which is case-sensitive and supports no character classes, so no pattern
///   short of every permutation would do it there.
QString patternsFor(const char *extensions)
{
    QStringList patterns;
    const QStringList list =
        QString::fromLatin1(extensions).split(QLatin1Char(' '), Qt::SkipEmptyParts);
    for (const QString &extension : list) {
        patterns << QStringLiteral("*.%1").arg(extension.toLower());
        patterns << QStringLiteral("*.%1").arg(extension.toUpper());
    }
    return patterns.join(QLatin1Char(' '));
}

/// A dialog filter string built from `formats`.
///
/// `withCombined` puts Photoshop's "All Supported Formats" entry first, which
/// is what the Open dialog wants and the Save dialog does not — saving is a
/// choice of one format, not a search.
QString buildFilter(const FormatEntry *formats, int count, bool withCombined)
{
    QStringList entries;
    QStringList everything;

    for (int i = 0; i < count; ++i) {
        const QString patterns = patternsFor(formats[i].extensions);
        entries << QStringLiteral("%1 (%2)").arg(QLatin1String(formats[i].label), patterns);
        everything << patterns;
    }
    if (withCombined) {
        entries.prepend(QStringLiteral("All Supported Formats (%1)")
                            .arg(everything.join(QLatin1Char(' '))));
    }
    entries << QStringLiteral("All Files (*)");
    return entries.join(QStringLiteral(";;"));
}

QString openFilter()
{
    return buildFilter(kOpenFormats, int(std::size(kOpenFormats)), true);
}

QString saveFilter()
{
    return buildFilter(kSaveFormats, int(std::size(kSaveFormats)), false);
}

/// The extensions one name-filter entry accepts, lower-cased and without
/// duplicates: `Photoshop (*.psd *.PSD)` gives `psd`.
///
/// Entries with no `*.ext` pattern — "All Files (*)" — give nothing, which is
/// the right answer: that entry is a request not to be second-guessed about
/// the format.
QStringList extensionsFor(const QString &nameFilter)
{
    const int open = nameFilter.indexOf(QLatin1Char('('));
    const int close = nameFilter.lastIndexOf(QLatin1Char(')'));
    if (open < 0 || close <= open) {
        return {};
    }

    QStringList extensions;
    const QStringList patterns =
        nameFilter.mid(open + 1, close - open - 1).split(QLatin1Char(' '), Qt::SkipEmptyParts);
    for (const QString &pattern : patterns) {
        if (!pattern.startsWith(QLatin1String("*."))) {
            continue;
        }
        const QString extension = pattern.mid(2).toLower();
        if (!extensions.contains(extension)) {
            extensions << extension;
        }
    }
    return extensions;
}

/// `path` carrying the extension of the format chosen in the dialog's "Files of
/// type" box, the way Photoshop does it: typing `poster` with Photoshop
/// selected saves `poster.psd`.
///
/// An extension the chosen format already accepts is left alone, so `shot.jpeg`
/// does not become `shot.jpeg.jpg`. Anything else is appended to rather than
/// replaced: in `render.2026` the trailing part is a name, not a format, and
/// the file still has to end in something a writer will recognize.
QString withExtension(const QString &path, const QString &nameFilter)
{
    const QStringList extensions = extensionsFor(nameFilter);
    if (path.isEmpty() || extensions.isEmpty()) {
        return path;
    }
    for (const QString &extension : extensions) {
        if (path.endsWith(QLatin1Char('.') + extension, Qt::CaseInsensitive)) {
            return path;
        }
    }
    return path + QLatin1Char('.') + extensions.first();
}

/// Ask for a file through Qt's own dialog.
///
/// Not the desktop's: its name filters are matched case-sensitively (see
/// `patternsFor`), so `IMG_0001.Jpg` would be invisible under any pattern we
/// could give it. Qt's own matching is case-insensitive, which is what makes
/// "any supported format, whatever case it is written in" true rather than
/// nearly true — and it themes with the rest of the application besides.
/// `suggestedName` pre-fills the name box when saving, so a command that
/// already knows what the file should be called does not make the user type it.
QStringList askForFiles(QWidget *parent, const QString &caption, const QString &filter,
                        QFileDialog::AcceptMode mode,
                        const QString &suggestedName = QString())
{
    QFileDialog dialog(parent, caption);
    dialog.setOption(QFileDialog::DontUseNativeDialog);
    dialog.setAcceptMode(mode);
    if (!suggestedName.isEmpty()) {
        dialog.selectFile(suggestedName);
    }
    // Opening takes as many files as are highlighted — Shift for a run,
    // Ctrl for a scattering — and each becomes its own tab. Saving is one file
    // by definition.
    dialog.setFileMode(mode == QFileDialog::AcceptOpen ? QFileDialog::ExistingFiles
                                                       : QFileDialog::AnyFile);
    dialog.setNameFilter(filter);

    if (mode == QFileDialog::AcceptSave) {
        // Tracks the format box so the dialog itself knows what the file will be
        // called. That matters before the dialog closes: it is what makes the
        // overwrite prompt fire for a `poster.psd` that already exists, when all
        // that was typed is `poster`.
        const auto followFilter = [&dialog](const QString &nameFilter) {
            const QStringList extensions = extensionsFor(nameFilter);
            dialog.setDefaultSuffix(extensions.isEmpty() ? QString() : extensions.first());
        };
        followFilter(dialog.selectedNameFilter());
        QObject::connect(&dialog, &QFileDialog::filterSelected, &dialog, followFilter);
    }

    if (dialog.exec() != QDialog::Accepted) {
        return {};
    }

    QStringList chosen = dialog.selectedFiles();
    if (mode == QFileDialog::AcceptSave) {
        // `setDefaultSuffix` covers a bare name and stops there — it leaves
        // `render.2026` alone, having taken `2026` for an extension. So the name
        // is finished here as well, where the whole of it can be judged against
        // the chosen format.
        const QString nameFilter = dialog.selectedNameFilter();
        for (QString &path : chosen) {
            path = withExtension(path, nameFilter);
        }
    }
    return chosen;
}

QString askForFile(QWidget *parent, const QString &caption, const QString &filter,
                   QFileDialog::AcceptMode mode,
                   const QString &suggestedName = QString())
{
    const QStringList chosen = askForFiles(parent, caption, filter, mode, suggestedName);
    return chosen.isEmpty() ? QString() : chosen.first();
}

/// Line-art tint for options-bar icons, matching the tool strip.
const QColor kOptionsIconColor(0xd4, 0xd4, 0xd4);

/// Row height for the font-family list, and the point size each preview is
/// drawn at. Fixed so the popup can lay itself out without measuring a single
/// font (see `FontFamilyDelegate`).
constexpr int kFontRowHeight = 22;
constexpr int kFontPreviewPoints = 11;

/// Renders the Type tool's font-family list the way CS6 does: each row
/// previewed in its own typeface, with a small "T" marking it as a font.
///
/// Rasterizing a preview means loading and shaping a font file, which is far
/// too slow to do from `paint()` — a hovered list repaints constantly, and
/// doing it inline cost seconds to open, lag while scrolling, and blocked long
/// enough that a stale mouse release could dismiss the popup. So `paint()`
/// never rasterizes: it draws whatever is already cached, and otherwise falls
/// back to the list's own font and queues the family up. A zero-interval timer
/// then renders a few at a time between events, so the popup appears at once
/// and the previews fill in behind it.
///
/// Requests are served newest-first because the queue is filled by painting:
/// the most recent additions are the rows the user is looking at now, not the
/// ones they scrolled past.
class FontFamilyDelegate : public QStyledItemDelegate
{
public:
    explicit FontFamilyDelegate(QObject *parent = nullptr)
        : QStyledItemDelegate(parent)
    {
        m_timer.setInterval(0);
        connect(&m_timer, &QTimer::timeout, &m_timer, [this] { renderBatch(); });
    }

    void paint(QPainter *painter, const QStyleOptionViewItem &option,
               const QModelIndex &index) const override
    {
        painter->save();

        const QRect row = option.rect;
        if (option.state & QStyle::State_Selected) {
            painter->fillRect(row, option.palette.color(QPalette::Highlight));
        } else if (option.state & QStyle::State_MouseOver) {
            painter->fillRect(row, option.palette.color(QPalette::Base).lighter(130));
        }

        // A plain "T" marks the row as a font, the way CS6 marks each entry
        // with its format (TrueType/OpenType/PostScript) — one glyph rather
        // than three icons for a distinction this minor. Drawn in the list's
        // own font, so it costs nothing to load.
        painter->setPen(kOptionsIconColor);
        painter->setFont(option.font);
        painter->drawText(QRect(row.left() + 6, row.top(), 14, row.height()),
                          Qt::AlignVCenter | Qt::AlignLeft, QStringLiteral("T"));

        const QString family = index.data(Qt::DisplayRole).toString();
        const qreal ratio = painter->device()->devicePixelRatioF();
        painter->setClipRect(row);

        if (const QPixmap *preview = m_cache.object(cacheKey(family, ratio))) {
            if (preview->isNull()) {
                // Rendered once and rejected — see renderPreview().
                painter->drawText(QRect(row.left() + 24, row.top(), row.width() - 28, row.height()),
                                  Qt::AlignVCenter | Qt::AlignLeft, family);
            } else {
                const int y = row.top() + (row.height() - int(preview->height()
                                                              / preview->devicePixelRatio())) / 2;
                painter->drawPixmap(row.left() + 24, y, *preview);
            }
        } else {
            painter->drawText(QRect(row.left() + 24, row.top(), row.width() - 28, row.height()),
                              Qt::AlignVCenter | Qt::AlignLeft, family);
            request(family, ratio, option.widget);
        }

        painter->restore();
    }

    QSize sizeHint(const QStyleOptionViewItem &option, const QModelIndex &index) const override
    {
        Q_UNUSED(option)
        Q_UNUSED(index)
        return QSize(220, kFontRowHeight);
    }

private:
    static QString cacheKey(const QString &family, qreal ratio)
    {
        return QStringLiteral("%1|%2").arg(family, QString::number(ratio));
    }

    /// Queue a family for rendering the next time the event loop is free.
    void request(const QString &family, qreal ratio, const QWidget *view) const
    {
        if (view) {
            m_view = const_cast<QWidget *>(view);
        }
        const QString key = cacheKey(family, ratio);
        if (m_queued.contains(key)) {
            return;
        }
        m_queued.insert(key);
        m_pending.append({family, ratio});
        if (!m_timer.isActive()) {
            m_timer.start();
        }
    }

    /// Render a handful of queued previews, then repaint. Deliberately a small
    /// slice per pass: the point is that the list keeps responding to scrolling
    /// and hovering while the previews arrive.
    void renderBatch() const
    {
        constexpr int kPerPass = 6;
        for (int i = 0; i < kPerPass && !m_pending.isEmpty(); ++i) {
            const Request req = m_pending.takeLast();
            const QString key = cacheKey(req.family, req.ratio);
            m_queued.remove(key);
            m_cache.insert(key, new QPixmap(renderPreview(req.family, req.ratio)));
        }
        if (m_pending.isEmpty()) {
            m_timer.stop();
        }
        if (m_view) {
            m_view->update();
        }
    }

    /// The family name drawn in its own typeface.
    ///
    /// Returns a null pixmap for fonts that cannot preview their own name
    /// legibly, which the caller renders in the UI font instead. Colour fonts
    /// (COLR/CBDT — Google's "Bitcount Ink" faces, for instance) are the case
    /// that matters: they ignore the pen and paint their own multi-coloured
    /// artwork, which at row height comes out as a smear of confetti rather
    /// than a readable name. There is no Qt API that reports this, so it is
    /// detected from the result — a preview that painted in colours of its own
    /// choosing is one we cannot show.
    QPixmap renderPreview(const QString &family, qreal ratio) const
    {
        // Pin the style so a variable font renders at its regular weight
        // rather than whatever instance Qt would default to. Leaving it as a
        // bare QFont(family) is what made entries like "Bitcount Cursive Semi"
        // render at the wrong weight.
        QFont font = QFontDatabase::font(family, QStringLiteral("Regular"), kFontPreviewPoints);
        font.setPointSize(kFontPreviewPoints);

        const QFontMetrics metrics(font);
        const int width = qBound(1, metrics.horizontalAdvance(family) + 4, 400);

        QPixmap pixmap(QSize(width, kFontRowHeight) * ratio);
        pixmap.setDevicePixelRatio(ratio);
        pixmap.fill(Qt::transparent);

        QPainter painter(&pixmap);
        painter.setRenderHint(QPainter::TextAntialiasing, true);
        painter.setFont(font);
        painter.setPen(kOptionsIconColor);
        painter.drawText(QRect(0, 0, width, kFontRowHeight),
                         Qt::AlignVCenter | Qt::AlignLeft, family);
        painter.end();

        return isColored(pixmap.toImage()) ? QPixmap() : pixmap;
    }

    /// True if the glyphs painted in colours the pen never asked for.
    ///
    /// The pen is a neutral grey, so ordinary text is grey through and through;
    /// only antialiasing fringes stray, and never far. A handful of clearly
    /// saturated pixels therefore means the font supplied its own colour.
    static bool isColored(const QImage &image)
    {
        constexpr int kSaturationThreshold = 60;
        constexpr int kMinColoredPixels = 8;

        int colored = 0;
        for (int y = 0; y < image.height(); ++y) {
            for (int x = 0; x < image.width(); ++x) {
                const QColor pixel = image.pixelColor(x, y);
                if (pixel.alpha() < 32) {
                    continue;
                }
                const int spread = std::max({pixel.red(), pixel.green(), pixel.blue()})
                                   - std::min({pixel.red(), pixel.green(), pixel.blue()});
                if (spread > kSaturationThreshold && ++colored >= kMinColoredPixels) {
                    return true;
                }
            }
        }
        return false;
    }

    struct Request
    {
        QString family;
        qreal ratio;
    };

    mutable QCache<QString, QPixmap> m_cache{1024};
    mutable QList<Request> m_pending;
    mutable QSet<QString> m_queued;
    mutable QPointer<QWidget> m_view;
    mutable QTimer m_timer;
};

/// Stop a dialog's buttons being squeezed narrower than their text.
///
/// A message box sizes itself from its message, and its button row is allowed to
/// shrink below what the buttons asked for — so a long label ends up clipped
/// mid-word. Pinning each button's minimum to its own size hint forces the
/// dialog to widen instead. This bites hardest with the platform-substituted
/// labels: Qt's "Discard" role becomes "Close without Saving" under GTK.
void unsqueezeButtons(QDialog *dialog)
{
    for (QAbstractButton *button : dialog->findChildren<QAbstractButton *>()) {
        button->setMinimumWidth(button->sizeHint().width());
    }
    if (QLayout *layout = dialog->layout()) {
        layout->activate();
    }
}

} // namespace

MainWindow::MainWindow(Engine *engine, CommandRegistry *registry, QWidget *parent)
    : QMainWindow(parent)
    , m_engine(engine)
    , m_registry(registry)
{
    setWindowTitle(tr("PhotoRust"));
    resize(1400, 900);
    // Photoshop lets panels stack into tabbed groups and nest side by side.
    // GroupedDragging is what lets a panel be dragged out of its tab group by
    // its tab and float on its own, the way every CS6 panel does. Without it a
    // tabbed panel can only be undocked by its title bar, and dragging the tab
    // just reorders it within the group.
    setDockOptions(QMainWindow::AnimatedDocks | QMainWindow::AllowNestedDocks
                   | QMainWindow::AllowTabbedDocks | QMainWindow::GroupedDragging);
    setTabPosition(Qt::AllDockWidgetAreas, QTabWidget::North);

    // The canvas sits under a tab bar, one tab per open document, as CS6 does.
    m_canvas = new CanvasView(m_engine, this);

    m_documentTabs = new QTabBar(this);
    m_documentTabs->setObjectName(QStringLiteral("documentTabs"));
    m_documentTabs->setExpanding(false);
    m_documentTabs->setTabsClosable(true);
    m_documentTabs->setMovable(true);
    m_documentTabs->setDrawBase(false);
    m_documentTabs->setElideMode(Qt::ElideRight);
    m_documentTabs->setUsesScrollButtons(true);

    auto *centre = new QWidget(this);
    auto *centreLayout = new QVBoxLayout(centre);
    centreLayout->setContentsMargins(0, 0, 0, 0);
    centreLayout->setSpacing(0);
    centreLayout->addWidget(m_documentTabs);
    centreLayout->addWidget(m_canvas, 1);
    setCentralWidget(centre);

    connect(m_documentTabs, &QTabBar::currentChanged, this, &MainWindow::onTabSelected);
    connect(m_documentTabs, &QTabBar::tabCloseRequested, this, &MainWindow::onTabCloseRequested);

    createToolPanel();

    createOptionsBar();
    createMenus();

    auto *brandLabel = new QLabel(QStringLiteral("photorust"), menuBar());
    brandLabel->setStyleSheet(
        QStringLiteral("color: #e8a020; font-weight: bold; font-style: italic;"
                       " font-size: 13px; padding-right: 8px;"));
    menuBar()->setCornerWidget(brandLabel, Qt::TopRightCorner);

    createDocks();
    createStatusBar();
    connectEngine();
    // Must run after createMenus(), so the tool commands the strip registered
    // and the menu commands are all present in the registry.
    installShortcuts();

    connect(m_toolStrip, &ToolStrip::toolChanged, this, &MainWindow::onToolChanged);
    connect(m_canvas, &CanvasView::cursorMoved, this, &MainWindow::onCursorMoved);
    connect(m_canvas, &CanvasView::cursorMoved, m_infoPanel, &InfoPanel::setCursorPosition);
    connect(m_canvas, &CanvasView::cursorLeft, m_infoPanel, &InfoPanel::clearCursorPosition);
    connect(m_canvas, &CanvasView::zoomChanged, this, &MainWindow::onZoomChanged);
    connect(m_canvas, &CanvasView::contextMenuRequested, this,
            &MainWindow::showSelectionContextMenu);
    connect(m_canvas, &CanvasView::zoomContextMenuRequested, this,
            &MainWindow::showZoomContextMenu);
    // Anything else taking the clipboard means the pixels on it are no longer
    // the ones we copied, so Paste in Place must stop claiming to know where
    // they came from. Our own copy sets the flag again straight afterwards.
    connect(QGuiApplication::clipboard(), &QClipboard::dataChanged, this,
            [this] { m_copyIsOurs = false; });
    // Q, and the button at the foot of the tool strip, both come through here.
    connect(m_toolStrip, &ToolStrip::quickMaskToggled, this, [this](bool on) {
        if (m_engine) {
            m_engine->setQuickMask(on);
        }
        statusBar()->showMessage(on ? tr("Edit in Quick Mask Mode")
                                    : tr("Edit in Standard Mode"),
                                 2000);
    });
    connect(m_canvas, &CanvasView::noteEditRequested, this, &MainWindow::editNote);
    connect(m_canvas, &CanvasView::statusMessage, this, [this](const QString &text) {
        statusBar()->showMessage(text, 4000);
    });
    connect(m_canvas, &CanvasView::mixerLoadChanged, this,
            &MainWindow::refreshMixerLoadSwatch);
    connect(m_canvas, &CanvasView::lockedLayerRefused, this,
            &MainWindow::warnLayerLocked);
    connect(m_canvas, &CanvasView::cloneSourceRequired, this,
            &MainWindow::warnCloneSourceRequired);
    connect(m_canvas, &CanvasView::healingSourceRequired, this,
            &MainWindow::warnHealingSourceRequired);
    connect(m_canvas, &CanvasView::colorPicked, this, [this](const QColor &c) {
        m_colorPanel->setForegroundColor(c);
        m_toolStrip->swatches()->setForeground(c);
    });
    connect(m_canvas, &CanvasView::typeStyleAdopted, this, &MainWindow::adoptTypeStyle);
    connect(m_canvas, &CanvasView::transformStarted, this,
            &MainWindow::showTransformOptionsBar);
    connect(m_canvas, &CanvasView::transformCommitted, this, [this] {
        m_hasTransformed = true;
        if (m_transformAgainAction) m_transformAgainAction->setEnabled(true);
        hideTransformOptionsBar();
    });
    connect(m_canvas, &CanvasView::transformCancelled, this,
            &MainWindow::hideTransformOptionsBar);
    connect(m_canvas, &CanvasView::transformChanged, this,
            &MainWindow::updateTransformReadouts);

    // Let every Color Picker in the application sample the image. A QPointer
    // rather than `this`, because the sampler outlives nothing in particular
    // and must not reach a canvas that has gone.
    ColorPickerDialog::setSampler([canvas = QPointer<CanvasView>(m_canvas)](
                                      const QPoint &globalPos) -> QColor {
        return canvas ? canvas->colorAtGlobal(globalPos) : QColor();
    });

    // Replace Color samples through the canvas itself (it is shown
    // non-modally, so the canvas sees the clicks); it only needs to be able
    // to put its eyedropper on screen.
    ReplaceColorDialog::setCursorHook(
        [canvas = QPointer<CanvasView>(m_canvas)](const QCursor *cursor) {
            if (canvas) {
                canvas->setSamplingCursor(cursor);
            }
        });

    // Keep the tool strip's swatch and the Color panel showing the same pair.
    connect(m_toolStrip->swatches(), &ColorSwatchWidget::foregroundChanged,
            m_colorPanel, &ColorPanel::setForegroundColor);
    connect(m_colorPanel, &ColorPanel::foregroundChanged, this, [this](const QColor &c) {
        m_toolStrip->swatches()->setForeground(c);
    });
    // "Foreground to Background" means the colours as they are now, so the
    // options-bar swatch has to follow them.
    connect(m_colorPanel, &ColorPanel::foregroundChanged, this,
            [this] { refreshGradientSwatch(); });
    // The Color panel only reports the foreground; the tool strip's swatch is
    // where the background pair is edited.
    connect(m_toolStrip->swatches(), &ColorSwatchWidget::backgroundChanged, this,
            [this] { refreshGradientSwatch(); });
    connect(m_toolStrip->swatches(), &ColorSwatchWidget::foregroundChanged, this,
            [this] { refreshGradientSwatch(); });

    onToolChanged(ToolId::Brush, 0);
    refreshDocumentTabs();
    updateWindowTitle();
    // Wait for the first layout pass so the canvas knows its real size.
    QMetaObject::invokeMethod(this, &MainWindow::fitOnScreen, Qt::QueuedConnection);
}

// ------------------------------------------------------------------- menus --

template <typename Slot>
QAction *MainWindow::command(const QString &id, const QString &text, Slot slot)
{
    // The registry already knows this command from shortcuts.json; registering
    // again just attaches the display text and returns the same QAction.
    QAction *action = m_registry->registerCommand(id, text);
    connect(action, &QAction::triggered, this, slot);
    // Actions must be children of the window to fire as window shortcuts.
    addAction(action);
    return action;
}

void MainWindow::createMenus()
{
    // -- File ---------------------------------------------------------------
    QMenu *file = menuBar()->addMenu(tr("&File"));
    file->addAction(command(QStringLiteral("file.new"), tr("&New..."),
                            &MainWindow::newDocument));
    file->addAction(command(QStringLiteral("file.open"), tr("&Open..."),
                            &MainWindow::openDocument));

    // Rebuilt from the stored list every time it is about to be shown, so it
    // is right even after opening a file from somewhere else in the session.
    m_recentMenu = file->addMenu(tr("Open &Recent"));
    connect(m_recentMenu, &QMenu::aboutToShow, this, &MainWindow::refreshRecentMenu);
    refreshRecentMenu();

    file->addSeparator();
    file->addAction(command(QStringLiteral("file.save"), tr("&Save"),
                            [this] { saveDocument(); }));
    file->addAction(command(QStringLiteral("file.saveAs"), tr("Save &As..."),
                            [this] { saveDocumentAs(); }));
    file->addAction(command(QStringLiteral("file.saveSlices"), tr("Save S&lices..."),
                            &MainWindow::exportSlices));
    file->addSeparator();

    QMenu *exportMenu = file->addMenu(tr("E&xport"));
    exportMenu->addAction(command(QStringLiteral("file.exportAs"), tr("Export &As..."),
                                  &MainWindow::exportAs));
    exportMenu->addAction(command(QStringLiteral("file.saveForWeb"),
                                  tr("Save for &Web (Legacy)..."),
                                  &MainWindow::saveForWeb));

    file->addSeparator();
    // Closing one document is the deliberate act that *does* prompt.
    file->addAction(command(QStringLiteral("file.close"), tr("&Close"),
                            &MainWindow::closeDocument));
    file->addAction(command(QStringLiteral("file.closeAll"), tr("Close &All"),
                            &MainWindow::closeAllDocuments));
    file->addSeparator();
    file->addAction(command(QStringLiteral("file.fileInfo"), tr("File &Info..."),
                            &MainWindow::showFileInfo));
    file->addSeparator();
    file->addAction(command(QStringLiteral("file.print"), tr("&Print..."),
                            &MainWindow::printDocument));
    file->addAction(command(QStringLiteral("file.printOneCopy"), tr("Print &One Copy"),
                            &MainWindow::printOneCopy));
    file->addSeparator();
    file->addAction(command(QStringLiteral("file.exit"), tr("E&xit"),
                            [this] { close(); }));

    // -- Edit ---------------------------------------------------------------
    QMenu *edit = menuBar()->addMenu(tr("&Edit"));
    edit->addAction(command(QStringLiteral("edit.undo"), tr("&Undo"), &MainWindow::undo));
    edit->addAction(command(QStringLiteral("edit.stepForward"), tr("Step &Forward"),
                            &MainWindow::redo));
    edit->addAction(command(QStringLiteral("edit.stepBackward"), tr("Step &Backward"),
                            &MainWindow::undo));
    edit->addSeparator();

    auto *cutAction = command(QStringLiteral("edit.cut"), tr("Cu&t"), &MainWindow::cut);
    edit->addAction(cutAction);
    m_editNonTypingActions << cutAction;

    auto *copyAction = command(QStringLiteral("edit.copy"), tr("&Copy"), &MainWindow::copy);
    edit->addAction(copyAction);
    m_editNonTypingActions << copyAction;

    auto *copyMergedAction = command(QStringLiteral("edit.copyMerged"), tr("Copy &Merged"),
                            &MainWindow::copyMerged);
    edit->addAction(copyMergedAction);
    m_editNonTypingActions << copyMergedAction;

    auto *pasteAction = command(QStringLiteral("edit.paste"), tr("&Paste"), &MainWindow::paste);
    edit->addAction(pasteAction);
    m_editNonTypingActions << pasteAction;

    // CS6 groups the three placed pastes under Paste Special.
    QMenu *pasteSpecial = edit->addMenu(tr("Paste &Special"));
    m_editNonTypingActions << pasteSpecial->menuAction();
    pasteSpecial->addAction(command(QStringLiteral("edit.pasteInPlace"), tr("Paste in Place"),
                                    &MainWindow::pasteInPlace));
    pasteSpecial->addAction(command(QStringLiteral("edit.pasteInto"), tr("Paste Into"),
                                    &MainWindow::pasteInto));
    // Paste Outside has no default shortcut in CS6 either.
    QAction *pasteOutside = new QAction(tr("Paste Outside"), this);
    connect(pasteOutside, &QAction::triggered, this, &MainWindow::pasteOutside);
    pasteSpecial->addAction(pasteOutside);

    auto *clearAction = command(QStringLiteral("edit.clear"), tr("Cl&ear"),
                            &MainWindow::clearSelection);
    edit->addAction(clearAction);
    m_editNonTypingActions << clearAction;

    edit->addSeparator();

    auto *fillDialogAction = command(QStringLiteral("edit.fill"),
                            tr("Fill..."), &MainWindow::showFillDialog);
    edit->addAction(fillDialogAction);
    m_editNonTypingActions << fillDialogAction;

    auto *strokeDialogAction = command(QStringLiteral("edit.stroke"),
                            tr("Stroke..."), &MainWindow::showStrokeDialog);
    edit->addAction(strokeDialogAction);
    m_editNonTypingActions << strokeDialogAction;

    edit->addSeparator();

    auto *freeTransformAction = command(QStringLiteral("edit.freeTransform"),
                            tr("Free &Transform"),
                            &MainWindow::freeTransform);
    edit->addAction(freeTransformAction);
    m_editNonTypingActions << freeTransformAction;

    QMenu *transformMenu = edit->addMenu(tr("Transfor&m"));

    m_transformAgainAction = command(QStringLiteral("edit.transformAgain"),
                            tr("Again"), &MainWindow::freeTransform);
    m_transformAgainAction->setEnabled(false);
    transformMenu->addAction(m_transformAgainAction);
    m_editNonTypingActions << m_transformAgainAction;

    transformMenu->addSeparator();

    auto *transformScaleAction = command(QStringLiteral("edit.transformScale"),
                            tr("Scale"), [this] {
        if (m_canvas) m_canvas->beginFreeTransform(CanvasView::TransformMode::Scale);
    });
    transformMenu->addAction(transformScaleAction);
    m_editNonTypingActions << transformScaleAction;

    auto *transformRotateAction = command(QStringLiteral("edit.transformRotate"),
                            tr("Rotate"), [this] {
        if (m_canvas) m_canvas->beginFreeTransform(CanvasView::TransformMode::Rotate);
    });
    transformMenu->addAction(transformRotateAction);
    m_editNonTypingActions << transformRotateAction;

    auto *transformSkewAction = command(QStringLiteral("edit.transformSkew"),
                            tr("Skew"), [this] {
        if (m_canvas) m_canvas->beginFreeTransform(CanvasView::TransformMode::Skew);
    });
    transformMenu->addAction(transformSkewAction);
    m_editNonTypingActions << transformSkewAction;

    auto *transformDistortAction = command(QStringLiteral("edit.transformDistort"),
                            tr("Distort"), [this] {
        if (m_canvas) m_canvas->beginFreeTransform(CanvasView::TransformMode::Distort);
    });
    transformMenu->addAction(transformDistortAction);
    m_editNonTypingActions << transformDistortAction;

    auto *transformPerspectiveAction = command(QStringLiteral("edit.transformPerspective"),
                            tr("Perspective"), [this] {
        if (m_canvas) m_canvas->beginFreeTransform(CanvasView::TransformMode::Perspective);
    });
    transformMenu->addAction(transformPerspectiveAction);
    m_editNonTypingActions << transformPerspectiveAction;

    auto *transformWarpAction = command(QStringLiteral("edit.transformWarp"),
                            tr("Warp"), [this] {
        if (m_canvas) m_canvas->beginFreeTransform(CanvasView::TransformMode::Warp);
    });
    transformMenu->addAction(transformWarpAction);
    m_editNonTypingActions << transformWarpAction;

    transformMenu->addSeparator();

    auto *rotate180Action = command(QStringLiteral("edit.rotate180"),
                            tr("Rotate 180°"), &MainWindow::transformRotate180);
    transformMenu->addAction(rotate180Action);
    m_editNonTypingActions << rotate180Action;

    auto *rotate90CWAction = command(QStringLiteral("edit.rotate90cw"),
                            tr("Rotate 90° Clockwise"), &MainWindow::transformRotate90CW);
    transformMenu->addAction(rotate90CWAction);
    m_editNonTypingActions << rotate90CWAction;

    auto *rotate90CCWAction = command(QStringLiteral("edit.rotate90ccw"),
                            tr("Rotate 90° Counter Clockwise"), &MainWindow::transformRotate90CCW);
    transformMenu->addAction(rotate90CCWAction);
    m_editNonTypingActions << rotate90CCWAction;

    transformMenu->addSeparator();

    auto *flipHAction = command(QStringLiteral("edit.flipHorizontal"),
                            tr("Flip Horizontal"), &MainWindow::transformFlipHorizontal);
    transformMenu->addAction(flipHAction);
    m_editNonTypingActions << flipHAction;

    auto *flipVAction = command(QStringLiteral("edit.flipVertical"),
                            tr("Flip Vertical"), &MainWindow::transformFlipVertical);
    transformMenu->addAction(flipVAction);
    m_editNonTypingActions << flipVAction;

    edit->addSeparator();

    m_autoAlignAction = command(QStringLiteral("edit.autoAlignLayers"),
                            tr("Auto-Align La&yers..."), [this] {
        autoAlignLayers();
    });
    edit->addAction(m_autoAlignAction);
    m_editNonTypingActions << m_autoAlignAction;

    edit->addSeparator();

    auto *findReplaceAction = command(QStringLiteral("edit.findReplaceText"),
                            tr("Find and Replace Te&xt..."),
                            &MainWindow::findReplaceText);
    edit->addAction(findReplaceAction);
    m_editNonTypingActions << findReplaceAction;

    edit->addSeparator();
    edit->addAction(command(QStringLiteral("edit.colorSettings"),
                            tr("&Color Settings..."),
                            &MainWindow::editColorSettings));
    edit->addAction(command(QStringLiteral("edit.keyboardShortcuts"),
                            tr("&Keyboard Shortcuts..."),
                            &MainWindow::editKeyboardShortcuts));

    connect(edit, &QMenu::aboutToShow, this, [this] {
        const bool typing = m_canvas && m_canvas->isTyping();
        for (QAction *a : std::as_const(m_editNonTypingActions)) {
            a->setEnabled(!typing);
        }
        if (m_transformAgainAction && !m_hasTransformed) {
            m_transformAgainAction->setEnabled(false);
        }
        if (m_autoAlignAction && m_layersPanel) {
            if (m_layersPanel->selectedIndices().size() < 2)
                m_autoAlignAction->setEnabled(false);
        }
    });

    // -- Image --------------------------------------------------------------
    QMenu *image = menuBar()->addMenu(tr("&Image"));

    // -- Mode submenu -------------------------------------------------------
    QMenu *modeMenu = image->addMenu(tr("&Mode"));

    struct ModeEntry { int index; const char *label; };
    const ModeEntry modeEntries[] = {
        {0, "Bitmap"},
        {1, "Grayscale"},
        {2, "Duotone"},
        {3, "Indexed Color..."},
        {4, "RGB Color"},
        {5, "CMYK Color"},
        {6, "Lab Color"},
        {7, "Multichannel"},
    };
    auto *modeGroup = new QActionGroup(this);
    modeGroup->setExclusive(true);
    for (const auto &entry : modeEntries) {
        auto *action = modeMenu->addAction(tr(entry.label));
        action->setCheckable(true);
        modeGroup->addAction(action);
        const int modeIdx = entry.index;
        connect(action, &QAction::triggered, this, [this, modeIdx] {
            if (modeIdx == 1 && m_engine->colorMode() != 1) {
                QMessageBox msgBox(this);
                msgBox.setWindowTitle(tr("Message"));
                msgBox.setText(tr("Discard color information?"));
                msgBox.setInformativeText(
                    tr("To control the conversion, use\n"
                       "Image > Adjustments > Black & White."));
                msgBox.setIcon(QMessageBox::Question);
                auto *discardBtn = msgBox.addButton(tr("Discard"), QMessageBox::AcceptRole);
                msgBox.addButton(QMessageBox::Cancel);
                msgBox.exec();
                if (msgBox.clickedButton() != discardBtn)
                    return;
            }
            if (modeIdx == 3) {
                IndexedColorDialog dlg(m_engine, this);
                if (dlg.exec() != QDialog::Accepted)
                    return;
                refreshAll();
                return;
            }
            if (modeIdx != 4 && m_engine->property("layerCount").toInt() > 1) {
                QMessageBox flatBox(this);
                flatBox.setWindowTitle(tr("Adobe Photoshop"));
                flatBox.setText(tr("Changing modes can affect the appearance of layers.\n"
                                   "Flatten image before mode change?"));
                flatBox.setIcon(QMessageBox::Question);
                auto *flattenBtn = flatBox.addButton(tr("Flatten"), QMessageBox::AcceptRole);
                flatBox.addButton(tr("Don't Flatten"), QMessageBox::RejectRole);
                auto *cancelBtn = flatBox.addButton(QMessageBox::Cancel);
                flatBox.exec();
                if (flatBox.clickedButton() == cancelBtn)
                    return;
                if (flatBox.clickedButton() == flattenBtn)
                    m_engine->flattenImage();
            }
            m_engine->setColorMode(modeIdx);
            refreshAll();
        });
    }

    modeMenu->addSeparator();

    auto *depthGroup = new QActionGroup(this);
    depthGroup->setExclusive(true);
    struct DepthEntry { int bits; const char *label; };
    const DepthEntry depthEntries[] = {
        {8,  "8 Bits/Channel"},
        {16, "16 Bits/Channel"},
        {32, "32 Bits/Channel"},
    };
    for (const auto &entry : depthEntries) {
        auto *action = modeMenu->addAction(tr(entry.label));
        action->setCheckable(true);
        depthGroup->addAction(action);
        const int bits = entry.bits;
        connect(action, &QAction::triggered, this, [this, bits] {
            m_engine->setBitDepth(bits);
            refreshAll();
        });
    }

    modeMenu->addSeparator();
    auto *colorTableAction = modeMenu->addAction(tr("Color Table..."));
    colorTableAction->setEnabled(false);

    connect(modeMenu, &QMenu::aboutToShow, this, [this, modeGroup, depthGroup] {
        const int curMode = m_engine->colorMode();
        const int curDepth = m_engine->bitDepth();
        const auto modeActions = modeGroup->actions();
        // indices: 0=Bitmap 1=Grayscale 2=Duotone 3=Indexed 4=RGB 5=CMYK 6=Lab 7=Multi
        for (int i = 0; i < modeActions.size(); ++i) {
            modeActions[i]->setChecked(i == curMode);
            modeActions[i]->setEnabled(true);
        }
        // Bitmap only available from Grayscale
        if (curMode != 1)
            modeActions[0]->setEnabled(false);
        // Duotone only available from Grayscale
        if (curMode != 1)
            modeActions[2]->setEnabled(false);
        // Already-active mode stays checked but clickable is fine
        const auto depthActions = depthGroup->actions();
        for (auto *a : depthActions) {
            if (a->text().startsWith(QString::number(curDepth)))
                a->setChecked(true);
        }
    });

    QMenu *adjustments = image->addMenu(tr("&Adjustments"));

    struct AdjustmentEntry {
        const char *commandId;
        const char *engineName;
    };

    // Top group: Brightness/Contrast, Levels, Exposure (matches CS6 order)
    {
        auto *bcAction = new QAction(tr("Brightness/Contrast..."), this);
        connect(bcAction, &QAction::triggered, this,
                [this] { applyAdjustment(QStringLiteral("Brightness/Contrast")); });
        adjustments->addAction(bcAction);
    }
    const AdjustmentEntry topEntries[] = {
        {"image.levels", "Levels"},
        {"image.curves", "Curves"},
    };
    for (const auto &entry : topEntries) {
        const QString engineName = QString::fromUtf8(entry.engineName);
        adjustments->addAction(
            command(QLatin1String(entry.commandId), engineName,
                    [this, engineName] { applyAdjustment(engineName); }));
    }
    {
        auto *expAction = new QAction(tr("Exposure..."), this);
        connect(expAction, &QAction::triggered, this,
                [this] { applyAdjustment(QStringLiteral("Exposure")); });
        adjustments->addAction(expAction);
    }
    adjustments->addSeparator();

    // Vibrance (CS6 places it between Exposure and Hue/Saturation)
    {
        auto *vibAction = new QAction(tr("Vibrance..."), this);
        connect(vibAction, &QAction::triggered, this,
                [this] { applyAdjustment(QStringLiteral("Vibrance")); });
        adjustments->addAction(vibAction);
    }

    // Middle group: Hue/Saturation, Color Balance, Black & White, Photo Filter, Channel Mixer, Gradient Map
    const AdjustmentEntry midEntries[] = {
        {"image.hueSaturation", "Hue/Saturation"},
        {"image.colorBalance", "Color Balance"},
        {"image.blackAndWhite", "Black & White"},
        {"image.photoFilter", "Photo Filter"},
        {"image.channelMixer", "Channel Mixer"},
        {"image.selectiveColor", "Selective Color"},
        {"image.shadowsHighlights", "Shadows/Highlights"},
        {"image.hdrToning", "HDR Toning"},
        {"image.gradientMap", "Gradient Map"},
    };
    for (const auto &entry : midEntries) {
        const QString engineName = QString::fromUtf8(entry.engineName);
        adjustments->addAction(
            command(QLatin1String(entry.commandId), engineName,
                    [this, engineName] { applyAdjustment(engineName); }));
    }
    adjustments->addSeparator();

    // Bottom group: Invert, Posterize, Threshold
    adjustments->addAction(
        command(QStringLiteral("image.invert"), tr("Invert"),
                [this] { applyAdjustment(QStringLiteral("Invert")); }));
    for (const char *name : {"Posterize", "Threshold"}) {
        const QString engineName = QString::fromUtf8(name);
        auto *action = new QAction(engineName + QStringLiteral("..."), this);
        connect(action, &QAction::triggered, this,
                [this, engineName] { applyAdjustment(engineName); });
        adjustments->addAction(action);
    }
    adjustments->addSeparator();
    adjustments->addAction(
        command(QStringLiteral("image.desaturate"), tr("Desaturate"),
                [this] { applyAdjustment(QStringLiteral("Desaturate")); }));
    adjustments->addAction(
        command(QStringLiteral("image.replaceColor"), tr("Replace Color..."),
                [this] { applyAdjustment(QStringLiteral("Replace Color")); }));
    adjustments->addAction(
        command(QStringLiteral("image.equalize"), tr("Equalize"),
                [this] { applyAdjustment(QStringLiteral("Equalize")); }));

    image->addSeparator();
    image->addAction(command(QStringLiteral("image.imageSize"), tr("&Image Size..."),
                             &MainWindow::showImageSize));
    image->addAction(command(QStringLiteral("image.canvasSize"), tr("&Canvas Size..."),
                             &MainWindow::showCanvasSize));

    // Image Rotation turns the whole document — every layer and the selection
    // with it — where Edit ▸ Transform turns only the active layer.
    QMenu *rotation = image->addMenu(tr("Image Rotation"));
    rotation->addAction(command(QStringLiteral("image.rotate180"), tr("180°"),
                                [this] { rotateCanvas(180.0); }));
    rotation->addAction(command(QStringLiteral("image.rotate90cw"), tr("90° Clockwise"),
                                [this] { rotateCanvas(90.0); }));
    rotation->addAction(command(QStringLiteral("image.rotate90ccw"),
                                tr("90° Counter Clockwise"),
                                [this] { rotateCanvas(270.0); }));
    rotation->addAction(command(QStringLiteral("image.rotateArbitrary"), tr("Arbitrary..."),
                                &MainWindow::showArbitraryRotation));
    rotation->addSeparator();
    rotation->addAction(command(QStringLiteral("image.flipCanvasHorizontal"),
                                tr("Flip Canvas Horizontal"),
                                [this] { flipCanvas(true); }));
    rotation->addAction(command(QStringLiteral("image.flipCanvasVertical"),
                                tr("Flip Canvas Vertical"),
                                [this] { flipCanvas(false); }));

    auto *cropAction = command(QStringLiteral("image.crop"), tr("Crop"),
                               &MainWindow::cropToSelection);
    image->addAction(cropAction);
    image->addAction(command(QStringLiteral("image.trim"), tr("Trim..."),
                             &MainWindow::showTrim));
    image->addAction(command(QStringLiteral("image.revealAll"), tr("Reveal All"), [this] {
        // Nothing hanging off the canvas means nothing to reveal, and the
        // engine says so rather than costing a history step.
        if (m_engine && m_engine->revealAll()) {
            refreshAll();
        }
    }));

    image->addSeparator();
    image->addAction(command(QStringLiteral("image.duplicate"), tr("Duplicate..."),
                             &MainWindow::showDuplicateImage));

    // Analysis is CS6's second way to the two measuring tools, which otherwise
    // hide behind the Eyedropper button. It selects them; it does not
    // duplicate anything they do.
    image->addSeparator();
    QMenu *analysis = image->addMenu(tr("Analysis"));
    const auto measuringTool = [this](EyedropperType variant) {
        if (m_toolStrip) {
            m_toolStrip->setActiveTool(ToolId::Eyedropper, static_cast<int>(variant));
        }
    };
    auto *rulerAction = command(QStringLiteral("image.rulerTool"), tr("Ruler Tool"),
                                [measuringTool] { measuringTool(EyedropperType::Ruler); });
    rulerAction->setCheckable(true);
    analysis->addAction(rulerAction);
    auto *countAction = command(QStringLiteral("image.countTool"), tr("Count Tool"),
                                [measuringTool] { measuringTool(EyedropperType::Count); });
    countAction->setCheckable(true);
    analysis->addAction(countAction);

    connect(image, &QMenu::aboutToShow, this,
            [this, cropAction, rulerAction, countAction] {
                // CS6 greys Crop out until there is a selection to crop to.
                cropAction->setEnabled(m_engine && m_engine->hasSelection());

                // A tick against whichever measuring tool is in hand.
                const bool eyedropper =
                    m_toolStrip && m_toolStrip->activeTool() == ToolId::Eyedropper;
                const int variant = m_toolStrip ? m_toolStrip->activeVariant() : -1;
                rulerAction->setChecked(
                    eyedropper && variant == static_cast<int>(EyedropperType::Ruler));
                countAction->setChecked(
                    eyedropper && variant == static_cast<int>(EyedropperType::Count));
            });

    // -- Layer --------------------------------------------------------------
    QMenu *layer = menuBar()->addMenu(tr("&Layer"));

    QMenu *layerNew = layer->addMenu(tr("&New"));
    layerNew->addAction(command(QStringLiteral("layer.new"), tr("&Layer..."),
                                &MainWindow::showNewLayer));
    auto *fromBackgroundAction =
        command(QStringLiteral("layer.newFromBackground"), tr("Layer from &Background..."),
                &MainWindow::showLayerFromBackground);
    layerNew->addAction(fromBackgroundAction);

    // `layer.newGroup` rather than `layer.group`: this is CS6's Layer ▸ New ▸
    // Group, which makes an empty one, and the registry hands back the *same*
    // QAction for an id — sharing it with Layer ▸ Group Layers below would
    // rename both entries to whichever was registered last.
    layerNew->addAction(command(QStringLiteral("layer.newGroup"), tr("&Group..."),
                                [this] { showNewGroup(false); }));
    auto *groupFromLayersAction =
        command(QStringLiteral("layer.groupFromLayers"), tr("Group &from Layers..."),
                [this] { showNewGroup(true); });
    layerNew->addAction(groupFromLayersAction);
    connect(layerNew, &QMenu::aboutToShow, this, [this, groupFromLayersAction] {
        groupFromLayersAction->setEnabled(m_engine
                                          && m_engine->canGroupLayers(selectedLayerVector()));
    });

    layerNew->addSeparator();
    auto *viaCopyAction =
        command(QStringLiteral("layer.newViaCopy"), tr("Layer Via &Copy"), [this] {
            if (m_engine && m_engine->layerViaCopy()) {
                refreshAll();
            }
        });
    layerNew->addAction(viaCopyAction);
    auto *viaCutAction =
        command(QStringLiteral("layer.newViaCut"), tr("Layer Via Cu&t"), [this] {
            if (m_engine && m_engine->layerViaCut()) {
                refreshAll();
            }
        });
    layerNew->addAction(viaCutAction);

    connect(layerNew, &QMenu::aboutToShow, this,
            [this, fromBackgroundAction, viaCutAction] {
                // Nothing to convert without a Background, and nothing to cut
                // without a selection — CS6 greys both out rather than letting
                // them do nothing.
                fromBackgroundAction->setEnabled(m_engine && m_engine->hasBackgroundLayer());
                viaCutAction->setEnabled(m_engine && m_engine->hasSelection());
            });

    layer->addAction(command(QStringLiteral("layer.duplicate"), tr("&Duplicate Layer..."),
                             &MainWindow::showDuplicateLayer));

    QMenu *layerDelete = layer->addMenu(tr("Dele&te"));
    auto *deleteLayerAction = command(QStringLiteral("layer.delete"), tr("Layer"), [this] {
        if (m_engine) {
            m_engine->deleteLayer(m_engine->getActiveLayerIndex());
            refreshAll();
        }
    });
    layerDelete->addAction(deleteLayerAction);
    auto *deleteHiddenAction =
        command(QStringLiteral("layer.deleteHidden"), tr("Hidden Layers"), [this] {
            if (m_engine && m_engine->deleteHiddenLayers() > 0) {
                refreshAll();
            }
        });
    layerDelete->addAction(deleteHiddenAction);
    connect(layerDelete, &QMenu::aboutToShow, this,
            [this, deleteLayerAction, deleteHiddenAction] {
                // The engine refuses to leave a document with no layers, so
                // the last one cannot go. Nothing hidden, likewise nothing to
                // delete. CS6 greys both out rather than offering a no-op.
                deleteLayerAction->setEnabled(m_engine && m_engine->getLayerCount() > 1);
                deleteHiddenAction->setEnabled(m_engine && m_engine->hiddenLayerCount() > 0);
            });

    layer->addSeparator();
    layer->addAction(command(QStringLiteral("layer.quickExportPng"),
                             tr("&Quick Export as PNG"), &MainWindow::quickExportPng));
    layer->addAction(command(QStringLiteral("layer.exportAs"), tr("&Export As..."),
                             &MainWindow::exportLayerAs));

    layer->addSeparator();
    QMenu *newFill = layer->addMenu(tr("New &Fill Layer"));
    newFill->addAction(command(QStringLiteral("layer.newFillSolid"), tr("Solid Color..."),
                               &MainWindow::showNewFillLayer));
    newFill->addAction(command(QStringLiteral("layer.newFillGradient"), tr("Gradient..."),
                               &MainWindow::showNewGradientFillLayer));
    newFill->addAction(command(QStringLiteral("layer.newFillPattern"), tr("Pattern..."),
                               &MainWindow::showNewPatternFillLayer));

    // CS6's list, in its groups. The engine says which of them it can
    // evaluate as a live layer; the rest are listed and greyed rather than
    // quietly making something else.
    QMenu *newAdjustment = layer->addMenu(tr("New &Adjustment Layer"));
    struct AdjustmentLayerEntry {
        const char *id;
        const char *kind;
    };
    const AdjustmentLayerEntry adjustmentLayers[] = {
        {"layer.adjBrightness", "Brightness/Contrast"},
        {"layer.adjLevels", "Levels"},
        {"layer.adjCurves", "Curves"},
        {"layer.adjExposure", "Exposure"},
        {nullptr, nullptr},
        {"layer.adjVibrance", "Vibrance"},
        {"layer.adjHueSaturation", "Hue/Saturation"},
        {"layer.adjColorBalance", "Color Balance"},
        {"layer.adjBlackWhite", "Black & White"},
        {"layer.adjPhotoFilter", "Photo Filter"},
        {"layer.adjChannelMixer", "Channel Mixer"},
        {"layer.adjColorLookup", "Color Lookup"},
        {nullptr, nullptr},
        {"layer.adjInvert", "Invert"},
        {"layer.adjPosterize", "Posterize"},
        {"layer.adjThreshold", "Threshold"},
        {"layer.adjGradientMap", "Gradient Map"},
        {"layer.adjSelectiveColor", "Selective Color"},
    };
    for (const AdjustmentLayerEntry &entry : adjustmentLayers) {
        if (!entry.id) {
            newAdjustment->addSeparator();
            continue;
        }
        const QString kind = QString::fromLatin1(entry.kind);
        auto *action = command(QString::fromLatin1(entry.id), kind + QStringLiteral("..."),
                               [this, kind] { showNewAdjustmentLayer(kind); });
        if (m_engine && !m_engine->supportsAdjustment(kind)) {
            action->setEnabled(false);
            action->setToolTip(
                tr("Not available as a live layer yet — Image ▸ Adjustments has it"));
        }
        newAdjustment->addAction(action);
    }

    layer->addSeparator();
    QMenu *layerStyle = layer->addMenu(tr("Layer &Style"));
    {
        // The effects the engine can draw, in CS6's menu order. Blending
        // Options, Bevel & Emboss, Satin and Pattern Overlay are in the
        // dialog's list too, greyed, so the shape of Photoshop's menu is
        // visible even where the drawing is not built.
        struct Entry {
            const char *id;
            const char *key;
            QString title;
        };
        const Entry entries[] = {
            {"layer.styleBevel", "bevel", tr("Bevel && Emboss...")},
            {"layer.styleStroke", "stroke", tr("Stroke...")},
            {"layer.styleInnerShadow", "innerShadow", tr("Inner Shadow...")},
            {"layer.styleInnerGlow", "innerGlow", tr("Inner Glow...")},
            {"layer.styleSatin", "satin", tr("Satin...")},
            {"layer.styleColorOverlay", "colorOverlay", tr("Color Overlay...")},
            {"layer.styleGradientOverlay", "gradientOverlay", tr("Gradient Overlay...")},
            {"layer.stylePatternOverlay", "patternOverlay", tr("Pattern Overlay...")},
            {"layer.styleOuterGlow", "outerGlow", tr("Outer Glow...")},
            {"layer.styleDropShadow", "dropShadow", tr("Drop Shadow...")},
        };

        // The dialog's first page, which is the layer's own blending rather
        // than an effect — so it opens with an empty key.
        layerStyle->addAction(command(QStringLiteral("layer.styleBlendingOptions"),
                                      tr("Blending Options..."),
                                      [this] { showLayerStyle(QString()); }));
        layerStyle->addSeparator();

        // Kept with their keys so the menu can tick the ones the active layer
        // already carries, as CS6 does.
        QList<QPair<QAction *, QString>> effectActions;
        for (const Entry &entry : entries) {
            const QString key = QString::fromLatin1(entry.key);
            auto *action = command(QString::fromLatin1(entry.id), entry.title,
                                   [this, key] { showLayerStyle(key); });
            action->setCheckable(true);
            layerStyle->addAction(action);
            effectActions.append({action, key});
        }

        layerStyle->addSeparator();
        auto *copyStyle =
            command(QStringLiteral("layer.copyStyle"), tr("Copy Layer Style"), [this] {
                if (m_engine) {
                    m_engine->copyLayerStyle(m_engine->getActiveLayerIndex());
                }
            });
        auto *pasteStyle =
            command(QStringLiteral("layer.pasteStyle"), tr("Paste Layer Style"), [this] {
                if (m_engine && m_engine->pasteLayerStyle(m_engine->getActiveLayerIndex())) {
                    refreshAll();
                }
            });
        auto *clearStyle =
            command(QStringLiteral("layer.clearStyle"), tr("Clear Layer Style"), [this] {
                if (m_engine && m_engine->clearLayerEffects(m_engine->getActiveLayerIndex())) {
                    refreshAll();
                }
            });
        layerStyle->addAction(copyStyle);
        layerStyle->addAction(pasteStyle);
        layerStyle->addAction(clearStyle);

        layerStyle->addSeparator();
        auto *hideEffects =
            command(QStringLiteral("layer.hideAllEffects"), tr("Hide All Effects"), [this] {
                if (m_engine) {
                    m_engine->hideAllEffects(!m_engine->effectsAreHidden());
                    refreshAll();
                }
            });
        layerStyle->addAction(hideEffects);

        connect(layerStyle, &QMenu::aboutToShow, this,
                [this, copyStyle, pasteStyle, clearStyle, hideEffects, effectActions] {
                    const int index = m_engine ? m_engine->getActiveLayerIndex() : -1;
                    const bool hasStyle = m_engine && m_engine->layerHasEffects(index);

                    // A tick against every effect switched on for this layer —
                    // the same state the dialog's checkboxes show.
                    for (const auto &[action, key] : effectActions) {
                        action->setChecked(
                            m_engine
                            && m_engine->layerEffectValue(index, key + QStringLiteral(".on"))
                                   >= 0.5f);
                    }
                    copyStyle->setEnabled(hasStyle);
                    clearStyle->setEnabled(hasStyle);
                    pasteStyle->setEnabled(m_engine && m_engine->hasCopiedLayerStyle());

                    const bool anyStyle = m_engine && m_engine->anyLayerHasEffects();
                    hideEffects->setEnabled(anyStyle);
                    // One entry that swaps its wording, as CS6 does.
                    hideEffects->setText(m_engine && m_engine->effectsAreHidden()
                                             ? tr("Show All Effects")
                                             : tr("Hide All Effects"));
                });
    }

    layer->addSeparator();
    layer->addAction(command(QStringLiteral("layer.createClippingMask"),
                             tr("Create &Clipping Mask"), [this] {
                                 const int index = m_engine->getActiveLayerIndex();
                                 m_engine->setLayerClipping(index,
                                                            !m_engine->layerIsClipping(index));
                                 refreshAll();
                             }));
    layer->addSeparator();
    // CS6's Group / Ungroup / Hide block.
    auto *groupAction = command(QStringLiteral("layer.group"), tr("&Group Layers"),
                                &MainWindow::groupSelectedLayers);
    layer->addAction(groupAction);
    auto *ungroupAction = command(QStringLiteral("layer.ungroup"), tr("&Ungroup Layers"),
                                  &MainWindow::ungroupSelectedLayers);
    layer->addAction(ungroupAction);
    auto *hideLayersAction = command(QStringLiteral("layer.hide"), tr("&Hide Layers"),
                                     &MainWindow::toggleSelectedLayersVisible);
    layer->addAction(hideLayersAction);
    layer->setToolTipsVisible(true);

    layer->addSeparator();
    auto *mergeDownAction =
        command(QStringLiteral("layer.mergeDown"), tr("&Merge Down"), [this] {
            m_engine->mergeLayerDown(m_engine->getActiveLayerIndex());
            refreshAll();
        });
    layer->addAction(mergeDownAction);
    auto *flattenAction =
        command(QStringLiteral("layer.mergeVisible"), tr("&Flatten Image"), [this] {
            m_engine->flattenImage();
            refreshAll();
        });
    layer->addAction(flattenAction);

    connect(layer, &QMenu::aboutToShow, this,
            [this, mergeDownAction, flattenAction, hideLayersAction, groupAction,
             ungroupAction] {
        // One entry that swaps its wording, as CS6 does: it offers to show the
        // selection back only once all of it is hidden.
        hideLayersAction->setText(selectedLayersAreHidden() ? tr("&Show Layers")
                                                            : tr("&Hide Layers"));
        groupAction->setEnabled(m_engine
                                && m_engine->canGroupLayers(selectedLayerVector()));
        // Ungroup takes the folder or anything in it, so it lights up for
        // either — which is what CS6 does.
        const int active = m_engine ? m_engine->getActiveLayerIndex() : -1;
        ungroupAction->setEnabled(m_engine && active >= 0
                                  && (m_engine->layerIsGroup(active)
                                      || m_engine->layerGroupIndex(active) >= 0));
        const int count = m_engine ? m_engine->getLayerCount() : 0;
        // Merge Down needs something underneath: the bottom layer has nothing
        // to merge into, and a single layer is always the bottom one. The
        // panel's row menu greys its own entry on the same test.
        mergeDownAction->setEnabled(count > 1 && active >= 0 && active < count - 1);
        // Flattening one layer would only rewrite it as itself.
        flattenAction->setEnabled(count > 1);
    });

    // -- Select -------------------------------------------------------------
    QMenu *select = menuBar()->addMenu(tr("&Select"));
    select->addAction(command(QStringLiteral("select.all"), tr("&All"), [this] {
        m_engine->selectAll();
        m_canvas->update();
    }));
    select->addAction(command(QStringLiteral("select.deselect"), tr("&Deselect"), [this] {
        m_engine->deselect();
        m_canvas->update();
    }));
    select->addAction(command(QStringLiteral("select.inverse"), tr("&Inverse"), [this] {
        m_engine->invertSelection();
        m_canvas->update();
    }));
    select->addSeparator();
    select->addAction(command(QStringLiteral("select.feather"), tr("&Feather..."), [this] {
        bool ok = false;
        const int radius = QInputDialog::getInt(this, tr("Feather Selection"),
                                                tr("Feather Radius (pixels):"), 5, 0, 250,
                                                1, &ok);
        if (ok) {
            m_engine->featherSelection(radius);
            m_canvas->update();
        }
    }));

    // -- Filter -------------------------------------------------------------
    QMenu *filter = menuBar()->addMenu(tr("Fi&lter"));
    QMenu *blur = filter->addMenu(tr("&Blur"));
    blur->addAction(command(QStringLiteral("filter.gaussianBlur"), tr("&Gaussian Blur..."),
                            [this] { applyFilter(QStringLiteral("Gaussian Blur")); }));
    QMenu *sharpen = filter->addMenu(tr("&Sharpen"));
    sharpen->addAction(command(QStringLiteral("filter.sharpen"), tr("&Sharpen"),
                               [this] { applyFilter(QStringLiteral("Sharpen")); }));
    sharpen->addAction(command(QStringLiteral("filter.unsharpMask"), tr("&Unsharp Mask..."),
                               [this] { applyFilter(QStringLiteral("Unsharp Mask")); }));
    QMenu *noise = filter->addMenu(tr("&Noise"));
    noise->addAction(command(QStringLiteral("filter.addNoise"), tr("&Add Noise..."),
                             [this] { applyFilter(QStringLiteral("Add Noise")); }));

    // -- View ---------------------------------------------------------------
    QMenu *view = menuBar()->addMenu(tr("&View"));
    view->addAction(command(QStringLiteral("view.zoomIn"), tr("Zoom &In"),
                            &MainWindow::zoomIn));
    view->addAction(command(QStringLiteral("view.zoomOut"), tr("Zoom &Out"),
                            &MainWindow::zoomOut));
    view->addAction(command(QStringLiteral("view.fitOnScreen"), tr("&Fit on Screen"),
                            &MainWindow::fitOnScreen));
    view->addAction(command(QStringLiteral("view.actualPixels"), tr("&Actual Pixels"),
                            &MainWindow::actualPixels));

    // -- Window (populated with the dock toggles in createDocks) ------------
    menuBar()->addMenu(tr("&Window"))->setObjectName(QStringLiteral("windowMenu"));

    // -- Help ---------------------------------------------------------------
    QMenu *help = menuBar()->addMenu(tr("&Help"));
    auto *about = new QAction(tr("&About PhotoRust"), this);
    connect(about, &QAction::triggered, this, [this] {
        // The rendering backend is worth showing here: whether a machine is
        // on the GPU or quietly fell back to the CPU is the first thing to
        // establish when it turns out to be slow.
        QMessageBox::about(this, tr("About PhotoRust"),
                           tr("<h3>PhotoRust</h3>"
                              "<p>A Photoshop CS6 clone.</p>"
                              "<p>Qt %1 shell over a Rust image engine.</p>"
                              "<p><b>Rendering:</b> %2<br>"
                              "<small>%3</small></p>")
                               .arg(QLatin1String(qVersion()),
                                    m_engine->renderBackend(),
                                    m_engine->renderBackendDetail()));
    });
    help->addAction(about);

    // Colour commands live in the keymap but have no menu home in CS6 either;
    // register them so D and X work.
    addAction(command(QStringLiteral("tool.defaultColors"), tr("Default Colors"), [this] {
        m_engine->resetColors();
        m_toolStrip->swatches()->reset();
    }));
    addAction(command(QStringLiteral("tool.swapColors"), tr("Swap Colors"), [this] {
        m_engine->swapColors();
        m_toolStrip->swatches()->swap();
    }));
}

// ------------------------------------------------------------ options bar ---

void MainWindow::installShortcuts()
{
    if (!m_registry) {
        return;
    }
    // Adopt every registered command, not just the ones that reached a menu.
    // Tool commands are registered by the ToolStrip and would otherwise never
    // belong to a widget, leaving their shortcuts dead.
    for (const QString &id : m_registry->commandIds()) {
        QAction *action = m_registry->action(id);
        if (action && !actions().contains(action)) {
            addAction(action);
        }
    }

    // Shift+letter cycles within a tool group, as CS6 does with its "Use Shift
    // Key for Tool Switch" preference on by default. Registered here rather
    // than in the strip so they land on the window with the rest.
    struct Cycle {
        const char *id;
        QString text;
        QString key;
        ToolId tool;
    };
    const Cycle cycles[] = {
        {"tool.marquee.cycle", tr("Cycle Marquee Tool"), QStringLiteral("Shift+M"),
         ToolId::Marquee},
        {"tool.lasso.cycle", tr("Cycle Lasso Tool"), QStringLiteral("Shift+L"),
         ToolId::Lasso},
        {"tool.quickselect.cycle", tr("Cycle Quick Selection Tool"),
         QStringLiteral("Shift+W"), ToolId::QuickSelect},
        {"tool.crop.cycle", tr("Cycle Crop Tool"), QStringLiteral("Shift+C"),
         ToolId::Crop},
        {"tool.eyedropper.cycle", tr("Cycle Eyedropper Tool"), QStringLiteral("Shift+I"),
         ToolId::Eyedropper},
        {"tool.healing.cycle", tr("Cycle Healing Tool"), QStringLiteral("Shift+J"),
         ToolId::Healing},
        {"tool.brush.cycle", tr("Cycle Brush Tool"), QStringLiteral("Shift+B"),
         ToolId::Brush},
    };

    for (const Cycle &entry : cycles) {
        QAction *cycle = m_registry->registerCommand(QString::fromUtf8(entry.id), entry.text,
                                                     QKeySequence(entry.key));
        const ToolId tool = entry.tool;
        connect(cycle, &QAction::triggered, this,
                [this, tool] { m_toolStrip->cycleVariant(tool); });
        if (!actions().contains(cycle)) {
            addAction(cycle);
        }
    }
}

void MainWindow::showZoomContextMenu(const QPoint &globalPos)
{
    // CS6's Zoom tool menu, in its order and grouping: the four sizes it can
    // jump straight to, then the two steps.
    QMenu menu(this);
    menu.setObjectName(QStringLiteral("canvasContextMenu"));
    menu.setToolTipsVisible(true);

    // The registry's own actions, so these show the same shortcuts and run the
    // same handlers as the View menu.
    auto addCommand = [this, &menu](const char *id) {
        if (QAction *action = m_registry->action(QLatin1String(id))) {
            menu.addAction(action);
        }
    };

    addCommand("view.fitOnScreen");
    addCommand("view.actualPixels");

    // 200% has no menu-bar command of its own — it exists only here, as in CS6.
    QAction *twice = menu.addAction(tr("200%"));
    connect(twice, &QAction::triggered, this, [this] { m_canvas->setZoom(2.0); });

    // Print Size needs a resolution to work from, and the document has no DPI
    // yet. Listed and disabled rather than omitted, like every other entry
    // whose engine side is missing.
    QAction *printSize = menu.addAction(tr("Print Size"));
    printSize->setEnabled(false);
    printSize->setToolTip(tr("The document has no print resolution yet"));

    menu.addSeparator();
    addCommand("view.zoomIn");
    addCommand("view.zoomOut");

    menu.exec(globalPos);
}

void MainWindow::showSelectionContextMenu(const QPoint &globalPos)
{
    // CS6's marquee right-click menu, in its order and grouping. Entries the
    // engine cannot do yet are listed and disabled rather than omitted, the
    // same way the tool flyouts list their unimplemented variants — the menu
    // keeps CS6's shape and nothing silently does nothing.
    struct Entry {
        const char *commandId; ///< Registry id, or nullptr for a separator.
        const char *text;      ///< Label, for entries with no command yet.
        bool implemented;
    };
    const Entry entries[] = {
        {"select.deselect", "Deselect", true},
        {"select.inverse", "Select Inverse", true},
        {"select.feather", "Feather...", true},
        {"select.refineEdge", "Refine Edge...", false},
        {nullptr, nullptr, false},
        {nullptr, "Save Selection...", false},
        {nullptr, "Make Work Path...", false},
        {nullptr, nullptr, false},
        {"layer.newViaCopy", "Layer Via Copy", true},
        {"layer.newViaCut", "Layer Via Cut", false},
        {"layer.new", "New Layer...", true},
        {nullptr, nullptr, false},
        {"edit.freeTransform", "Free Transform", false},
        {nullptr, "Transform Selection", false},
        {nullptr, nullptr, false},
        {"edit.fill", "Fill...", true},
        {"edit.stroke", "Stroke...", true},
        {nullptr, nullptr, false},
        {"filter.last", "Last Filter", false},
        {"edit.fade", "Fade...", false},
    };

    QMenu menu(this);
    menu.setObjectName(QStringLiteral("canvasContextMenu"));
    // So the disabled entries can say why they are disabled.
    menu.setToolTipsVisible(true);

    for (const Entry &entry : entries) {
        if (!entry.commandId && !entry.text) {
            menu.addSeparator();
            continue;
        }

        // An implemented entry reuses the registry's action, so the menu shows
        // the same shortcut and runs the same handler as the menu bar.
        if (entry.implemented && entry.commandId) {
            if (QAction *action = m_registry->action(QLatin1String(entry.commandId))) {
                menu.addAction(action);
                continue;
            }
        }

        // Not tr(): the label comes from the table above, not a literal, so
        // there is nothing for lupdate to collect here.
        QAction *placeholder = menu.addAction(QString::fromUtf8(entry.text));
        placeholder->setEnabled(false);
        placeholder->setToolTip(tr("Not implemented yet"));
    }

    menu.exec(globalPos);
}

void MainWindow::createToolPanel()
{
    m_toolStrip = new ToolStrip(m_registry, this);

    // CS6's Tools panel: dragged by its header, dockable on either side,
    // floatable and closable. A QDockWidget with a PanelHeader for its title
    // bar gives all four; a QToolBar could not, because Qt stops painting a
    // toolbar's drag grip once it floats, stranding the panel.
    m_toolsDock = new QDockWidget(tr("Tools"), this);
    m_toolsDock->setObjectName(QStringLiteral("toolsDock"));
    m_toolsDock->setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea);
    m_toolsDock->setFeatures(QDockWidget::DockWidgetMovable
                             | QDockWidget::DockWidgetFloatable
                             | QDockWidget::DockWidgetClosable);

    auto *header = new PanelHeader(m_toolsDock);
    m_toolsDock->setTitleBarWidget(header);
    m_toolsDock->setWidget(m_toolStrip);
    addDockWidget(Qt::LeftDockWidgetArea, m_toolsDock);

    connect(header, &PanelHeader::closeClicked, m_toolsDock, &QWidget::close);
    connect(header, &PanelHeader::collapseClicked, this, [this] {
        m_toolStrip->setColumnCount(m_toolStrip->columnCount() == 1 ? 2 : 1);
    });
    connect(m_toolStrip, &ToolStrip::columnCountChanged, this,
            [this, header](int columns) {
                header->setCollapsePointsLeft(columns == 2);
                // The dock area caches its extent, so ask for the new one.
                resizeDocks({m_toolsDock}, {m_toolStrip->sizeHint().width()},
                            Qt::Horizontal);
            });
}

void MainWindow::createOptionsBar()
{
    m_optionsBar = new QToolBar(tr("Options"), this);
    m_optionsBar->setObjectName(QStringLiteral("optionsBar"));
    m_optionsBar->setMovable(false);
    m_optionsBar->setFloatable(false);
    addToolBar(Qt::TopToolBarArea, m_optionsBar);
}

void MainWindow::populateOptionsBar(ToolId tool, int variant)
{
    // Building the bar sets every control, and each of those emits its change
    // signal. Nothing here is the user changing anything, so the handlers must
    // not act on it — a type layer would be re-rendered, and re-recorded in the
    // History panel, just for being selected.
    const bool wasBuilding = m_buildingOptionsBar;
    m_buildingOptionsBar = true;
    struct Guard {
        bool *flag;
        bool restore;
        ~Guard() { *flag = restore; }
    } guard{&m_buildingOptionsBar, wasBuilding};

    m_optionsBar->clear();
    // These point into the widgets we just deleted.
    m_brushOpacity = nullptr;
    m_brushFlow = nullptr;
    m_brushTipButton = nullptr;
    m_mixerLoadButton = nullptr;
    m_mixerPresetCombo = nullptr;
    m_gradientSwatch = nullptr;
    m_patternSwatch = nullptr;
    m_customShapeButton = nullptr;

    // Name the active variant, so switching to Elliptical says so.
    auto *label = new QLabel(QStringLiteral("  %1  ").arg(toolVariantName(tool, variant)),
                             m_optionsBar);
    QFont bold = label->font();
    bold.setBold(true);
    label->setFont(bold);
    m_optionsBar->addWidget(label);
    m_optionsBar->addSeparator();

    // The healing group is mostly brush-driven, but Patch, Content-Aware Move
    // and Red Eye are not — they have no brush at all, so they must not take the
    // Size/Hardness/Opacity/Flow branch below.
    // The Magic Eraser is the odd one in a brush-tipped group: it clicks once
    // and erases a region, so the tip picker and Flow would configure nothing.
    const bool magicErasing =
        tool == ToolId::Eraser && static_cast<EraserType>(variant) == EraserType::MagicEraser;
    const bool paintsWithBrush = toolPaints(tool) && !magicErasing
        && (!toolHeals(tool) || healingIsBrush(static_cast<HealingType>(variant)));

    if (paintsWithBrush) {
        // CS6 puts Size and Hardness inside the brush preset picker rather than
        // on the bar; the bar carries the tip button that opens it, showing the
        // current tip and its diameter.
        m_brushTipButton = new QToolButton(m_optionsBar);
        m_brushTipButton->setObjectName(QStringLiteral("brushTipButton"));
        m_brushTipButton->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
        m_brushTipButton->setPopupMode(QToolButton::InstantPopup);
        m_brushTipButton->setIconSize(QSize(20, 20));
        m_brushTipButton->setToolTip(tr("Brush preset picker: size, hardness and presets"));
        m_optionsBar->addWidget(m_brushTipButton);
        refreshBrushTipButton();
        connect(m_brushTipButton, &QToolButton::clicked, this, [this] {
            brushPicker()->setValues(m_brushSizeValue, m_brushHardnessValue);
            brushPicker()->popUpUnder(m_brushTipButton);
        });

        // The Pencil has no Flow in CS6 — it lays whole pixels, so there is
        // nothing to build up gradually — and gains Auto Erase instead.
        const bool pencil = tool == ToolId::Brush
            && brushIsPencil(static_cast<BrushType>(variant));

        const bool replacing = tool == ToolId::Brush
            && brushReplacesColor(static_cast<BrushType>(variant));

        // The Mixer Brush has no Opacity in CS6: how much paint reaches the
        // canvas is Wet, Load and Flow's business, and a master opacity on top
        // of them would mean two controls for one thing.
        const bool mixing = tool == ToolId::Brush
            && brushMixesColor(static_cast<BrushType>(variant));

        if (!mixing) {
            m_optionsBar->addWidget(new QLabel(tr("Opacity:"), m_optionsBar));
            m_brushOpacity = new QSpinBox(m_optionsBar);
            m_brushOpacity->setRange(0, 100);
            m_brushOpacity->setValue(100);
            m_brushOpacity->setSuffix(QStringLiteral("%"));
            m_brushOpacity->setFixedWidth(64);
            m_optionsBar->addWidget(m_brushOpacity);
        }

        // The Background Eraser erases by colour, so it gets Sampling, Limits
        // and Tolerance in place of Flow — the same shape as the Color
        // Replacement Brush's bar, which is the same machinery underneath.
        const bool backgroundErasing = tool == ToolId::Eraser
            && static_cast<EraserType>(variant) == EraserType::BackgroundEraser;

        if (replacing) {
            addColorReplaceOptions();
        } else if (backgroundErasing) {
            addBackgroundEraseOptions();
        } else if (mixing) {
            addMixerOptions();
        } else if (!pencil) {
            m_optionsBar->addWidget(new QLabel(tr("Flow:"), m_optionsBar));
            m_brushFlow = new QSpinBox(m_optionsBar);
            m_brushFlow->setRange(1, 100);
            m_brushFlow->setValue(100);
            m_brushFlow->setSuffix(QStringLiteral("%"));
            m_brushFlow->setFixedWidth(64);
            m_optionsBar->addWidget(m_brushFlow);
        } else {
            m_optionsBar->addSeparator();
            auto *autoErase = new QCheckBox(tr("Auto Erase"), m_optionsBar);
            autoErase->setChecked(m_autoErase);
            autoErase->setToolTip(tr("Begin a stroke on a pixel that is already the "
                                     "foreground colour and it paints the background "
                                     "colour instead"));
            m_optionsBar->addWidget(autoErase);
            connect(autoErase, &QCheckBox::toggled, this, [this](bool on) {
                m_autoErase = on;
                if (m_engine) {
                    m_engine->setAutoErase(on);
                }
            });
        }

        if (m_brushOpacity) {
            connect(m_brushOpacity, &QSpinBox::valueChanged, this,
                    &MainWindow::pushBrushSettings);
        }
        if (m_brushFlow) {
            connect(m_brushFlow, &QSpinBox::valueChanged, this,
                    &MainWindow::pushBrushSettings);
        }

        pushBrushSettings();

        // Neither do the toning tools: Exposure (or the Sponge's Flow) is the
        // only strength they have.
        if (tool == ToolId::Dodge) {
            const QString unused = tr("Not used by this tool; see Exposure");
            if (m_brushOpacity) {
                m_brushOpacity->setEnabled(false);
                m_brushOpacity->setToolTip(unused);
            }
            if (m_brushFlow) {
                m_brushFlow->setEnabled(false);
                m_brushFlow->setToolTip(unused);
            }
            addToneOptions(static_cast<ToneTool>(variant));
        }

        // None of the Blur button's three has Opacity or Flow in CS6 — how much
        // they do is Strength's business — so they are disabled rather than left
        // there doing nothing, the same treatment the healing brushes get.
        if (tool == ToolId::Blur) {
            const QString unused = tr("Not used by this tool; see Strength");
            if (m_brushOpacity) {
                m_brushOpacity->setEnabled(false);
                m_brushOpacity->setToolTip(unused);
            }
            if (m_brushFlow) {
                m_brushFlow->setEnabled(false);
                m_brushFlow->setToolTip(unused);
            }
            addBlurOptions(static_cast<BlurTool>(variant));
        }

        // The Clone Stamp adds CS6's Aligned and Sample after the brush
        // controls — the stroke is an ordinary one, so everything above applies
        // to it unchanged. The Pattern Stamp shares the brush controls and
        // swaps the source options for a pattern picker.
        if (tool == ToolId::CloneStamp) {
            if (static_cast<CloneType>(variant) == CloneType::PatternStamp) {
                addPatternStampOptions();
            } else {
                addCloneOptions();
            }
        }

        // The Spot Healing Brush adds CS6's Type buttons after the brush
        // controls. Opacity and Flow have no meaning for it — the region is
        // rebuilt, not painted — so they are hidden rather than left there
        // doing nothing.
        if (toolHeals(tool)) {
            m_brushOpacity->setEnabled(false);
            m_brushOpacity->setToolTip(tr("Not used when healing"));
            m_brushFlow->setEnabled(false);
            m_brushFlow->setToolTip(tr("Not used when healing"));

            m_optionsBar->addSeparator();
            const auto healing = static_cast<HealingType>(variant);
            if (healing == HealingType::SpotHealing) {
                // Only the Spot Healing Brush chooses how to reconstruct; the
                // Healing Brush is told where to sample from instead.
                addHealTypeButtons();
            } else {
                m_optionsBar->addWidget(new QLabel(
                    tr("Alt+click to define a source point, then drag to repair"),
                    m_optionsBar));
            }
        }
    } else if (toolSelects(tool)) {
        addSelectionModeButtons();

        // Feather, as CS6 places it: straight after the combine buttons. The
        // value applies to selections made from now on, not to the current
        // one — Select ▸ Feather is what softens an existing selection.
        //
        // CS6 gives it to the marquee and lasso families only; the Quick
        // Selection button's two tools have no Feather field, so neither do
        // we. The stored radius still applies to them, which is why the canvas
        // is told either way.
        if (tool != ToolId::QuickSelect) {
            m_optionsBar->addWidget(new QLabel(tr("Feather:"), m_optionsBar));
            auto *feather = new QSpinBox(m_optionsBar);
            feather->setRange(0, 1000);
            feather->setValue(m_featherRadius);
            feather->setSuffix(tr(" px"));
            feather->setFixedWidth(72);
            feather->setToolTip(tr("Soften the edge of new selections"));
            m_optionsBar->addWidget(feather);
            connect(feather, &QSpinBox::valueChanged, this, [this](int value) {
                m_featherRadius = value;
                m_canvas->setFeatherRadius(value);
            });
            m_optionsBar->addSeparator();
        }
        m_canvas->setFeatherRadius(m_featherRadius);

        const bool lineSelect = tool == ToolId::Marquee
            && (static_cast<MarqueeType>(variant) == MarqueeType::SingleRow
                || static_cast<MarqueeType>(variant) == MarqueeType::SingleColumn);
        const LassoType lasso = static_cast<LassoType>(variant);
        const QuickSelectType quick = static_cast<QuickSelectType>(variant);

        // The Magnetic Lasso's own three controls, in CS6's order. They tune
        // the edge search, so they only appear for that variant.
        if (tool == ToolId::Lasso && lasso == LassoType::Magnetic) {
            addMagneticOptions();
            m_optionsBar->addSeparator();
        } else if (tool == ToolId::QuickSelect) {
            addQuickSelectOptions(quick);
            m_optionsBar->addSeparator();
        }

        QString hint;
        if (lineSelect) {
            hint = tr("Click to select a line    Ctrl+Shift = add    Ctrl+Alt = subtract");
        } else if (tool == ToolId::Lasso && lasso != LassoType::Freehand) {
            hint = tr("Click to place points    Double-click or Enter to close    "
                      "Backspace undoes one    Esc cancels");
        } else if (tool == ToolId::QuickSelect && quick == QuickSelectType::MagicWand) {
            hint = tr("Click to select a matching area    Ctrl+Shift = add    "
                      "Ctrl+Alt = subtract");
        } else if (tool == ToolId::QuickSelect) {
            hint = tr("Drag to grow the selection    Ctrl+Shift = add    "
                      "Ctrl+Alt = subtract");
        } else {
            hint = tr("Ctrl+Shift = add    Ctrl+Alt = subtract    Click = deselect");
        }
        m_optionsBar->addWidget(new QLabel(hint, m_optionsBar));
    } else if (tool == ToolId::Healing) {
        addHealingRegionOptions(static_cast<HealingType>(variant));
    } else if (tool == ToolId::Gradient) {
        if (static_cast<GradientTool>(variant) == GradientTool::PaintBucket) {
            addBucketOptions();
        } else {
            addGradientOptions();
        }
    } else if (tool == ToolId::Pen) {
        addPenOptions(static_cast<PenTool>(variant));
    } else if (tool == ToolId::PathSelect) {
        m_optionsBar->addWidget(new QLabel(
            static_cast<PathSelectTool>(variant) == PathSelectTool::PathSelection
                ? tr("Drag a subpath to move it")
                : tr("Drag an anchor or handle to reshape the path. Alt+drag a handle "
                     "breaks it free of its smooth point"),
            m_optionsBar));
    } else if (tool == ToolId::Shape) {
        addShapeOptions(static_cast<ShapeTool>(variant));
    } else if (magicErasing) {
        addMagicEraseOptions();
    } else if (tool == ToolId::Crop) {
        addCropOptions(static_cast<CropType>(variant));
    } else if (tool == ToolId::Eyedropper
               && static_cast<EyedropperType>(variant) != EyedropperType::Eyedropper) {
        addAnnotationOptions(static_cast<EyedropperType>(variant));
    } else if (tool == ToolId::Type) {
        addTypeOptions();
    } else if (tool == ToolId::Zoom) {
        m_optionsBar->addWidget(
            new QLabel(tr("Click to zoom in    Drag to zoom into a rectangle    "
                          "Ctrl+Alt+click to zoom out    Right-click for zoom levels"),
                       m_optionsBar));
    } else if (tool == ToolId::Move) {
        m_optionsBar->addWidget(
            new QLabel(tr("Drag to move the active layer    Arrow keys nudge"),
                       m_optionsBar));
    } else if (tool == ToolId::Hand
               && static_cast<HandTool>(variant) == HandTool::RotateView) {
        addRotateViewOptions();
    }

    m_optionsBar->addSeparator();
    auto *spacer = new QWidget(m_optionsBar);
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    m_optionsBar->addWidget(spacer);
}

void MainWindow::addSelectionModeButtons()
{
    // CS6 opens the selection tools' options bar with these four buttons,
    // before Feather and the rest. They are a radio set: the chosen mode
    // persists across tool switches, and holding a modifier overrides it for
    // one drag without moving the checked button (see
    // CanvasView::effectiveSelectionMode).
    struct Entry {
        SelectionMode mode;
        QString hint;
    };
    const Entry entries[] = {
        {SelectionMode::New, QString()},
        {SelectionMode::Add, tr("Ctrl+Shift-drag")},
        {SelectionMode::Subtract, tr("Ctrl+Alt-drag")},
        {SelectionMode::Intersect, tr("Ctrl+Shift+Alt-drag")},
    };

    auto *group = new QButtonGroup(m_optionsBar);
    group->setExclusive(true);

    for (const Entry &entry : entries) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setIcon(ToolIcons::fromSvgBody(ToolIcons::selectionModeSvg(entry.mode),
                                               kOptionsIconColor));
        button->setIconSize(QSize(20, 20));
        button->setChecked(entry.mode == m_selectionMode);

        const QString name = selectionModeName(entry.mode);
        button->setToolTip(entry.hint.isEmpty()
                               ? name
                               : QStringLiteral("%1 (%2)").arg(name, entry.hint));
        button->setStatusTip(button->toolTip());

        group->addButton(button, static_cast<int>(entry.mode));
        m_optionsBar->addWidget(button);
    }

    connect(group, &QButtonGroup::idClicked, this, [this](int id) {
        m_selectionMode = static_cast<SelectionMode>(id);
        m_canvas->setSelectionMode(m_selectionMode);
    });

    // Keep the canvas in step even if the bar was rebuilt for another tool.
    m_canvas->setSelectionMode(m_selectionMode);
    m_optionsBar->addSeparator();
}

void MainWindow::addMagneticOptions()
{
    // Width, Contrast and Frequency, as CS6 labels and orders them. The values
    // live on MainWindow rather than in the widgets so they survive the
    // options bar being rebuilt on every tool change.
    struct Entry {
        QString label;
        int min;
        int max;
        int *value;
        QString suffix;
        QString tip;
    };
    const Entry entries[] = {
        {tr("Width:"), 1, 256, &m_magneticWidth, tr(" px"),
         tr("How far either side of the cursor an edge is looked for")},
        {tr("Contrast:"), 1, 100, &m_magneticContrast, QStringLiteral("%"),
         tr("How strong a gradient has to be to count as an edge")},
        {tr("Frequency:"), 0, 100, &m_magneticFrequency, QString(),
         tr("How often a fastening point is dropped automatically")},
    };

    for (const Entry &entry : entries) {
        m_optionsBar->addWidget(new QLabel(entry.label, m_optionsBar));
        auto *spin = new QSpinBox(m_optionsBar);
        spin->setRange(entry.min, entry.max);
        spin->setValue(*entry.value);
        spin->setSuffix(entry.suffix);
        spin->setFixedWidth(72);
        spin->setToolTip(entry.tip);
        spin->setStatusTip(entry.tip);
        m_optionsBar->addWidget(spin);

        int *slot = entry.value;
        connect(spin, &QSpinBox::valueChanged, this, [this, slot](int value) {
            *slot = value;
            pushMagneticOptions();
        });
    }

    pushMagneticOptions();
}

QString MainWindow::infoHintFor(EyedropperType type) const
{
    // CS6 changes the Info panel's footer per tool, and it is the only place
    // the modifier hints for these tools appear.
    switch (type) {
    case EyedropperType::ColorSampler:
        return tr("Click image to place new color sampler.\n"
                  "Drag to move it. Use Alt to remove.");
    case EyedropperType::Ruler:
        return tr("Click and drag to create ruler.\nDrag either end to adjust.");
    case EyedropperType::Note:
        return tr("Click image to add a note.\nClick a note to edit it.");
    case EyedropperType::Count:
        return tr("Click image to count. Use Alt to remove a mark.");
    case EyedropperType::Eyedropper:
        break;
    }
    return tr("Click image to sample a color.");
}

void MainWindow::addContentAwareMoveOptions()
{
    // CS6's bar, left to right: combine buttons, Mode, Structure, Color,
    // Sample All Layers, and the transform-on-drop toggle.
    addSelectionModeButtons();

    // Mode: Move relocates the subject and heals the gap; Extend leaves the
    // original, so the subject is lengthened instead.
    m_optionsBar->addWidget(new QLabel(tr("Mode:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    mode->addItem(tr("Move"), false);
    mode->addItem(tr("Extend"), true);
    mode->setCurrentIndex(m_camExtend ? 1 : 0);
    m_optionsBar->addWidget(mode);
    connect(mode, &QComboBox::currentIndexChanged, this, [this, mode](int index) {
        m_camExtend = mode->itemData(index).toBool();
        pushContentAwareMoveOptions();
    });

    m_optionsBar->addSeparator();

    struct Slider {
        QString label;
        int min;
        int max;
        int *value;
        QString tip;
    };
    const Slider sliders[] = {
        {tr("Structure:"), 1, 7, &m_camStructure,
         tr("How strictly the filled area follows the edges around it. Higher matches "
            "larger patches more finely")},
        {tr("Color:"), 0, 10, &m_camColor,
         tr("How far the moved pixels shift toward the colour of their new "
            "surroundings. 0 moves them untouched")},
    };
    for (const Slider &slider : sliders) {
        m_optionsBar->addWidget(new QLabel(slider.label, m_optionsBar));
        auto *spin = new QSpinBox(m_optionsBar);
        spin->setRange(slider.min, slider.max);
        spin->setValue(*slider.value);
        spin->setFixedWidth(52);
        spin->setToolTip(slider.tip);
        spin->setStatusTip(slider.tip);
        m_optionsBar->addWidget(spin);

        int *slot = slider.value;
        connect(spin, &QSpinBox::valueChanged, this, [this, slot](int v) {
            *slot = v;
            pushContentAwareMoveOptions();
        });
    }

    m_optionsBar->addSeparator();

    auto *sampleAll = new QCheckBox(tr("Sample All Layers"), m_optionsBar);
    sampleAll->setChecked(m_camSampleAllLayers);
    sampleAll->setToolTip(tr("Read the pixels to move from the composite rather than the "
                             "active layer alone. The result is still written to the "
                             "active layer"));
    m_optionsBar->addWidget(sampleAll);
    connect(sampleAll, &QCheckBox::toggled, this, [this](bool on) {
        m_camSampleAllLayers = on;
        pushContentAwareMoveOptions();
    });

    // CS6's T: show transform handles on the dropped region. Transforms are not
    // implemented, so this is present for the bar's shape but disabled.
    auto *transform = new QCheckBox(tr("T"), m_optionsBar);
    transform->setEnabled(false);
    transform->setToolTip(tr("Not implemented yet: transform on drop needs the transform "
                             "tools"));
    m_optionsBar->addWidget(transform);

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(
        tr("Drag to outline a subject, then drag it where it should go"), m_optionsBar));

    pushContentAwareMoveOptions();
}

void MainWindow::pushContentAwareMoveOptions()
{
    m_canvas->setContentAwareMoveOptions(m_camExtend, m_camStructure, m_camColor,
                                        m_camSampleAllLayers);
}

void MainWindow::addPatchOptions()
{
    // CS6's Patch bar, left to right: the selection combine buttons, the Patch
    // mode, the Source/Destination pair, Transparent, and Use Pattern.
    addSelectionModeButtons();

    m_optionsBar->addWidget(new QLabel(tr("Patch:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    mode->addItem(tr("Normal"), false);
    mode->addItem(tr("Content-Aware"), true);
    mode->setCurrentIndex(m_patchContentAware ? 1 : 0);
    mode->setToolTip(tr("Normal samples from where you drag; Content-Aware rebuilds the "
                        "selection from its surroundings and ignores the drag"));
    m_optionsBar->addWidget(mode);

    m_optionsBar->addSeparator();

    // Source and Destination are a radio pair, drawn as two buttons.
    auto *direction = new QButtonGroup(m_optionsBar);
    direction->setExclusive(true);
    QToolButton *sourceButton = nullptr;
    QToolButton *destinationButton = nullptr;
    struct Entry {
        QString label;
        bool destination;
        QString tip;
    };
    const Entry entries[] = {
        {tr("Source"), false,
         tr("The selection is the flaw; drag it onto the pixels to repair it with")},
        {tr("Destination"), true,
         tr("The selection is good material; drag it onto the area to repair")},
    };
    for (const Entry &entry : entries) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setText(entry.label);
        button->setToolTip(entry.tip);
        button->setStatusTip(entry.tip);
        button->setChecked(entry.destination == m_patchDestination);
        direction->addButton(button, entry.destination ? 1 : 0);
        m_optionsBar->addWidget(button);
        if (entry.destination) {
            destinationButton = button;
        } else {
            sourceButton = button;
        }
    }
    connect(direction, &QButtonGroup::idClicked, this, [this](int id) {
        m_patchDestination = id == 1;
        pushPatchOptions();
    });

    m_optionsBar->addSeparator();

    auto *transparent = new QCheckBox(tr("Transparent"), m_optionsBar);
    transparent->setChecked(m_patchTransparent);
    transparent->setToolTip(tr("Transfer only the source's texture, keeping the patched "
                               "area's own colour"));
    m_optionsBar->addWidget(transparent);
    connect(transparent, &QCheckBox::toggled, this, [this](bool on) {
        m_patchTransparent = on;
        pushPatchOptions();
    });

    // No pattern support yet, so this is present for CS6's shape but disabled —
    // the same treatment the unimplemented flyout entries get.
    auto *usePattern = new QToolButton(m_optionsBar);
    usePattern->setText(tr("Use Pattern"));
    usePattern->setEnabled(false);
    usePattern->setToolTip(tr("Not implemented yet: there are no patterns to fill with"));
    m_optionsBar->addWidget(usePattern);

    // Source, Destination and Transparent all describe how to sample from the
    // drag, and Content-Aware does not sample at all — so they switch off
    // together rather than sitting there having no effect.
    const auto syncEnabled = [sourceButton, destinationButton, transparent](bool contentAware) {
        sourceButton->setEnabled(!contentAware);
        destinationButton->setEnabled(!contentAware);
        transparent->setEnabled(!contentAware);
    };
    syncEnabled(m_patchContentAware);
    connect(mode, &QComboBox::currentIndexChanged, this,
            [this, mode, syncEnabled](int index) {
                m_patchContentAware = mode->itemData(index).toBool();
                syncEnabled(m_patchContentAware);
                pushPatchOptions();
            });

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(
        tr("Drag to outline a region, then drag the outline to patch"), m_optionsBar));

    pushPatchOptions();
}

void MainWindow::pushPatchOptions()
{
    m_canvas->setPatchOptions(m_patchContentAware, m_patchDestination, m_patchTransparent);
}

void MainWindow::warnLayerLocked()
{
    // Photoshop's own wording, naming the tool that was refused, with its error
    // icon. A modal alert rather than a status-bar line, because the click the
    // user just made did nothing at all.
    const QString tool = toolVariantName(m_activeTool, m_activeVariant).toLower();
    QMessageBox box(QMessageBox::Critical, tr("PhotoRust"),
                    tr("Could not use the %1 because the layer is locked.").arg(tool),
                    QMessageBox::Ok, this);
    unsqueezeButtons(&box);
    box.exec();
}

void MainWindow::warnHealingSourceRequired()
{
    // Photoshop's own wording and its error icon. The Healing Brush cannot
    // guess where to repair from, so this is a hard stop rather than a hint in
    // the status bar.
#ifdef Q_OS_MACOS
    const QString message = tr("Option-click to define a source point to be used to "
                               "repair the image.");
#else
    const QString message = tr("Alt-click to define a source point to be used to "
                               "repair the image.");
#endif
    QMessageBox box(QMessageBox::Critical, tr("PhotoRust"), message, QMessageBox::Ok, this);
    unsqueezeButtons(&box);
    box.exec();
}

void MainWindow::addHealingRegionOptions(HealingType type)
{
    switch (type) {
    case HealingType::Patch:
        addPatchOptions();
        break;

    case HealingType::ContentAwareMove:
        addContentAwareMoveOptions();
        break;

    case HealingType::RedEye: {
        struct Field {
            QString label;
            int *value;
        };
        const Field fields[] = {
            {tr("Pupil Size:"), &m_pupilSize},
            {tr("Darken Amount:"), &m_darkenAmount},
        };
        for (const Field &field : fields) {
            m_optionsBar->addWidget(new QLabel(field.label, m_optionsBar));
            auto *spin = new QSpinBox(m_optionsBar);
            spin->setRange(0, 100);
            spin->setValue(*field.value);
            spin->setSuffix(QStringLiteral("%"));
            spin->setFixedWidth(64);
            m_optionsBar->addWidget(spin);

            int *slot = field.value;
            connect(spin, &QSpinBox::valueChanged, this, [this, slot](int v) {
                *slot = v;
                m_canvas->setRedEyeOptions(m_pupilSize, m_darkenAmount);
            });
        }
        m_canvas->setRedEyeOptions(m_pupilSize, m_darkenAmount);

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(
            new QLabel(tr("Drag over an eye, or click it"), m_optionsBar));
        break;
    }

    case HealingType::SpotHealing:
    case HealingType::Healing:
        // Handled with the brush controls.
        break;
    }
}

void MainWindow::addColorReplaceOptions()
{
    // CS6's bar: Mode, the three Sampling buttons, Limits, Tolerance and
    // Anti-alias. Opacity and Flow have already been added above.
    struct Choice {
        QString label;
        int value;
        QString tip;
    };

    const auto addChoices = [this](const QString &label, const QList<Choice> &choices,
                                   int *slot) {
        m_optionsBar->addWidget(new QLabel(label, m_optionsBar));
        auto *combo = new QComboBox(m_optionsBar);
        for (const Choice &choice : choices) {
            combo->addItem(choice.label, choice.value);
            combo->setItemData(combo->count() - 1, choice.tip, Qt::ToolTipRole);
            if (choice.value == *slot) {
                combo->setCurrentIndex(combo->count() - 1);
            }
        }
        m_optionsBar->addWidget(combo);
        connect(combo, &QComboBox::currentIndexChanged, this, [this, combo, slot](int index) {
            *slot = combo->itemData(index).toInt();
            pushColorReplaceOptions();
        });
    };

    addChoices(tr("Mode:"),
               {{tr("Hue"), int(ReplaceMode::Hue), tr("Replace the hue alone")},
                {tr("Saturation"), int(ReplaceMode::Saturation),
                 tr("Replace how saturated the pixel is")},
                {tr("Color"), int(ReplaceMode::Color),
                 tr("Replace hue and saturation, keeping the pixel's brightness — so "
                    "shading survives")},
                {tr("Luminosity"), int(ReplaceMode::Luminosity),
                 tr("Replace the brightness, keeping the pixel's colour")}},
               &m_replaceMode);

    m_optionsBar->addSeparator();

    addChoices(tr("Sampling:"),
               {{tr("Continuous"), int(ReplaceSampling::Continuous),
                 tr("Re-read the colour under the brush as it moves")},
                {tr("Once"), int(ReplaceSampling::Once),
                 tr("Read the colour where the stroke begins, and keep it")},
                {tr("Background Swatch"), int(ReplaceSampling::BackgroundSwatch),
                 tr("Replace whatever matches the background colour")}},
               &m_replaceSampling);

    addChoices(tr("Limits:"),
               {{tr("Discontiguous"), int(ReplaceLimits::Discontiguous),
                 tr("Every matching pixel under the brush")},
                {tr("Contiguous"), int(ReplaceLimits::Contiguous),
                 tr("Only pixels joined to the one under the cursor")},
                {tr("Find Edges"), int(ReplaceLimits::FindEdges),
                 tr("As contiguous, but stopping at edges so colour does not leak "
                    "across a boundary")}},
               &m_replaceLimits);

    m_optionsBar->addWidget(new QLabel(tr("Tolerance:"), m_optionsBar));
    auto *tolerance = new QSpinBox(m_optionsBar);
    tolerance->setRange(0, 100);
    tolerance->setValue(m_replaceTolerance);
    tolerance->setSuffix(QStringLiteral("%"));
    tolerance->setFixedWidth(64);
    tolerance->setToolTip(tr("How far a pixel may differ from the sampled colour and "
                             "still be replaced"));
    m_optionsBar->addWidget(tolerance);
    connect(tolerance, &QSpinBox::valueChanged, this, [this](int v) {
        m_replaceTolerance = v;
        pushColorReplaceOptions();
    });

    auto *antialias = new QCheckBox(tr("Anti-alias"), m_optionsBar);
    antialias->setChecked(m_replaceAntialias);
    antialias->setToolTip(tr("Soften the edge of the replaced area"));
    m_optionsBar->addWidget(antialias);
    connect(antialias, &QCheckBox::toggled, this, [this](bool on) {
        m_replaceAntialias = on;
        pushColorReplaceOptions();
    });

    pushColorReplaceOptions();
}

void MainWindow::pushColorReplaceOptions()
{
    if (!m_engine) {
        return;
    }
    // CS6 shows Tolerance as a percentage; the engine matches per channel in
    // 0-255.
    const int tolerance = qRound(m_replaceTolerance * 255.0 / 100.0);
    m_engine->setReplaceOptions(m_replaceMode, m_replaceSampling, m_replaceLimits, tolerance,
                                m_replaceAntialias);
}

void MainWindow::addMixerOptions()
{
    // CS6's bar, left to right after the tip: the load swatch with its Load and
    // Clean menu, the two after-each-stroke toggles, the Wet/Load/Mix preset
    // menu, the four sliders themselves, and Sample All Layers.
    m_mixerLoadButton = new QToolButton(m_optionsBar);
    m_mixerLoadButton->setPopupMode(QToolButton::InstantPopup);
    m_mixerLoadButton->setIconSize(QSize(20, 20));
    m_mixerLoadButton->setToolTip(tr("The paint currently on the brush"));

    auto *loadMenu = new QMenu(m_mixerLoadButton);
    QAction *loadPaint = loadMenu->addAction(tr("Load Brush"));
    loadPaint->setToolTip(tr("Fill the brush with the foreground colour"));
    QAction *cleanBrush = loadMenu->addAction(tr("Clean Brush"));
    cleanBrush->setToolTip(tr("Empty the brush, so a wet one smears without adding "
                              "colour of its own"));
    m_mixerLoadButton->setMenu(loadMenu);
    m_optionsBar->addWidget(m_mixerLoadButton);
    connect(loadPaint, &QAction::triggered, this, [this] {
        if (m_engine) {
            m_engine->loadMixerBrush();
            refreshMixerLoadSwatch();
        }
    });
    connect(cleanBrush, &QAction::triggered, this, [this] {
        if (m_engine) {
            m_engine->cleanMixerBrush();
            refreshMixerLoadSwatch();
        }
    });
    refreshMixerLoadSwatch();

    struct Toggle {
        QString label;
        QString tip;
        bool *value;
    };
    const Toggle toggles[] = {
        {tr("Load After Stroke"), tr("Refill the brush from the foreground colour when "
                                     "each stroke ends"),
         &m_mixerLoadAfterStroke},
        {tr("Clean After Stroke"), tr("Empty the brush when each stroke ends, so the "
                                      "next one starts with no paint of its own"),
         &m_mixerCleanAfterStroke},
    };
    for (const Toggle &toggle : toggles) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setText(toggle.label);
        button->setToolTip(toggle.tip);
        button->setStatusTip(toggle.tip);
        button->setChecked(*toggle.value);
        m_optionsBar->addWidget(button);

        bool *slot = toggle.value;
        connect(button, &QToolButton::toggled, this, [this, slot](bool on) {
            *slot = on;
            pushMixerOptions();
        });
    }

    m_optionsBar->addSeparator();

    m_mixerPresetCombo = new QComboBox(m_optionsBar);
    for (const MixerPreset &preset : mixerPresets()) {
        m_mixerPresetCombo->addItem(tr(preset.name));
    }
    m_mixerPresetCombo->setToolTip(tr("Ready-made Wet, Load and Mix combinations"));
    m_optionsBar->addWidget(m_mixerPresetCombo);
    syncMixerPresetCombo();
    connect(m_mixerPresetCombo, &QComboBox::activated, this, [this](int index) {
        const QList<MixerPreset> &presets = mixerPresets();
        if (index < 0 || index >= presets.size() || presets[index].wet < 0) {
            // Custom carries no values: choosing it changes nothing, it is only
            // what the menu falls back to once a slider is moved by hand.
            return;
        }
        m_mixerWet = presets[index].wet;
        m_mixerLoad = presets[index].load;
        m_mixerMix = presets[index].mix;
        // The sliders are rebuilt from the new values rather than updated one by
        // one, which keeps this from having to hold three more pointers.
        populateOptionsBar(m_activeTool, m_activeVariant);
    });

    struct Field {
        QString label;
        QString tip;
        int *value;
    };
    const Field fields[] = {
        {tr("Wet:"), tr("How wet the canvas is: at 0 the paint sits on top like an "
                        "ordinary brush, higher and the colour already there joins in"),
         &m_mixerWet},
        {tr("Load:"), tr("How much paint the brush holds. It runs down as the stroke "
                         "goes, and a wet brush that has run out only smears"),
         &m_mixerLoad},
        {tr("Mix:"), tr("The balance between the canvas colour and the brush's own: at "
                        "100% the stroke is a pure smear"),
         &m_mixerMix},
        {tr("Flow:"), tr("How fast each dab deposits"), &m_mixerFlow},
    };
    QSpinBox *spins[4] = {};
    int index = 0;
    for (const Field &field : fields) {
        m_optionsBar->addWidget(new QLabel(field.label, m_optionsBar));
        auto *spin = new QSpinBox(m_optionsBar);
        spin->setRange(0, 100);
        spin->setValue(*field.value);
        spin->setSuffix(QStringLiteral("%"));
        spin->setFixedWidth(64);
        spin->setToolTip(field.tip);
        m_optionsBar->addWidget(spin);
        spins[index++] = spin;

        int *slot = field.value;
        connect(spin, &QSpinBox::valueChanged, this, [this, slot](int v) {
            *slot = v;
            pushMixerOptions();
            syncMixerPresetCombo();
        });
    }

    // Load and Mix have no say while the canvas is dry: a dry brush paints its
    // own paint, never runs out and has nothing to mix with. CS6 greys them out
    // rather than letting them look effective.
    const auto followWet = [spins](int wet) {
        spins[1]->setEnabled(wet > 0);
        spins[2]->setEnabled(wet > 0);
    };
    followWet(m_mixerWet);
    connect(spins[0], &QSpinBox::valueChanged, this,
            [followWet](int wet) { followWet(wet); });

    m_optionsBar->addSeparator();

    auto *sampleAll = new QCheckBox(tr("Sample All Layers"), m_optionsBar);
    sampleAll->setChecked(m_mixerSampleAllLayers);
    sampleAll->setToolTip(tr("Pick colour up from every visible layer. The paint still "
                             "lands on the active one"));
    m_optionsBar->addWidget(sampleAll);
    connect(sampleAll, &QCheckBox::toggled, this, [this](bool on) {
        m_mixerSampleAllLayers = on;
        pushMixerOptions();
    });

    pushMixerOptions();
}

void MainWindow::pushMixerOptions()
{
    if (!m_engine) {
        return;
    }
    m_engine->setMixerOptions(m_mixerWet, m_mixerLoad, m_mixerMix, m_mixerFlow,
                              m_mixerSampleAllLayers, m_mixerLoadAfterStroke,
                              m_mixerCleanAfterStroke);
}

void MainWindow::refreshMixerLoadSwatch()
{
    if (!m_mixerLoadButton || !m_engine) {
        return;
    }
    const QColor paint = m_engine->mixerLoadColor();

    // A clean brush is drawn as the checkerboard alone, so "no paint" reads
    // differently from "loaded with white".
    QPixmap swatch(20, 20);
    QPainter painter(&swatch);
    const QColor checks[2] = {QColor(0xcc, 0xcc, 0xcc), QColor(0xff, 0xff, 0xff)};
    for (int y = 0; y < 20; y += 5) {
        for (int x = 0; x < 20; x += 5) {
            painter.fillRect(x, y, 5, 5, checks[((x / 5) + (y / 5)) % 2]);
        }
    }
    painter.fillRect(swatch.rect(), paint);
    painter.setPen(QColor(0x1a, 0x1a, 0x1a));
    painter.drawRect(0, 0, 19, 19);
    painter.end();

    m_mixerLoadButton->setIcon(QIcon(swatch));
}

void MainWindow::syncMixerPresetCombo()
{
    if (!m_mixerPresetCombo) {
        return;
    }
    const QList<MixerPreset> &presets = mixerPresets();
    int match = 0; // Custom, unless one of the presets matches exactly.
    for (int i = 0; i < presets.size(); ++i) {
        if (presets[i].wet == m_mixerWet && presets[i].load == m_mixerLoad
            && presets[i].mix == m_mixerMix) {
            match = i;
            break;
        }
    }
    const QSignalBlocker blocker(m_mixerPresetCombo);
    m_mixerPresetCombo->setCurrentIndex(match);
}

void MainWindow::addToneOptions(ToneTool tool)
{
    m_optionsBar->addSeparator();

    const bool sponge = tool == ToneTool::Sponge;

    // Dodge and Burn choose a tonal band; the Sponge chooses a direction. CS6
    // puts each in the same place on the bar.
    m_optionsBar->addWidget(new QLabel(sponge ? tr("Mode:") : tr("Range:"), m_optionsBar));
    auto *choice = new QComboBox(m_optionsBar);
    struct Entry {
        QString label;
        int value;
        QString tip;
    };
    const QList<Entry> entries = sponge
        ? QList<Entry>{{tr("Desaturate"), int(SpongeMode::Desaturate),
                        tr("Drain colour toward grey")},
                       {tr("Saturate"), int(SpongeMode::Saturate),
                        tr("Lift colour away from grey")}}
        : QList<Entry>{{tr("Shadows"), int(ToneRange::Shadows),
                        tr("Work hardest on the dark end of the scale")},
                       {tr("Midtones"), int(ToneRange::Midtones),
                        tr("Work hardest on the middle of the scale")},
                       {tr("Highlights"), int(ToneRange::Highlights),
                        tr("Work hardest on the bright end of the scale")}};
    for (const Entry &entry : entries) {
        choice->addItem(entry.label, entry.value);
        choice->setItemData(choice->count() - 1, entry.tip, Qt::ToolTipRole);
        if (entry.value == (sponge ? m_spongeMode : m_toneRange)) {
            choice->setCurrentIndex(choice->count() - 1);
        }
    }
    m_optionsBar->addWidget(choice);
    connect(choice, &QComboBox::currentIndexChanged, this, [this, choice, sponge](int index) {
        (sponge ? m_spongeMode : m_toneRange) = choice->itemData(index).toInt();
        pushToneOptions();
    });

    // One number under two names, exactly as CS6 labels it.
    m_optionsBar->addWidget(new QLabel(sponge ? tr("Flow:") : tr("Exposure:"), m_optionsBar));
    auto *amount = new QSpinBox(m_optionsBar);
    amount->setRange(0, 100);
    amount->setValue(m_toneAmount);
    amount->setSuffix(QStringLiteral("%"));
    amount->setFixedWidth(64);
    amount->setToolTip(sponge
                           ? tr("How much colour each dab moves. Dwelling on one spot goes "
                                "on moving it")
                           : tr("How much each dab lightens or darkens. Dwelling on one "
                                "spot goes on working it"));
    m_optionsBar->addWidget(amount);
    connect(amount, &QSpinBox::valueChanged, this, [this](int v) {
        m_toneAmount = v;
        pushToneOptions();
    });

    m_optionsBar->addSeparator();

    if (sponge) {
        auto *vibrance = new QCheckBox(tr("Vibrance"), m_optionsBar);
        vibrance->setChecked(m_toneVibrance);
        vibrance->setToolTip(tr("Ease off on colour that is already vivid, so the flat "
                                "parts of an image lift without the vivid parts clipping"));
        m_optionsBar->addWidget(vibrance);
        connect(vibrance, &QCheckBox::toggled, this, [this](bool on) {
            m_toneVibrance = on;
            pushToneOptions();
        });
    } else {
        auto *protect = new QCheckBox(tr("Protect Tones"), m_optionsBar);
        protect->setChecked(m_toneProtectTones);
        protect->setToolTip(tr("Change brightness and keep the pixel's own colour, rather "
                               "than scaling its channels — which is what bleaches a "
                               "dodge and muddies a burn"));
        m_optionsBar->addWidget(protect);
        connect(protect, &QCheckBox::toggled, this, [this](bool on) {
            m_toneProtectTones = on;
            pushToneOptions();
        });
    }

    pushToneOptions();
}

void MainWindow::pushToneOptions()
{
    if (!m_engine) {
        return;
    }
    m_engine->setToneOptions(m_toneAmount, m_toneRange, m_spongeMode, m_toneProtectTones,
                             m_toneVibrance);
}

void MainWindow::addBlurOptions(BlurTool tool)
{
    m_optionsBar->addSeparator();

    m_optionsBar->addWidget(new QLabel(tr("Mode:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    // CS6 offers a cut-down list here, not all 27: see `blurModes()`.
    for (const auto &entry : blurModes()) {
        mode->addItem(entry.first, entry.second);
        if (entry.second == m_blurMode) {
            mode->setCurrentIndex(mode->count() - 1);
        }
    }
    m_optionsBar->addWidget(mode);
    connect(mode, &QComboBox::currentIndexChanged, this, [this, mode](int index) {
        m_blurMode = mode->itemData(index).toInt();
        pushBlurOptions();
    });

    m_optionsBar->addWidget(new QLabel(tr("Strength:"), m_optionsBar));
    auto *strength = new QSpinBox(m_optionsBar);
    strength->setRange(0, 100);
    strength->setValue(m_blurStrength);
    strength->setSuffix(QStringLiteral("%"));
    strength->setFixedWidth(64);
    switch (tool) {
    case BlurTool::Blur:
        strength->setToolTip(tr("How much each dab softens. Dwelling on one spot goes on "
                                "softening it"));
        break;
    case BlurTool::Sharpen:
        strength->setToolTip(tr("How much each dab sharpens. Dwelling on one spot goes on "
                                "sharpening it"));
        break;
    case BlurTool::Smudge:
        strength->setToolTip(tr("How much of what the finger is carrying each dab lays "
                                "down. At 100% it drags the pixels it started on the "
                                "whole way"));
        break;
    }
    m_optionsBar->addWidget(strength);
    connect(strength, &QSpinBox::valueChanged, this, [this](int v) {
        m_blurStrength = v;
        pushBlurOptions();
    });

    m_optionsBar->addSeparator();

    auto *sampleAll = new QCheckBox(tr("Sample All Layers"), m_optionsBar);
    sampleAll->setChecked(m_blurSampleAllLayers);
    sampleAll->setToolTip(tr("Work on what the whole visible image shows. The result "
                             "still lands on the active layer"));
    m_optionsBar->addWidget(sampleAll);
    connect(sampleAll, &QCheckBox::toggled, this, [this](bool on) {
        m_blurSampleAllLayers = on;
        pushBlurOptions();
    });

    // Each of the two has one checkbox of its own, which CS6 shows only for it.
    if (tool == BlurTool::Sharpen) {
        auto *protect = new QCheckBox(tr("Protect Detail"), m_optionsBar);
        protect->setChecked(m_blurProtectDetail);
        protect->setToolTip(tr("Hold each pixel inside the range its own neighbours span, "
                               "so repeated passes cannot throw haloes or blown speckle"));
        m_optionsBar->addWidget(protect);
        connect(protect, &QCheckBox::toggled, this, [this](bool on) {
            m_blurProtectDetail = on;
            pushBlurOptions();
        });
    } else if (tool == BlurTool::Smudge) {
        auto *finger = new QCheckBox(tr("Finger Painting"), m_optionsBar);
        finger->setChecked(m_blurFingerPainting);
        finger->setToolTip(tr("Start each stroke with the foreground colour on the finger, "
                              "so it drags paint in rather than what was already there"));
        m_optionsBar->addWidget(finger);
        connect(finger, &QCheckBox::toggled, this, [this](bool on) {
            m_blurFingerPainting = on;
            pushBlurOptions();
        });
    }

    pushBlurOptions();
}

void MainWindow::pushBlurOptions()
{
    if (!m_engine) {
        return;
    }
    m_engine->setFocusOptions(m_blurStrength, m_blurMode, m_blurSampleAllLayers,
                              m_blurProtectDetail, m_blurFingerPainting);
}

void MainWindow::addBucketOptions()
{
    // CS6's bar: Fill, Mode, Opacity, Tolerance, then Anti-alias, Contiguous and
    // All Layers.
    m_optionsBar->addWidget(new QLabel(tr("Fill:"), m_optionsBar));
    auto *fill = new QComboBox(m_optionsBar);
    fill->addItem(tr("Foreground"), int(BucketFill::Foreground));
    fill->addItem(tr("Pattern"), int(BucketFill::Pattern));
    // Patterns are a sub-system of their own and there are none to fill with, so
    // the entry is listed for the bar's shape and disabled — the same treatment
    // the Patch tool's Use Pattern button gets.
    fill->setItemData(1, false, Qt::UserRole - 1);
    fill->setItemData(1, tr("Not implemented yet: there are no patterns to fill with"),
                      Qt::ToolTipRole);
    m_optionsBar->addWidget(fill);

    m_optionsBar->addWidget(new QLabel(tr("Mode:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    mode->addItems(m_engine->blendModeNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts));
    mode->setCurrentIndex(m_bucketMode);
    m_optionsBar->addWidget(mode);
    connect(mode, &QComboBox::currentIndexChanged, this, [this](int index) {
        m_bucketMode = index;
        pushBucketOptions();
    });

    struct Field {
        QString label;
        QString suffix;
        int max;
        QString tip;
        int *value;
    };
    const Field fields[] = {
        {tr("Opacity:"), QStringLiteral("%"), 100, tr("How strongly the fill is applied"),
         &m_bucketOpacity},
        {tr("Tolerance:"), QString(), 255, tr("How far a pixel may differ from the one "
                                              "clicked and still be filled — the same "
                                              "scale as the Magic Wand's"),
         &m_bucketTolerance},
    };
    for (const Field &field : fields) {
        m_optionsBar->addWidget(new QLabel(field.label, m_optionsBar));
        auto *spin = new QSpinBox(m_optionsBar);
        spin->setRange(0, field.max);
        spin->setValue(*field.value);
        spin->setSuffix(field.suffix);
        spin->setFixedWidth(64);
        spin->setToolTip(field.tip);
        m_optionsBar->addWidget(spin);

        int *slot = field.value;
        connect(spin, &QSpinBox::valueChanged, this, [this, slot](int v) {
            *slot = v;
            pushBucketOptions();
        });
    }

    m_optionsBar->addSeparator();

    struct Toggle {
        QString label;
        QString tip;
        bool *value;
    };
    const Toggle toggles[] = {
        {tr("Anti-alias"), tr("Soften the edge of the filled area"), &m_bucketAntialias},
        {tr("Contiguous"), tr("Fill only the area joined to the pixel clicked. Off, every "
                              "matching pixel in the layer is filled"),
         &m_bucketContiguous},
        {tr("All Layers"), tr("Decide what matches from the whole visible image. The fill "
                              "still lands on the active layer"),
         &m_bucketAllLayers},
    };
    for (const Toggle &toggle : toggles) {
        auto *box = new QCheckBox(toggle.label, m_optionsBar);
        box->setChecked(*toggle.value);
        box->setToolTip(toggle.tip);
        m_optionsBar->addWidget(box);

        bool *slot = toggle.value;
        connect(box, &QCheckBox::toggled, this, [this, slot](bool on) {
            *slot = on;
            pushBucketOptions();
        });
    }

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(
        new QLabel(tr("Click to fill an area with the foreground colour"), m_optionsBar));

    pushBucketOptions();
}

void MainWindow::pushBucketOptions()
{
    if (!m_engine) {
        return;
    }
    m_engine->setBucketOptions(m_bucketMode, m_bucketOpacity, m_bucketTolerance,
                               m_bucketAntialias, m_bucketContiguous, m_bucketAllLayers);
}

void MainWindow::addGradientOptions()
{
    // CS6's bar: the gradient swatch and its preset menu, the five type buttons,
    // Mode, Opacity, then Reverse, Dither and Transparency.
    m_gradientSwatch = new QToolButton(m_optionsBar);
    m_gradientSwatch->setObjectName(QStringLiteral("gradientSwatch"));
    m_gradientSwatch->setPopupMode(QToolButton::InstantPopup);
    m_gradientSwatch->setIconSize(QSize(64, 16));
    m_gradientSwatch->setToolTip(tr("Click to choose a gradient"));

    auto *presets = new QMenu(m_gradientSwatch);
    const QStringList names =
        m_engine->gradientPresetNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    if (m_gradientPreset.isEmpty() && !names.isEmpty()) {
        m_gradientPreset = names.first();
    }
    for (const QString &name : names) {
        // The preview comes from the engine, so what the menu shows and what the
        // tool paints cannot drift apart.
        QAction *action = presets->addAction(QIcon(QPixmap::fromImage(
                                                m_engine->gradientPreview(name, 64, 16))),
                                            name);
        action->setCheckable(true);
        action->setChecked(name == m_gradientPreset);
        connect(action, &QAction::triggered, this, [this, name] {
            m_gradientPreset = name;
            refreshGradientSwatch();
            pushGradientOptions();
        });
    }
    m_gradientSwatch->setMenu(presets);
    m_optionsBar->addWidget(m_gradientSwatch);
    refreshGradientSwatch();

    m_optionsBar->addSeparator();

    auto *types = new QButtonGroup(m_optionsBar);
    types->setExclusive(true);
    for (GradientType type : {GradientType::Linear, GradientType::Radial,
                              GradientType::Angle, GradientType::Reflected,
                              GradientType::Diamond}) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setIconSize(QSize(20, 20));
        button->setIcon(ToolIcons::fromSvgBody(ToolIcons::gradientTypeSvg(type),
                                               QColor(0x30, 0x30, 0x30)));
        button->setToolTip(gradientTypeName(type));
        button->setStatusTip(gradientTypeName(type));
        button->setChecked(int(type) == m_gradientType);
        types->addButton(button, int(type));
        m_optionsBar->addWidget(button);
    }
    connect(types, &QButtonGroup::idClicked, this, [this](int id) {
        m_gradientType = id;
        pushGradientOptions();
    });

    m_optionsBar->addSeparator();

    m_optionsBar->addWidget(new QLabel(tr("Mode:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    // The engine owns the blend-mode list and its order, exactly as it does for
    // the Layers panel.
    mode->addItems(m_engine->blendModeNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts));
    mode->setCurrentIndex(m_gradientMode);
    m_optionsBar->addWidget(mode);
    connect(mode, &QComboBox::currentIndexChanged, this, [this](int index) {
        m_gradientMode = index;
        pushGradientOptions();
    });

    m_optionsBar->addWidget(new QLabel(tr("Opacity:"), m_optionsBar));
    auto *opacity = new QSpinBox(m_optionsBar);
    opacity->setRange(0, 100);
    opacity->setValue(m_gradientOpacity);
    opacity->setSuffix(QStringLiteral("%"));
    opacity->setFixedWidth(64);
    m_optionsBar->addWidget(opacity);
    connect(opacity, &QSpinBox::valueChanged, this, [this](int v) {
        m_gradientOpacity = v;
        pushGradientOptions();
    });

    m_optionsBar->addSeparator();

    struct Toggle {
        QString label;
        QString tip;
        bool *value;
    };
    const Toggle toggles[] = {
        {tr("Reverse"), tr("Draw the ramp end to start"), &m_gradientReverse},
        {tr("Dither"), tr("Break up banding with a little noise"), &m_gradientDither},
        {tr("Transparency"), tr("Honour the gradient's own opacity. Off, it is drawn "
                                "solid"),
         &m_gradientTransparency},
    };
    for (const Toggle &toggle : toggles) {
        auto *box = new QCheckBox(toggle.label, m_optionsBar);
        box->setChecked(*toggle.value);
        box->setToolTip(toggle.tip);
        m_optionsBar->addWidget(box);

        bool *slot = toggle.value;
        connect(box, &QCheckBox::toggled, this, [this, slot](bool on) {
            *slot = on;
            pushGradientOptions();
        });
    }

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(
        tr("Drag to set the gradient's direction and length    Shift constrains the angle"),
        m_optionsBar));

    pushGradientOptions();
}

void MainWindow::pushGradientOptions()
{
    if (!m_engine) {
        return;
    }
    m_engine->setGradientOptions(m_gradientPreset, m_gradientType, m_gradientMode,
                                 m_gradientOpacity, m_gradientReverse, m_gradientDither,
                                 m_gradientTransparency);
}

void MainWindow::refreshGradientSwatch()
{
    if (!m_gradientSwatch || !m_engine) {
        return;
    }
    m_gradientSwatch->setIcon(
        QIcon(QPixmap::fromImage(m_engine->gradientPreview(m_gradientPreset, 64, 16))));
    m_gradientSwatch->setToolTip(tr("%1 — click to choose a gradient").arg(m_gradientPreset));
}

void MainWindow::addPenOptions(PenTool tool)
{
    m_optionsBar->addSeparator();

    if (tool == PenTool::Pen) {
        auto *autoAddDelete = new QCheckBox(tr("Auto Add/Delete"), m_optionsBar);
        autoAddDelete->setChecked(m_penAutoAddDelete);
        autoAddDelete->setToolTip(tr("Hovering the finished part of the path adds an anchor "
                                     "over a segment, or removes one under the cursor, "
                                     "without switching tools"));
        m_optionsBar->addWidget(autoAddDelete);
        connect(autoAddDelete, &QCheckBox::toggled, this, [this](bool on) {
            m_penAutoAddDelete = on;
            pushPenOptions();
        });

        auto *rubberBand = new QCheckBox(tr("Rubber Band"), m_optionsBar);
        rubberBand->setChecked(m_penRubberBand);
        rubberBand->setToolTip(tr("Preview the next segment from the last anchor to the "
                                  "cursor, before it is placed"));
        m_optionsBar->addWidget(rubberBand);
        connect(rubberBand, &QCheckBox::toggled, this, [this](bool on) {
            m_penRubberBand = on;
            pushPenOptions();
        });

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(new QLabel(
            tr("Click for a corner, drag for a curve    Click the start to close    "
               "Enter or double-click to finish"),
            m_optionsBar));
    } else if (tool == PenTool::FreeformPen) {
        m_optionsBar->addWidget(new QLabel(tr("Curve Fit:"), m_optionsBar));
        auto *fit = new QDoubleSpinBox(m_optionsBar);
        fit->setRange(0.5, 10.0);
        fit->setSingleStep(0.5);
        fit->setSuffix(tr(" px"));
        fit->setValue(m_freeformTolerance);
        fit->setFixedWidth(72);
        fit->setToolTip(tr("How closely the fitted path follows the drag. Lower values keep "
                           "more anchors"));
        m_optionsBar->addWidget(fit);
        connect(fit, &QDoubleSpinBox::valueChanged, this, [this](double v) {
            m_freeformTolerance = v;
            m_canvas->setFreeformPenTolerance(v);
        });

        // CS6's Magnetic checkbox reuses the Magnetic Lasso's edge-snapping.
        // Wiring it up for a *path* — anchors that snap to edges rather than a
        // selection mask — is a real chunk of its own and is not included
        // here.
        auto *magnetic = new QCheckBox(tr("Magnetic"), m_optionsBar);
        magnetic->setEnabled(false);
        magnetic->setToolTip(tr("Not implemented yet"));
        m_optionsBar->addWidget(magnetic);

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(new QLabel(
            tr("Drag to draw freehand    Drag back to the start to close"), m_optionsBar));
    } else {
        const QString hint = tool == PenTool::AddAnchor
            ? tr("Click a segment of the active path to add an anchor there")
            : tool == PenTool::DeleteAnchor
                ? tr("Click an anchor on the active path to remove it")
                : tr("Click a smooth anchor to make it a corner    Drag a corner to pull out "
                     "new handles    Drag a handle to break it free");
        m_optionsBar->addWidget(new QLabel(hint, m_optionsBar));
    }

    pushPenOptions();
}

void MainWindow::pushPenOptions()
{
    m_canvas->setPenOptions(m_penAutoAddDelete, m_penRubberBand);
    m_canvas->setFreeformPenTolerance(m_freeformTolerance);
}

void MainWindow::addTypeOptions()
{
    // The first time this bar is built, start from the current foreground
    // colour — CS6's own default — rather than the black this class is
    // constructed with. After that the user's own choice persists like every
    // other tool option here.
    if (!m_typeColorInitialized && m_engine) {
        m_typeColor = m_engine->foregroundColor();
        m_typeColorInitialized = true;
    }

    // Font family. CS6's own combo previews each entry in its own typeface;
    // FontFamilyDelegate above does the same, rendering the previews off the
    // paint path so the popup opens immediately however many fonts are
    // installed.
    auto *family = new QComboBox(m_optionsBar);
    family->setItemDelegate(new FontFamilyDelegate(family));
    QStringList families;
    for (const QString &name : QFontDatabase::families()) {
        // A colour-emoji font renders its own name as oversized colour
        // glyphs when previewed at text size rather than garbling like an
        // ordinary font would — and CS6 predates system emoji fonts
        // entirely, so leaving them out is truer to the target UI, not just
        // a workaround.
        if (!name.contains(QLatin1String("Emoji"), Qt::CaseInsensitive)) {
            families << name;
        }
    }
    family->addItems(families);
    const int familyIdx = family->findText(m_typeFont.family());
    family->setCurrentIndex(familyIdx >= 0 ? familyIdx : 0);
    family->setFixedWidth(170);
    family->setMaxVisibleItems(20);
    family->setToolTip(tr("Set the font family"));
    // Every row is the same fixed height, so let the view take that as given
    // rather than asking the delegate about each of several hundred entries
    // to work out how tall the popup should be.
    if (auto *view = qobject_cast<QListView *>(family->view())) {
        view->setUniformItemSizes(true);
        view->setLayoutMode(QListView::Batched);
    }
    m_optionsBar->addWidget(family);

    // Style — CS6 keeps this as a second combo beside the family rather than
    // bold/italic toggle buttons. The list comes from what the chosen family
    // actually has, so it never offers a style the font cannot render.
    auto *style = new QComboBox(m_optionsBar);
    style->setFixedWidth(110);
    style->setToolTip(tr("Set the font style"));
    m_optionsBar->addWidget(style);

    auto refreshStyles = [this, style](const QString &familyName) {
        const QSignalBlocker blocker(style);
        style->clear();
        QStringList styles = QFontDatabase::styles(familyName);
        if (styles.isEmpty()) {
            styles << tr("Regular");
        }
        style->addItems(styles);
        const int idx = style->findText(m_typeStyle);
        style->setCurrentIndex(idx >= 0 ? idx : 0);
        m_typeStyle = style->currentText();
    };
    refreshStyles(m_typeFont.family());

    connect(family, &QComboBox::currentTextChanged, this,
            [this, refreshStyles](const QString &familyName) {
                m_typeFont.setFamily(familyName);
                refreshStyles(familyName);
                pushTypeOptions();
            });
    connect(style, &QComboBox::currentTextChanged, this, [this](const QString &text) {
        m_typeStyle = text;
        pushTypeOptions();
    });

    m_optionsBar->addSeparator();

    // Size: CS6's common point sizes, plus room to type any value.
    auto *size = new QComboBox(m_optionsBar);
    size->setEditable(true);
    size->setFixedWidth(64);
    size->setValidator(new QIntValidator(1, 1296, size));
    size->setToolTip(tr("Set the font size"));
    for (int pt : {6, 7, 8, 9, 10, 11, 12, 14, 18, 24, 30, 36, 48, 60, 72, 96, 144, 192, 288}) {
        size->addItem(QString::number(pt));
    }
    size->setCurrentText(QString::number(int(m_typeFont.pointSizeF())));
    m_optionsBar->addWidget(size);
    connect(size, &QComboBox::currentTextChanged, this, [this](const QString &text) {
        bool ok = false;
        const double pt = text.toDouble(&ok);
        if (ok && pt > 0) {
            m_typeFont.setPointSizeF(pt);
            pushTypeOptions();
        }
    });

    m_optionsBar->addSeparator();

    // Anti-aliasing method. CS6 offers five; only None turns Qt's own text
    // antialiasing off — the other four are all a *way* of smoothing, which
    // Qt's rasterizer does not expose a choice between.
    auto *aa = new QComboBox(m_optionsBar);
    aa->addItems({tr("None"), tr("Sharp"), tr("Crisp"), tr("Strong"), tr("Smooth")});
    aa->setCurrentText(m_typeAntialias ? tr("Sharp") : tr("None"));
    aa->setFixedWidth(90);
    aa->setToolTip(tr("Set the anti-aliasing method"));
    m_optionsBar->addWidget(aa);
    connect(aa, &QComboBox::currentTextChanged, this, [this](const QString &text) {
        m_typeAntialias = text != tr("None");
        pushTypeOptions();
    });

    m_optionsBar->addSeparator();

    // Paragraph alignment: left, centre, right.
    auto *alignGroup = new QButtonGroup(m_optionsBar);
    alignGroup->setExclusive(true);
    struct AlignEntry {
        Qt::Alignment align;
        QString tip;
    };
    // Vertical type runs down the page, so the same three buttons mean top,
    // centre and bottom — CS6 turns their icons a quarter turn to say so.
    const AlignEntry aligns[] = {
        {Qt::AlignLeft, m_typeVertical ? tr("Top align text") : tr("Left align text")},
        {Qt::AlignHCenter, tr("Center text")},
        {Qt::AlignRight, m_typeVertical ? tr("Bottom align text") : tr("Right align text")},
    };
    for (const AlignEntry &entry : aligns) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setIcon(ToolIcons::fromSvgBody(
            ToolIcons::textAlignSvg(entry.align, m_typeVertical), kOptionsIconColor));
        button->setIconSize(QSize(20, 20));
        button->setChecked(m_typeAlignment == entry.align);
        button->setToolTip(entry.tip);
        alignGroup->addButton(button, int(entry.align));
        m_optionsBar->addWidget(button);
    }
    connect(alignGroup, &QButtonGroup::idClicked, this, [this](int id) {
        m_typeAlignment = Qt::Alignment(id);
        pushTypeOptions();
    });

    m_optionsBar->addSeparator();

    // Text colour swatch.
    auto *colorSwatch = new QToolButton(m_optionsBar);
    colorSwatch->setFixedSize(22, 22);
    colorSwatch->setToolTip(tr("Set the text colour"));
    auto refreshSwatch = [this, colorSwatch] {
        QPixmap pm(16, 16);
        pm.fill(m_typeColor);
        QPainter p(&pm);
        p.setPen(QColor(0, 0, 0, 160));
        p.drawRect(pm.rect().adjusted(0, 0, -1, -1));
        colorSwatch->setIcon(QIcon(pm));
    };
    refreshSwatch();
    m_optionsBar->addWidget(colorSwatch);
    connect(colorSwatch, &QToolButton::clicked, this, [this, refreshSwatch] {
        const QColor picked = ColorPickerDialog::getColor(m_typeColor, this, tr("Text Color"));
        if (picked.isValid()) {
            m_typeColor = picked;
            refreshSwatch();
            pushTypeOptions();
        }
    });

    m_optionsBar->addSeparator();

    // Warp Text and the Character/Paragraph panel toggle: listed for CS6's
    // shape, neither is implemented — there is no text-warp geometry and no
    // Character/Paragraph panel behind them yet.
    auto *warp = new QToolButton(m_optionsBar);
    warp->setAutoRaise(true);
    warp->setIcon(ToolIcons::fromSvgBody(
        QStringLiteral(R"SVG(<path d="M3 14c3-6 11-6 14 0" stroke-width="1.3"/>
                  <path d="M10 4V11M7 4H13" stroke-width="1.3"/>)SVG"),
        kOptionsIconColor));
    warp->setIconSize(QSize(20, 20));
    warp->setEnabled(false);
    warp->setToolTip(tr("Warp Text — not implemented yet"));
    m_optionsBar->addWidget(warp);

    auto *panels = new QToolButton(m_optionsBar);
    panels->setAutoRaise(true);
    panels->setIcon(ToolIcons::fromSvgBody(
        QStringLiteral(R"SVG(<rect x="3" y="3" width="14" height="14" rx="1" stroke-width="1.2"/>
                  <path d="M3 9H17" stroke-width="1"/>
                  <path d="M6 6H12M6 13H14" stroke-width="1"/>)SVG"),
        kOptionsIconColor));
    panels->setIconSize(QSize(20, 20));
    panels->setEnabled(false);
    panels->setToolTip(tr("Toggle the Character and Paragraph panels — not implemented yet"));
    m_optionsBar->addWidget(panels);

    m_optionsBar->addSeparator();

    // Cancel and commit. CS6 disables these until there is an edit in
    // progress; both of ours are harmless no-ops when there is not, which
    // avoids rebuilding the bar on every keystroke just to toggle them.
    auto *cancel = new QToolButton(m_optionsBar);
    cancel->setAutoRaise(true);
    cancel->setIcon(ToolIcons::fromSvgBody(ToolIcons::cancelSvg(), kOptionsIconColor));
    cancel->setIconSize(QSize(18, 18));
    cancel->setToolTip(tr("Cancel any current edits (Esc)"));
    m_optionsBar->addWidget(cancel);
    connect(cancel, &QToolButton::clicked, this, [this] { m_canvas->cancelTypeEdit(); });

    auto *commit = new QToolButton(m_optionsBar);
    commit->setAutoRaise(true);
    commit->setIcon(ToolIcons::fromSvgBody(ToolIcons::commitSvg(), kOptionsIconColor));
    commit->setIconSize(QSize(18, 18));
    commit->setToolTip(tr("Commit any current edits (Enter)"));
    m_optionsBar->addWidget(commit);
    connect(commit, &QToolButton::clicked, this, [this] { m_canvas->commitTypeEdit(); });

    pushTypeOptions();
}

void MainWindow::pushTypeOptions()
{
    // The style name (e.g. "Bold Italic") comes from the family's own style
    // list rather than QFont's bold/italic bits, so look the concrete font up
    // by name instead of setting those bits and hoping they match.
    QFont resolved = QFontDatabase::font(m_typeFont.family(), m_typeStyle,
                                         int(m_typeFont.pointSizeF()));
    resolved.setPointSizeF(m_typeFont.pointSizeF());
    m_canvas->setTypeOptions(resolved, m_typeStyle, m_typeColor, m_typeAlignment,
                             m_typeAntialias);

    // With a type layer selected and no edit in progress, the bar restyles
    // that layer — Photoshop does not make you click into the text and select
    // it first. The canvas refuses when the layer is not type, or when an edit
    // is running and owns the setting instead.
    if (!m_buildingOptionsBar && !m_canvas->isTyping() && m_engine
        && m_canvas->restyleTypeLayer(m_engine->getActiveLayerIndex())) {
        refreshAll();
    }
}

/// The alignment a stored type record's code stands for — CS6's 0, 1, 2.
static Qt::Alignment typeAlignmentFor(int code)
{
    switch (code) {
    case 1:
        return Qt::AlignHCenter;
    case 2:
        return Qt::AlignRight;
    default:
        return Qt::AlignLeft;
    }
}

void MainWindow::syncTypeBarToActiveLayer()
{
    if (!m_engine || !m_canvas || m_canvas->isTyping() || m_activeTool != ToolId::Type) {
        return;
    }
    const int index = m_engine->getActiveLayerIndex();
    if (index == m_typeBarLayer) {
        return;
    }
    m_typeBarLayer = index;
    if (m_engine->layerTextRunCount(index) <= 0) {
        return;
    }
    // Show what the selected layer is set in, before the bar is used to change
    // it: otherwise touching the size alone would also impose whatever family
    // and colour the bar happened to be left on.
    adoptTypeStyle(m_engine->layerTextRunFamily(index, 0),
                   m_engine->layerTextRunStyle(index, 0),
                   m_engine->layerTextRunSize(index, 0),
                   m_engine->layerTextRunColor(index, 0),
                   typeAlignmentFor(m_engine->layerTextAlign(index)),
                   m_engine->layerTextAntialias(index),
                   m_engine->layerTextVertical(index));
}

void MainWindow::adoptTypeStyle(const QString &family, const QString &style, qreal pointSize,
                                const QColor &color, Qt::Alignment alignment, bool antialias,
                                bool vertical)
{
    // Orientation belongs to the text: reopening vertical type edits it
    // vertically whichever Type tool was in hand, so the bar's alignment
    // buttons have to describe the axis actually being edited.
    m_typeVertical = vertical;
    m_typeFont.setFamily(family);
    m_typeFont.setPointSizeF(pointSize);
    m_typeStyle = style;
    m_typeColor = color;
    // The reopened text's colour is a deliberate choice already made, so it
    // must not be overwritten by the foreground colour the first time the bar
    // is built.
    m_typeColorInitialized = true;
    m_typeAlignment = alignment;
    m_typeAntialias = antialias;

    // Rebuild the bar so its combos, swatch and alignment buttons show what is
    // now being edited. It ends by pushing these same values back to the
    // canvas, which is a no-op — they came from there.
    if (m_activeTool == ToolId::Type) {
        populateOptionsBar(ToolId::Type, m_activeVariant);
    }
}

void MainWindow::addCloneOptions()
{
    m_optionsBar->addSeparator();

    auto *aligned = new QCheckBox(tr("Aligned"), m_optionsBar);
    aligned->setChecked(m_cloneAligned);
    aligned->setToolTip(tr("Keep the sample point travelling with the cursor across "
                           "strokes. Off, every stroke starts copying from the source "
                           "point again"));
    m_optionsBar->addWidget(aligned);
    connect(aligned, &QCheckBox::toggled, this, [this](bool on) {
        m_cloneAligned = on;
        pushCloneOptions();
    });

    m_optionsBar->addWidget(new QLabel(tr("Sample:"), m_optionsBar));
    auto *sample = new QComboBox(m_optionsBar);
    struct Choice {
        QString label;
        CloneSampling value;
        QString tip;
    };
    const Choice choices[] = {
        {tr("Current Layer"), CloneSampling::CurrentLayer,
         tr("Copy from the active layer alone")},
        {tr("Current & Below"), CloneSampling::CurrentAndBelow,
         tr("Copy from the active layer composited with everything beneath it")},
        {tr("All Layers"), CloneSampling::AllLayers,
         tr("Copy from the whole visible image")},
    };
    for (const Choice &choice : choices) {
        sample->addItem(choice.label, int(choice.value));
        sample->setItemData(sample->count() - 1, choice.tip, Qt::ToolTipRole);
        if (int(choice.value) == m_cloneSampling) {
            sample->setCurrentIndex(sample->count() - 1);
        }
    }
    m_optionsBar->addWidget(sample);
    connect(sample, &QComboBox::currentIndexChanged, this, [this, sample](int index) {
        m_cloneSampling = sample->itemData(index).toInt();
        pushCloneOptions();
    });

    m_optionsBar->addSeparator();
#ifdef Q_OS_MACOS
    m_optionsBar->addWidget(new QLabel(
        tr("Option+click to set the source point, then drag to clone"), m_optionsBar));
#else
    m_optionsBar->addWidget(new QLabel(
        tr("Alt+click to set the source point, then drag to clone"), m_optionsBar));
#endif

    pushCloneOptions();
}

void MainWindow::pushCloneOptions()
{
    m_canvas->setCloneOptions(m_cloneAligned, static_cast<CloneSampling>(m_cloneSampling));
}

void MainWindow::addRotateViewOptions()
{
    // CS6's bar: the angle as a number, and a way back to upright.
    m_optionsBar->addWidget(new QLabel(tr("Rotation Angle:"), m_optionsBar));
    auto *angle = new QSpinBox(m_optionsBar);
    angle->setRange(0, 359);
    angle->setWrapping(true);
    angle->setSuffix(QStringLiteral("°"));
    angle->setFixedWidth(72);
    angle->setValue(int(m_canvas->viewRotation()));
    angle->setToolTip(tr("How far the canvas is turned on screen. The image itself is "
                         "not rotated — nothing about it changes"));
    m_optionsBar->addWidget(angle);
    connect(angle, &QSpinBox::valueChanged, this,
            [this](int value) { m_canvas->setViewRotation(value); });
    // A drag on the canvas moves the field, and the field moves the canvas,
    // without the two chasing each other: the canvas only reports angles it
    // has actually taken on.
    connect(m_canvas, &CanvasView::viewRotationChanged, angle, [angle](double degrees) {
        const QSignalBlocker blocker(angle);
        angle->setValue(int(std::lround(degrees)) % 360);
    });

    auto *reset = new QPushButton(tr("Reset View"), m_optionsBar);
    reset->setToolTip(tr("Put the canvas back upright"));
    m_optionsBar->addWidget(reset);
    connect(reset, &QPushButton::clicked, this, [this] { m_canvas->setViewRotation(0.0); });

    m_optionsBar->addSeparator();

    // Present for CS6's shape: with one canvas there is nothing for it to do.
    auto *allWindows = new QCheckBox(tr("Rotate All Windows"), m_optionsBar);
    allWindows->setEnabled(false);
    allWindows->setToolTip(tr("There is only one canvas to turn"));
    m_optionsBar->addWidget(allWindows);

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(
        tr("Drag to turn the canvas    Space+drag still pans"), m_optionsBar));
}

void MainWindow::addShapeOptions(ShapeTool tool)
{
    m_shapeTool = tool;

    // CS6 leads its shape bar with the Mode menu, since it decides what the
    // drag even produces.
    m_optionsBar->addWidget(new QLabel(tr("Mode:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    struct Choice {
        QString label;
        ShapeMode value;
        QString tip;
    };
    const Choice choices[] = {
        {tr("Shape"), ShapeMode::Shape,
         tr("Add a layer of its own, filled with the foreground colour and cut to the "
            "shape")},
        {tr("Path"), ShapeMode::Path,
         tr("Add the outline to the work path, drawing nothing until the Paths panel "
            "fills or strokes it")},
        {tr("Pixels"), ShapeMode::Pixels,
         tr("Paint the shape straight onto the active layer in the foreground colour")},
    };
    for (const Choice &choice : choices) {
        mode->addItem(choice.label, int(choice.value));
        mode->setItemData(mode->count() - 1, choice.tip, Qt::ToolTipRole);
        if (choice.value == m_shapeMode) {
            mode->setCurrentIndex(mode->count() - 1);
        }
    }
    m_optionsBar->addWidget(mode);
    connect(mode, &QComboBox::currentIndexChanged, this, [this, mode](int index) {
        m_shapeMode = static_cast<ShapeMode>(mode->itemData(index).toInt());
        m_canvas->setShapeMode(m_shapeMode);
    });

    m_optionsBar->addSeparator();

    // CS6's Fill swatch. The shape takes the foreground colour, so this is that
    // swatch rather than a second colour of its own — picking here changes the
    // foreground, exactly as the Type tool's colour button does.
    m_optionsBar->addWidget(new QLabel(tr("Fill:"), m_optionsBar));
    auto *fill = new QToolButton(m_optionsBar);
    fill->setFixedSize(22, 22);
    fill->setToolTip(tr("The foreground colour the shape is filled with"));
    auto refreshFill = [this, fill] {
        QPixmap pm(16, 16);
        pm.fill(m_engine ? m_engine->foregroundColor() : QColor(Qt::black));
        QPainter p(&pm);
        p.setPen(QColor(0, 0, 0, 160));
        p.drawRect(pm.rect().adjusted(0, 0, -1, -1));
        fill->setIcon(QIcon(pm));
    };
    refreshFill();
    m_optionsBar->addWidget(fill);
    connect(fill, &QToolButton::clicked, this, [this, refreshFill] {
        if (!m_engine) {
            return;
        }
        const QColor picked =
            ColorPickerDialog::getColor(m_engine->foregroundColor(), this, tr("Fill Color"));
        if (picked.isValid()) {
            m_engine->setForegroundColor(picked);
            m_colorPanel->setForegroundColor(picked);
            m_toolStrip->swatches()->setForeground(picked);
            refreshFill();
        }
    });

    m_optionsBar->addSeparator();

    // Then whichever setting this particular shape owns. CS6 shows one field
    // here and it changes with the tool, so only the relevant one is built.
    switch (tool) {
    case ShapeTool::RoundedRectangle: {
        m_optionsBar->addWidget(new QLabel(tr("Radius:"), m_optionsBar));
        auto *radius = new QSpinBox(m_optionsBar);
        radius->setRange(0, 1000);
        radius->setValue(m_shapeCornerRadius);
        radius->setSuffix(tr(" px"));
        radius->setFixedWidth(76);
        radius->setToolTip(tr("How far the corners are rounded. Anything past half the "
                              "shorter side gives the same fully rounded ends"));
        m_optionsBar->addWidget(radius);
        connect(radius, &QSpinBox::valueChanged, this, [this](int value) {
            m_shapeCornerRadius = value;
            pushShapeOptions();
        });
        break;
    }

    case ShapeTool::Polygon: {
        m_optionsBar->addWidget(new QLabel(tr("Sides:"), m_optionsBar));
        auto *sides = new QSpinBox(m_optionsBar);
        sides->setRange(3, 100);
        sides->setValue(m_shapeSides);
        sides->setFixedWidth(64);
        sides->setToolTip(tr("How many sides the polygon has"));
        m_optionsBar->addWidget(sides);
        connect(sides, &QSpinBox::valueChanged, this, [this](int value) {
            m_shapeSides = value;
            pushShapeOptions();
        });
        break;
    }

    case ShapeTool::Line: {
        m_optionsBar->addWidget(new QLabel(tr("Weight:"), m_optionsBar));
        auto *weight = new QSpinBox(m_optionsBar);
        weight->setRange(1, 1000);
        weight->setValue(m_shapeLineWeight);
        weight->setSuffix(tr(" px"));
        weight->setFixedWidth(76);
        weight->setToolTip(tr("How thick the line is. The Line tool draws a filled "
                              "shape, not a brush stroke, so this is its width rather "
                              "than a brush size"));
        m_optionsBar->addWidget(weight);
        connect(weight, &QSpinBox::valueChanged, this, [this](int value) {
            m_shapeLineWeight = value;
            pushShapeOptions();
        });
        break;
    }

    case ShapeTool::CustomShape: {
        m_optionsBar->addWidget(new QLabel(tr("Shape:"), m_optionsBar));
        m_customShapeButton = new QToolButton(m_optionsBar);
        m_customShapeButton->setPopupMode(QToolButton::InstantPopup);
        m_customShapeButton->setIconSize(QSize(24, 24));
        m_customShapeButton->setFixedSize(38, 26);

        auto *menu = new QMenu(m_customShapeButton);
        const QStringList names = m_engine
            ? m_engine->customShapeNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts)
            : QStringList();
        for (int i = 0; i < names.size(); ++i) {
            QAction *action = menu->addAction(names.at(i));
            action->setIcon(QIcon(QPixmap::fromImage(m_engine->customShapePreview(i, 32))));
            action->setCheckable(true);
            action->setChecked(i == m_customShape);
            connect(action, &QAction::triggered, this, [this, i] {
                m_customShape = i;
                refreshCustomShapeButton();
                pushShapeOptions();
            });
        }
        m_customShapeButton->setMenu(menu);
        m_optionsBar->addWidget(m_customShapeButton);
        refreshCustomShapeButton();
        break;
    }

    case ShapeTool::Rectangle:
    case ShapeTool::Ellipse:
        break;
    }

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(shapeHintFor(tool), m_optionsBar));

    m_canvas->setShapeMode(m_shapeMode);
    pushShapeOptions();
}

QString MainWindow::shapeHintFor(ShapeTool tool) const
{
    // The modifiers do different things per tool, so the hint says which.
    switch (tool) {
    case ShapeTool::Polygon:
        return tr("Drag from the centre outward    Shift snaps the angle to 15°");
    case ShapeTool::Line:
        return tr("Drag end to end    Shift snaps the angle to 45°");
    case ShapeTool::Ellipse:
        return tr("Drag to draw    Shift constrains to a circle    Alt draws from the centre");
    default:
        return tr("Drag to draw    Shift constrains to a square    Alt draws from the centre");
    }
}

void MainWindow::refreshCustomShapeButton()
{
    if (!m_customShapeButton || !m_engine) {
        return;
    }
    m_customShapeButton->setIcon(
        QIcon(QPixmap::fromImage(m_engine->customShapePreview(m_customShape, 24))));
    const QStringList names =
        m_engine->customShapeNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    m_customShapeButton->setToolTip(tr("Shape: %1").arg(names.value(m_customShape)));
}

void MainWindow::pushShapeOptions()
{
    if (m_engine) {
        m_engine->setShapeOptions(int(m_shapeTool), float(m_shapeCornerRadius), m_shapeSides,
                                  float(m_shapeLineWeight), m_customShape);
    }
}

void MainWindow::addBackgroundEraseOptions()
{
    m_optionsBar->addSeparator();

    // Sampling: three buttons in CS6, since it is the control the tool lives or
    // dies by — where the colour to erase comes from.
    m_optionsBar->addWidget(new QLabel(tr("Sampling:"), m_optionsBar));
    auto *sampling = new QComboBox(m_optionsBar);
    struct Mode {
        QString label;
        QString tip;
    };
    const Mode samplingModes[] = {
        {tr("Continuous"), tr("Re-read the colour under the crosshair as it moves, so "
                              "dragging along an edge erases whatever it is over")},
        {tr("Once"), tr("Erase only the colour under the crosshair when the drag began")},
        {tr("Background Swatch"), tr("Erase whatever matches the background colour, "
                                     "sampling nothing from the image")},
    };
    for (const Mode &mode : samplingModes) {
        sampling->addItem(mode.label);
        sampling->setItemData(sampling->count() - 1, mode.tip, Qt::ToolTipRole);
    }
    sampling->setCurrentIndex(m_bgEraseSampling);
    m_optionsBar->addWidget(sampling);
    connect(sampling, &QComboBox::currentIndexChanged, this, [this](int index) {
        m_bgEraseSampling = index;
        pushBackgroundEraseOptions();
    });

    m_optionsBar->addWidget(new QLabel(tr("Limits:"), m_optionsBar));
    auto *limits = new QComboBox(m_optionsBar);
    const Mode limitModes[] = {
        {tr("Discontiguous"), tr("Erase every matching pixel under the brush, connected "
                                 "or not")},
        {tr("Contiguous"), tr("Erase only what is joined to the pixel under the "
                              "crosshair")},
        {tr("Find Edges"), tr("As contiguous, but stopping at strong edges, which keeps "
                              "the erase off the far side of a boundary")},
    };
    for (const Mode &mode : limitModes) {
        limits->addItem(mode.label);
        limits->setItemData(limits->count() - 1, mode.tip, Qt::ToolTipRole);
    }
    limits->setCurrentIndex(m_bgEraseLimits);
    m_optionsBar->addWidget(limits);
    connect(limits, &QComboBox::currentIndexChanged, this, [this](int index) {
        m_bgEraseLimits = index;
        pushBackgroundEraseOptions();
    });

    m_optionsBar->addWidget(new QLabel(tr("Tolerance:"), m_optionsBar));
    auto *tolerance = new QSpinBox(m_optionsBar);
    tolerance->setRange(0, 100);
    tolerance->setValue(m_bgEraseTolerance);
    tolerance->setSuffix(QStringLiteral("%"));
    tolerance->setFixedWidth(64);
    tolerance->setToolTip(tr("How far a colour may differ from the sampled one and still "
                             "be erased"));
    m_optionsBar->addWidget(tolerance);
    connect(tolerance, &QSpinBox::valueChanged, this, [this](int value) {
        m_bgEraseTolerance = value;
        pushBackgroundEraseOptions();
    });

    auto *protect = new QCheckBox(tr("Protect Foreground Color"), m_optionsBar);
    protect->setChecked(m_bgEraseProtectForeground);
    protect->setToolTip(tr("Never erase what matches the foreground colour, for keeping a "
                           "colour that also appears in the background"));
    m_optionsBar->addWidget(protect);
    connect(protect, &QCheckBox::toggled, this, [this](bool on) {
        m_bgEraseProtectForeground = on;
        pushBackgroundEraseOptions();
    });

    pushBackgroundEraseOptions();
}

void MainWindow::pushBackgroundEraseOptions()
{
    m_canvas->setBackgroundEraseOptions(m_bgEraseSampling, m_bgEraseLimits, m_bgEraseTolerance,
                                        m_bgEraseProtectForeground);
}

void MainWindow::addMagicEraseOptions()
{
    m_optionsBar->addWidget(new QLabel(tr("Tolerance:"), m_optionsBar));
    auto *tolerance = new QSpinBox(m_optionsBar);
    tolerance->setRange(0, 255);
    tolerance->setValue(m_magicEraseTolerance);
    tolerance->setFixedWidth(64);
    tolerance->setToolTip(tr("How far a colour may differ from the clicked one and still "
                             "be erased"));
    m_optionsBar->addWidget(tolerance);
    connect(tolerance, &QSpinBox::valueChanged, this, [this](int value) {
        m_magicEraseTolerance = value;
        pushMagicEraseOptions();
    });

    auto *antialias = new QCheckBox(tr("Anti-alias"), m_optionsBar);
    antialias->setChecked(m_magicEraseAntialias);
    antialias->setToolTip(tr("Soften the edge of what is erased"));
    m_optionsBar->addWidget(antialias);
    connect(antialias, &QCheckBox::toggled, this, [this](bool on) {
        m_magicEraseAntialias = on;
        pushMagicEraseOptions();
    });

    auto *contiguous = new QCheckBox(tr("Contiguous"), m_optionsBar);
    contiguous->setChecked(m_magicEraseContiguous);
    contiguous->setToolTip(tr("Erase only the matching area joined to the click. Off, "
                              "every matching pixel in the layer goes"));
    m_optionsBar->addWidget(contiguous);
    connect(contiguous, &QCheckBox::toggled, this, [this](bool on) {
        m_magicEraseContiguous = on;
        pushMagicEraseOptions();
    });

    auto *sampleAll = new QCheckBox(tr("Sample All Layers"), m_optionsBar);
    sampleAll->setChecked(m_magicEraseSampleAll);
    sampleAll->setToolTip(tr("Decide the region from the whole visible image rather than "
                             "the active layer alone. Only the active layer is erased "
                             "either way"));
    m_optionsBar->addWidget(sampleAll);
    connect(sampleAll, &QCheckBox::toggled, this, [this](bool on) {
        m_magicEraseSampleAll = on;
        pushMagicEraseOptions();
    });

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(tr("Opacity:"), m_optionsBar));
    auto *opacity = new QSpinBox(m_optionsBar);
    opacity->setRange(1, 100);
    opacity->setValue(m_magicEraseOpacity);
    opacity->setSuffix(QStringLiteral("%"));
    opacity->setFixedWidth(64);
    opacity->setToolTip(tr("How much of the region to take away. Below 100% it is left "
                           "partly there"));
    m_optionsBar->addWidget(opacity);
    connect(opacity, &QSpinBox::valueChanged, this, [this](int value) {
        m_magicEraseOpacity = value;
        pushMagicEraseOptions();
    });

    pushMagicEraseOptions();
}

void MainWindow::pushMagicEraseOptions()
{
    m_canvas->setMagicEraseOptions(m_magicEraseTolerance, m_magicEraseAntialias,
                                   m_magicEraseContiguous, m_magicEraseSampleAll,
                                   m_magicEraseOpacity);
}

void MainWindow::addPatternStampOptions()
{
    m_optionsBar->addSeparator();

    // The pattern swatch and its picker, as CS6 has it: a button showing the
    // current tile that drops down the list. The swatches come from the engine
    // so what the picker shows is what the tool paints.
    m_patternSwatch = new QToolButton(m_optionsBar);
    m_patternSwatch->setPopupMode(QToolButton::InstantPopup);
    m_patternSwatch->setToolButtonStyle(Qt::ToolButtonIconOnly);
    m_patternSwatch->setIconSize(QSize(24, 24));
    m_patternSwatch->setFixedSize(38, 26);

    auto *menu = new QMenu(m_patternSwatch);
    const QStringList names = m_engine
        ? m_engine->patternNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts)
        : QStringList();
    for (int i = 0; i < names.size(); ++i) {
        QAction *action = menu->addAction(names.at(i));
        action->setIcon(QIcon(QPixmap::fromImage(m_engine->patternPreview(i, 32))));
        action->setCheckable(true);
        action->setChecked(i == m_patternIndex);
        connect(action, &QAction::triggered, this, [this, i] {
            m_patternIndex = i;
            refreshPatternSwatch();
            pushPatternOptions();
        });
    }
    m_patternSwatch->setMenu(menu);
    m_optionsBar->addWidget(m_patternSwatch);
    refreshPatternSwatch();

    auto *aligned = new QCheckBox(tr("Aligned"), m_optionsBar);
    aligned->setChecked(m_patternAligned);
    aligned->setToolTip(tr("Pin the pattern to the document, so separate strokes uncover "
                           "one continuous sheet. Off, every stroke starts the pattern "
                           "again where it began"));
    m_optionsBar->addWidget(aligned);
    connect(aligned, &QCheckBox::toggled, this, [this](bool on) {
        m_patternAligned = on;
        pushPatternOptions();
    });

    // Listed for CS6's shape, disabled: Impressionist repaints the pattern as
    // smeared dabs, which is a brush-dynamics feature of its own.
    auto *impressionist = new QCheckBox(tr("Impressionist"), m_optionsBar);
    impressionist->setEnabled(false);
    impressionist->setToolTip(tr("Not implemented yet"));
    m_optionsBar->addWidget(impressionist);

    pushPatternOptions();
}

void MainWindow::refreshPatternSwatch()
{
    if (!m_patternSwatch || !m_engine) {
        return;
    }
    m_patternSwatch->setIcon(QIcon(QPixmap::fromImage(m_engine->patternPreview(m_patternIndex, 24))));
    const QStringList names =
        m_engine->patternNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    m_patternSwatch->setToolTip(tr("Pattern: %1").arg(names.value(m_patternIndex)));
}

void MainWindow::pushPatternOptions()
{
    if (m_engine) {
        m_engine->setPatternOptions(m_patternIndex, m_patternAligned);
    }
}

void MainWindow::warnCloneSourceRequired()
{
    // Photoshop's own wording, down to the parenthesis.
#ifdef Q_OS_MACOS
    const QString message = tr("Could not use the clone stamp because the area to clone "
                               "has not been defined (Option-click to define a source "
                               "point).");
#else
    const QString message = tr("Could not use the clone stamp because the area to clone "
                               "has not been defined (Alt-click to define a source "
                               "point).");
#endif
    QMessageBox box(QMessageBox::Critical, tr("PhotoRust"), message, QMessageBox::Ok, this);
    unsqueezeButtons(&box);
    box.exec();
}

void MainWindow::addHealTypeButtons()
{
    // CS6's Type: a radio set of three. The choice persists across tool
    // switches, so it lives on MainWindow rather than in the widgets.
    m_optionsBar->addWidget(new QLabel(tr("Type:"), m_optionsBar));

    auto *group = new QButtonGroup(m_optionsBar);
    group->setExclusive(true);

    const HealType types[] = {HealType::ProximityMatch, HealType::CreateTexture,
                              HealType::ContentAware};
    const QString tips[] = {
        tr("Fill smoothly from the pixels around the brush — best for a blemish "
           "on skin, sky or any other gradient"),
        tr("Fill smoothly, then add grain matched to the surroundings"),
        tr("Rebuild from nearby patches, so edges and texture carry across"),
    };

    for (int i = 0; i < 3; ++i) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setText(healTypeName(types[i]));
        button->setChecked(types[i] == m_healType);
        button->setToolTip(tips[i]);
        button->setStatusTip(tips[i]);
        group->addButton(button, static_cast<int>(types[i]));
        m_optionsBar->addWidget(button);
    }

    connect(group, &QButtonGroup::idClicked, this, [this](int id) {
        m_healType = static_cast<HealType>(id);
        m_canvas->setHealType(m_healType);
    });

    m_canvas->setHealType(m_healType);
}

void MainWindow::addAnnotationOptions(EyedropperType type)
{
    // The readout labels are recreated with the bar, so the list of them is
    // rebuilt here and `updateAnnotationReadouts` fills them in.
    m_annotationReadouts.clear();

    auto addReadout = [this](const QString &prefix, int width) {
        auto *label = new QLabel(prefix, m_optionsBar);
        label->setMinimumWidth(width);
        label->setProperty("readoutPrefix", prefix);
        m_optionsBar->addWidget(label);
        m_annotationReadouts.append(label);
    };

    switch (type) {
    case EyedropperType::ColorSampler:
        // No readouts here: the sampler values live in the Info panel, which
        // is where CS6 puts them too.
        break;

    case EyedropperType::Ruler:
        // Photoshop's own labels, in its order.
        for (const QString &field : {QStringLiteral("X:"), QStringLiteral("Y:"),
                                     QStringLiteral("W:"), QStringLiteral("H:"),
                                     QStringLiteral("A:"), QStringLiteral("D1:")}) {
            addReadout(field, 62);
        }
        break;

    case EyedropperType::Count:
        addReadout(tr("Count:"), 80);
        break;

    case EyedropperType::Note:
        addReadout(tr("Notes:"), 80);
        break;

    case EyedropperType::Eyedropper:
        return;
    }

    m_optionsBar->addSeparator();

    auto *clear = new QToolButton(m_optionsBar);
    clear->setText(tr("Clear"));
    m_optionsBar->addWidget(clear);
    connect(clear, &QToolButton::clicked, this, [this, type] {
        if (!m_engine) {
            return;
        }
        switch (type) {
        case EyedropperType::ColorSampler:
            m_engine->clearMarkers(static_cast<int>(MarkerKind::ColorSampler));
            break;
        case EyedropperType::Note:
            m_engine->clearMarkers(static_cast<int>(MarkerKind::Note));
            break;
        case EyedropperType::Count:
            m_engine->clearMarkers(static_cast<int>(MarkerKind::Count));
            break;
        case EyedropperType::Ruler:
            m_engine->clearRuler();
            break;
        case EyedropperType::Eyedropper:
            break;
        }
    });

    m_optionsBar->addSeparator();

    QString hint;
    switch (type) {
    case EyedropperType::ColorSampler:
        hint = tr("Click to place a sampler    Drag to move    Alt+click to remove    "
                  "Values read out in the Info panel (F8)");
        break;
    case EyedropperType::Ruler:
        hint = tr("Drag to measure    Drag either end to adjust");
        break;
    case EyedropperType::Note:
        hint = tr("Click to add a note    Click a note to edit it    Alt+click to remove");
        break;
    case EyedropperType::Count:
        hint = tr("Click to count    Drag to move a mark    Alt+click to remove");
        break;
    case EyedropperType::Eyedropper:
        break;
    }
    m_optionsBar->addWidget(new QLabel(hint, m_optionsBar));

    updateAnnotationReadouts();
}

void MainWindow::updateAnnotationReadouts()
{
    if (m_annotationReadouts.isEmpty() || !m_engine) {
        return;
    }

    const auto set = [this](int i, const QString &text) {
        if (i < m_annotationReadouts.size()) {
            QLabel *label = m_annotationReadouts.at(i);
            label->setText(label->property("readoutPrefix").toString() + QLatin1Char(' ') + text);
        }
    };

    switch (m_activeTool == ToolId::Eyedropper ? static_cast<EyedropperType>(m_activeVariant)
                                               : EyedropperType::Eyedropper) {
    case EyedropperType::ColorSampler:
        // Handled entirely by the Info panel.
        break;

    case EyedropperType::Ruler: {
        const rust::Vec<float> m = m_engine->rulerMeasurement();
        if (m.size() < 6) {
            for (int i = 0; i < m_annotationReadouts.size(); ++i) {
                set(i, QStringLiteral("—"));
            }
            break;
        }
        for (int i = 0; i < 6; ++i) {
            // The angle gets a degree sign; the rest are plain pixels.
            set(i, i == 4 ? QStringLiteral("%1°").arg(m[i], 0, 'f', 1)
                          : QStringLiteral("%1").arg(m[i], 0, 'f', 1));
        }
        break;
    }

    case EyedropperType::Count:
        set(0, QString::number(m_engine->markerCount(static_cast<int>(MarkerKind::Count))));
        break;

    case EyedropperType::Note:
        set(0, QString::number(m_engine->markerCount(static_cast<int>(MarkerKind::Note))));
        break;

    case EyedropperType::Eyedropper:
        break;
    }
}

void MainWindow::editNote(int index)
{
    if (!m_engine) {
        return;
    }
    const int kind = static_cast<int>(MarkerKind::Note);
    const QString current = m_engine->markerText(kind, index);

    bool ok = false;
    const QString text = QInputDialog::getMultiLineText(this, tr("Note"), tr("Note text:"),
                                                        current, &ok);
    if (!ok) {
        // Cancelling a brand-new note removes it again, rather than leaving an
        // empty marker on the image.
        if (current.isEmpty()) {
            m_engine->removeMarker(kind, index);
        }
        return;
    }
    m_engine->setMarkerText(kind, index, text);
}

void MainWindow::addCropOptions(CropType type)
{
    if (type == CropType::Slice || type == CropType::SliceSelect) {
        auto *clear = new QToolButton(m_optionsBar);
        clear->setText(tr("Clear Slices"));
        clear->setToolTip(tr("Remove every user slice, leaving the whole canvas as one "
                             "auto slice"));
        m_optionsBar->addWidget(clear);
        connect(clear, &QToolButton::clicked, this, [this] {
            if (m_engine) {
                m_engine->clearSlices();
            }
        });

        if (type == CropType::SliceSelect) {
            auto *remove = new QToolButton(m_optionsBar);
            remove->setText(tr("Delete Slice"));
            remove->setToolTip(tr("Delete the selected slice (Del)"));
            m_optionsBar->addWidget(remove);
            connect(remove, &QToolButton::clicked, m_canvas, &CanvasView::deleteSelectedSlice);
        }

        m_optionsBar->addSeparator();

        auto *save = new QToolButton(m_optionsBar);
        save->setText(tr("Save Slices..."));
        save->setToolTip(tr("Write every slice out as its own image file"));
        m_optionsBar->addWidget(save);
        connect(save, &QToolButton::clicked, this, &MainWindow::exportSlices);

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(new QLabel(
            type == CropType::Slice
                ? tr("Drag to cut a slice    The rest of the canvas is sliced automatically")
                : tr("Click a slice to select it    Drag it or its handles to adjust    "
                     "Del to remove it"),
            m_optionsBar));
        return;
    }

    if (type == CropType::Perspective) {
        // Perspective Crop has neither a ratio preset nor Delete Cropped
        // Pixels in CS6: the output size comes from the quad the user marked,
        // and everything outside it is resampled away by definition.
        auto *cancel = new QToolButton(m_optionsBar);
        cancel->setText(QStringLiteral("✘"));
        cancel->setToolTip(tr("Cancel the crop (Esc)"));
        m_optionsBar->addWidget(cancel);
        connect(cancel, &QToolButton::clicked, m_canvas, &CanvasView::resetCrop);

        auto *apply = new QToolButton(m_optionsBar);
        apply->setText(QStringLiteral("✓"));
        apply->setToolTip(tr("Straighten and crop (Enter)"));
        m_optionsBar->addWidget(apply);
        connect(apply, &QToolButton::clicked, m_canvas, &CanvasView::commitCrop);

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(new QLabel(
            tr("Drag out a box, then pull its corners onto the subject    "
               "Enter or double-click to straighten    Esc to reset"),
            m_optionsBar));
        return;
    }

    // CS6 opens the Crop bar with a ratio preset. The named ratios are the
    // ones its own list offers; "Unconstrained" leaves the box free.
    struct Preset {
        QString label;
        double ratio;
    };
    const Preset presets[] = {
        {tr("Unconstrained"), 0.0},
        {tr("1 : 1 (Square)"), 1.0},
        {tr("4 : 5 (8:10)"), 4.0 / 5.0},
        {tr("5 : 7"), 5.0 / 7.0},
        {tr("2 : 3 (4:6)"), 2.0 / 3.0},
        {tr("16 : 9"), 16.0 / 9.0},
    };

    auto *ratio = new QComboBox(m_optionsBar);
    for (const Preset &preset : presets) {
        ratio->addItem(preset.label, preset.ratio);
    }
    ratio->setCurrentIndex(0);
    for (int i = 0; i < ratio->count(); ++i) {
        if (qFuzzyCompare(ratio->itemData(i).toDouble() + 1.0, m_cropRatio + 1.0)) {
            ratio->setCurrentIndex(i);
            break;
        }
    }
    ratio->setToolTip(tr("Lock the crop box to an aspect ratio"));
    m_optionsBar->addWidget(ratio);
    connect(ratio, &QComboBox::currentIndexChanged, this, [this, ratio](int index) {
        m_cropRatio = ratio->itemData(index).toDouble();
        pushCropOptions();
    });

    m_optionsBar->addSeparator();

    auto *deletePixels = new QCheckBox(tr("Delete Cropped Pixels"), m_optionsBar);
    deletePixels->setChecked(m_cropDeletePixels);
    deletePixels->setToolTip(tr("Discard the pixels outside the crop, rather than keeping "
                                "them hidden beyond the canvas edge"));
    m_optionsBar->addWidget(deletePixels);
    connect(deletePixels, &QCheckBox::toggled, this, [this](bool on) {
        m_cropDeletePixels = on;
        pushCropOptions();
    });

    m_optionsBar->addSeparator();

    // CS6 puts a cancel/commit pair at the right of the crop bar. The keyboard
    // route (Esc / Enter) is handled by the canvas.
    auto *cancel = new QToolButton(m_optionsBar);
    cancel->setText(QStringLiteral("✘"));
    cancel->setToolTip(tr("Cancel the crop (Esc)"));
    m_optionsBar->addWidget(cancel);
    connect(cancel, &QToolButton::clicked, m_canvas, &CanvasView::resetCrop);

    auto *commit = new QToolButton(m_optionsBar);
    commit->setText(QStringLiteral("✓"));
    commit->setToolTip(tr("Apply the crop (Enter)"));
    m_optionsBar->addWidget(commit);
    connect(commit, &QToolButton::clicked, m_canvas, &CanvasView::commitCrop);

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(
        tr("Drag to set the crop    Enter or double-click to apply    Esc to reset"),
        m_optionsBar));

    pushCropOptions();
}

void MainWindow::pushCropOptions()
{
    m_canvas->setCropOptions(m_cropRatio, m_cropDeletePixels);
}

void MainWindow::addQuickSelectOptions(QuickSelectType type)
{
    if (type == QuickSelectType::Brush) {
        // CS6 gives Quick Selection a full brush picker; the diameter is the
        // part that changes the result, so that is what is here. Notably it
        // has no Tolerance — the tool works that out from the pixels under the
        // brush (see core/src/wand.rs).
        m_optionsBar->addWidget(new QLabel(tr("Size:"), m_optionsBar));
        auto *size = new QSpinBox(m_optionsBar);
        size->setRange(1, 5000);
        size->setValue(m_quickBrushSize);
        size->setSuffix(tr(" px"));
        size->setFixedWidth(80);
        size->setToolTip(tr("Diameter of the brush the selection grows from"));
        m_optionsBar->addWidget(size);
        connect(size, &QSpinBox::valueChanged, this, [this](int value) {
            m_quickBrushSize = value;
            pushQuickSelectOptions();
        });
        pushQuickSelectOptions();
        return;
    }

    // Magic Wand: Tolerance, then the two checkboxes, in CS6's order.
    m_optionsBar->addWidget(new QLabel(tr("Tolerance:"), m_optionsBar));
    auto *tolerance = new QSpinBox(m_optionsBar);
    tolerance->setRange(0, 255);
    tolerance->setValue(m_wandTolerance);
    tolerance->setFixedWidth(64);
    tolerance->setToolTip(tr("How far a pixel may differ, per channel, and still match"));
    m_optionsBar->addWidget(tolerance);
    connect(tolerance, &QSpinBox::valueChanged, this, [this](int value) {
        m_wandTolerance = value;
        pushQuickSelectOptions();
    });

    struct Toggle {
        QString label;
        bool *value;
        QString tip;
    };
    const Toggle toggles[] = {
        {tr("Anti-alias"), &m_wandAntialias, tr("Soften the edge of the selection")},
        {tr("Contiguous"), &m_wandContiguous,
         tr("Select only the connected area, rather than every matching pixel")},
    };

    for (const Toggle &toggle : toggles) {
        auto *box = new QCheckBox(toggle.label, m_optionsBar);
        box->setChecked(*toggle.value);
        box->setToolTip(toggle.tip);
        m_optionsBar->addWidget(box);

        bool *slot = toggle.value;
        connect(box, &QCheckBox::toggled, this, [this, slot](bool on) {
            *slot = on;
            pushQuickSelectOptions();
        });
    }

    pushQuickSelectOptions();
}

void MainWindow::pushQuickSelectOptions()
{
    m_canvas->setQuickSelectOptions(m_quickBrushSize, m_wandTolerance, m_wandAntialias,
                                    m_wandContiguous);
}

void MainWindow::pushMagneticOptions()
{
    m_canvas->setMagneticOptions(m_magneticWidth, m_magneticContrast, m_magneticFrequency);
}

void MainWindow::pushBrushSettings()
{
    if (!m_engine) {
        return;
    }
    // Opacity and Flow live on the bar and only exist while a painting tool is
    // active; size and hardness are kept here, since the picker that edits them
    // is created once and outlives any one options bar.
    const int opacity = m_brushOpacity ? m_brushOpacity->value() : 100;
    const int flow = m_brushFlow ? m_brushFlow->value() : 100;
    // Spacing belongs to the tip, so it comes from the picker's preset rather
    // than a fixed value — a spatter brush needs a much wider step than a round
    // one to read as spatter instead of a solid line.
    const int spacing = m_brushPicker ? m_brushPicker->current().spacing : 25;
    m_engine->setBrush(float(m_brushSizeValue), m_brushHardnessValue, opacity, flow, spacing);
    // The Pencil paints aliased; every other tool in the family antialiases.
    m_engine->setBrushAntialias(!m_pencilMode);
    m_engine->setAutoErase(m_pencilMode && m_autoErase);
    if (m_canvas) {
        m_canvas->setBrushSize(m_brushSizeValue);
    }
}

BrushPresetPicker *MainWindow::brushPicker()
{
    // Built on first use and kept: it holds the current tip, so it must not be
    // recreated with the options bar.
    if (!m_brushPicker) {
        m_brushPicker = new BrushPresetPicker(m_engine, this);
        connect(m_brushPicker, &BrushPresetPicker::tipChanged, this,
                [this](const BrushPresetPicker::Preset &preset) {
                    m_brushSizeValue = preset.size;
                    m_brushHardnessValue = preset.hardness;
                    refreshBrushTipButton();
                    // The picker has already pushed the tip shape; this adds the
                    // options bar's Opacity and Flow on top.
                    pushBrushSettings();
                });
    }
    return m_brushPicker;
}

void MainWindow::refreshBrushTipButton()
{
    if (!m_brushTipButton) {
        return;
    }
    m_brushTipButton->setIcon(QIcon(brushPicker()->tipPreview(20)));
    m_brushTipButton->setText(QString::number(int(m_brushSizeValue)));
}

// ------------------------------------------------------------------- docks --

void MainWindow::createDocks()
{
    QMenu *windowMenu = menuBar()->findChild<QMenu *>(QStringLiteral("windowMenu"));

    // The tool panel closes from the × on its own header, so it needs an entry
    // here to come back — same as CS6's Window ▸ Tools.
    if (windowMenu && m_toolsDock) {
        windowMenu->addAction(m_toolsDock->toggleViewAction());
        windowMenu->addSeparator();
    }

    auto addPanel = [&](const QString &title, QWidget *content, Qt::DockWidgetArea area,
                        const QString &commandId = {}) {
        auto *dock = new QDockWidget(title, this);
        dock->setObjectName(title + QStringLiteral("Dock"));
        dock->setWidget(content);
        dock->setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea);
        addDockWidget(area, dock);

        if (windowMenu) {
            QAction *toggle = dock->toggleViewAction();
            // Bind the panel toggle to its keymap entry (F7 for Layers, …).
            if (!commandId.isEmpty()) {
                if (QAction *bound = m_registry->action(commandId)) {
                    toggle->setShortcut(bound->shortcut());
                }
            }
            windowMenu->addAction(toggle);
        }
        return dock;
    };

    m_colorPanel = new ColorPanel(m_engine, this);
    addPanel(tr("Color"), m_colorPanel, Qt::RightDockWidgetArea,
             QStringLiteral("window.color"));

    // A stand-in so the Color/Swatches pair reads like CS6; real swatch
    // management is not implemented yet.
    auto *swatchesPlaceholder = new QLabel(tr("  Swatches"), this);
    swatchesPlaceholder->setAlignment(Qt::AlignTop | Qt::AlignLeft);
    QDockWidget *swatchesDock =
        addPanel(tr("Swatches"), swatchesPlaceholder, Qt::RightDockWidgetArea);

    m_infoPanel = new InfoPanel(m_engine, this);
    m_infoDock = addPanel(tr("Info"), m_infoPanel, Qt::RightDockWidgetArea,
                          QStringLiteral("window.info"));

    // CS6 tabs Properties in with Color and Swatches, in that corner.
    m_propertiesPanel = new PropertiesPanel(m_engine, this);
    m_propertiesDock = addPanel(tr("Properties"), m_propertiesPanel,
                                Qt::RightDockWidgetArea);

    m_historyPanel = new HistoryPanel(m_engine, this);
    QDockWidget *historyDock = addPanel(tr("History"), m_historyPanel,
                                        Qt::RightDockWidgetArea);

    m_layersPanel = new LayersPanel(m_engine, this);
    QDockWidget *layersDock = addPanel(tr("Layers"), m_layersPanel,
                                       Qt::RightDockWidgetArea,
                                       QStringLiteral("window.layers"));

    m_channelsPanel = new ChannelsPanel(m_engine, this);
    QDockWidget *channelsDock = addPanel(tr("Channels"), m_channelsPanel, Qt::RightDockWidgetArea);

    m_pathsPanel = new PathsPanel(m_engine, this);
    QDockWidget *pathsDock = addPanel(tr("Paths"), m_pathsPanel, Qt::RightDockWidgetArea);

    // Stack Color/Swatches into one tabbed group, as CS6 ships them.
    if (QDockWidget *colorDock =
            findChild<QDockWidget *>(tr("Color") + QStringLiteral("Dock"))) {
        tabifyDockWidget(colorDock, swatchesDock);
        if (m_infoDock) {
            tabifyDockWidget(swatchesDock, m_infoDock);
        }
        if (m_propertiesDock) {
            tabifyDockWidget(m_infoDock ? m_infoDock : swatchesDock, m_propertiesDock);
        }
        colorDock->raise();
    }
    tabifyDockWidget(layersDock, channelsDock);
    tabifyDockWidget(channelsDock, pathsDock);
    layersDock->raise();

    resizeDocks({historyDock, layersDock}, {220, 380}, Qt::Vertical);

    connect(m_layersPanel, &LayersPanel::editLayerStyle, this,
            [this](int layerIndex, const QString &effectKey) {
                if (m_engine) {
                    m_engine->setActiveLayer(layerIndex);
                }
                // An empty key is the "Effects" heading, which opens the dialog
                // wherever it was last.
                showLayerStyle(effectKey);
            });
    connect(m_layersPanel, &LayersPanel::documentChanged,
            this, &MainWindow::onDocumentChanged);
    connect(m_historyPanel, &HistoryPanel::documentChanged,
            this, &MainWindow::onDocumentChanged);
    connect(m_pathsPanel, &PathsPanel::documentChanged,
            this, &MainWindow::onDocumentChanged);
    connect(m_channelsPanel, &ChannelsPanel::channelMaskChanged,
            m_canvas, &CanvasView::setChannelMask);
    connect(m_propertiesPanel, &PropertiesPanel::documentChanged,
            this, &MainWindow::onDocumentChanged);
}

void MainWindow::createStatusBar()
{
    // Editable, as Photoshop's is: type a percentage and press Enter.
    m_statusZoom = new QLineEdit(QStringLiteral("100%"), this);
    m_statusZoom->setObjectName(QStringLiteral("statusZoom"));
    m_statusZoom->setFixedWidth(58);
    m_statusZoom->setAlignment(Qt::AlignRight);
    m_statusZoom->setToolTip(tr("Zoom level. Type a percentage and press Enter."));
    statusBar()->addWidget(m_statusZoom);

    connect(m_statusZoom, &QLineEdit::returnPressed, this, &MainWindow::applyTypedZoom);
    // Clicking away without pressing Enter puts the real value back, rather than
    // leaving a half-typed number sitting there looking authoritative.
    connect(m_statusZoom, &QLineEdit::editingFinished, this, [this] {
        if (!m_statusZoom->hasFocus()) {
            onZoomChanged(m_canvas->zoom());
        }
    });

    m_statusDocSize = new QLabel(this);
    m_statusDocSize->setMinimumWidth(140);
    statusBar()->addWidget(m_statusDocSize);

    m_statusPosition = new QLabel(this);
    m_statusPosition->setMinimumWidth(120);
    statusBar()->addPermanentWidget(m_statusPosition);
}

void MainWindow::refreshDocumentTabs()
{
    if (!m_engine || !m_documentTabs) {
        return;
    }
    // Rebuilding emits currentChanged, which would bounce straight back into
    // the engine and fight with what it just told us.
    const QSignalBlocker blocker(m_documentTabs);

    const int count = m_engine->documentCount();
    while (m_documentTabs->count() > count) {
        m_documentTabs->removeTab(m_documentTabs->count() - 1);
    }
    while (m_documentTabs->count() < count) {
        m_documentTabs->addTab(QString());
    }
    for (int i = 0; i < count; ++i) {
        m_documentTabs->setTabText(i, m_engine->documentTitleAt(i));
        m_documentTabs->setTabToolTip(i, m_engine->documentTitleAt(i));
    }
    m_documentTabs->setCurrentIndex(m_engine->activeDocument());
}

void MainWindow::onTabSelected(int index)
{
    if (m_engine && index >= 0) {
        if (m_canvas && m_canvas->isFreeTransforming())
            m_canvas->cancelFreeTransform();
        m_engine->setActiveDocument(index);
    }
}

void MainWindow::onTabCloseRequested(int index)
{
    if (!m_engine || index < 0) {
        return;
    }
    // Switch to the tab first, so an unsaved-changes prompt names that document
    // and Save writes the right one.
    if (m_engine->documentModifiedAt(index)) {
        m_engine->setActiveDocument(index);
        refreshDocumentTabs();
        if (!confirmDiscardChanges()) {
            return;
        }
        index = m_engine->activeDocument();
    }

    if (!m_engine->closeDocument(index)) {
        statusBar()->showMessage(tr("The last document cannot be closed."), 4000);
    }
}

void MainWindow::connectEngine()
{
    if (!m_engine) {
        return;
    }
    // The engine is the source of truth: it announces changes and the UI
    // re-reads. No panel pushes state at another panel.
    connect(m_engine, &Engine::canvasChanged, m_canvas, &CanvasView::refresh);
    connect(m_engine, &Engine::layersChanged, m_layersPanel, &LayersPanel::refresh);
    // Selecting a layer goes through the engine, so this covers both a change
    // to the layer and a change of which layer the panel is showing.
    connect(m_engine, &Engine::layersChanged, m_propertiesPanel,
            &PropertiesPanel::refresh);
    connect(m_engine, &Engine::documentsChanged, m_propertiesPanel,
            &PropertiesPanel::refresh);
    connect(m_engine, &Engine::canvasChanged, m_channelsPanel, &ChannelsPanel::refresh);
    connect(m_engine, &Engine::historyChanged, m_historyPanel, &HistoryPanel::refresh);
    connect(m_engine, &Engine::selectionChanged, m_canvas, &CanvasView::refreshSelection);
    connect(m_engine, &Engine::slicesChanged, m_canvas, &CanvasView::refreshSlices);
    connect(m_engine, &Engine::pathsChanged, m_pathsPanel, &PathsPanel::refresh);
    // The Pen and Path Selection tools edit the active path directly, with no
    // C++-side echo of their own — this is what repaints the overlay.
    connect(m_engine, &Engine::pathsChanged, m_canvas, [this] { m_canvas->update(); });
    connect(m_engine, &Engine::annotationsChanged, m_canvas, &CanvasView::refreshAnnotations);
    connect(m_engine, &Engine::annotationsChanged, this,
            &MainWindow::updateAnnotationReadouts);
    connect(m_engine, &Engine::annotationsChanged, m_infoPanel, &InfoPanel::refreshSamplers);
    connect(m_engine, &Engine::annotationsChanged, m_infoPanel, &InfoPanel::refreshRuler);
    connect(m_engine, &Engine::selectionChanged, m_infoPanel, &InfoPanel::refreshSelection);
    // The samplers read the pixels beneath them, so an edit changes their
    // values even when the samplers themselves have not moved.
    connect(m_engine, &Engine::canvasChanged, m_infoPanel, &InfoPanel::refreshSamplers);
    connect(m_engine, &Engine::layersChanged, m_infoPanel, &InfoPanel::refreshDocumentSize);
    connect(m_engine, &Engine::documentTitleChanged, this, &MainWindow::updateWindowTitle);
    connect(m_engine, &Engine::documentTitleChanged, this, &MainWindow::refreshDocumentTabs);
    connect(m_engine, &Engine::documentsChanged, this, &MainWindow::refreshDocumentTabs);
    // A document swap changes everything downstream, so treat it like a reload.
    connect(m_engine, &Engine::documentsChanged, this, &MainWindow::onDocumentChanged);
    // Only a *swap* invalidates the Alt-clicked sample points, which is why this
    // hangs off the engine's signal and not `onDocumentChanged` — the panels
    // raise that one for every edit, and adding a layer must not silently
    // forget where the Clone Stamp was told to sample from.
    connect(m_engine, &Engine::documentsChanged, m_canvas,
            &CanvasView::forgetSampleSources);
}

// ---------------------------------------------------------------- commands --

void MainWindow::newDocument()
{
    const int docNumber = m_engine->documentCount() + 1;
    NewDocumentDialog dialog(docNumber, this);
    if (dialog.exec() != QDialog::Accepted)
        return;

    m_engine->newDocument(dialog.widthPixels(), dialog.heightPixels(),
                          dialog.backgroundFill());
    refreshAll();
    fitOnScreen();
}

bool MainWindow::openAnimatedFrames(const QString &path)
{
    // Qt reports the frame count for the formats that have one; -1 means it
    // does not know, which for our purposes is the same as "one image".
    QImageReader reader(path);
    // A camera's own orientation, applied by Qt as it decodes — the same thing
    // the engine does for the files it reads itself.
    reader.setAutoTransform(true);
    if (reader.imageCount() <= 1) {
        return false;
    }

    QImage frame = reader.read();
    if (frame.isNull() || !m_engine->loadImage(frame, path)) {
        return false;
    }

    // The document arrives with one layer holding the first frame; the rest
    // stack above it in order, so the composite shows the last frame and the
    // Layers panel reads like the animation.
    m_engine->setLayerName(0, tr("Frame 1"));
    int count = 1;
    while (reader.canRead()) {
        const QImage next = reader.read();
        if (next.isNull()) {
            break;
        }
        ++count;
        if (!m_engine->addImageLayer(next, 0, 0, tr("Frame %1").arg(count))) {
            break;
        }
    }

    statusBar()->showMessage(tr("Opened %1 frames as layers").arg(count), 4000);
    return true;
}

void MainWindow::openDocument()
{
    // Every file opens in its own tab, so nothing already open is at risk.
    const QStringList paths =
        askForFiles(this, tr("Open"), openFilter(), QFileDialog::AcceptOpen);
    if (paths.isEmpty()) {
        return;
    }

    // One warning at the end rather than one per file: choosing thirty photos
    // and being asked to dismiss five dialogs is worse than being told once
    // which five did not open.
    QStringList failed;
    for (const QString &path : paths) {
        if (!loadPath(path)) {
            failed.append(QFileInfo(path).fileName());
        }
    }

    if (!failed.isEmpty()) {
        QMessageBox::warning(this, tr("Open"),
                             tr("Could not open:\n\n%1\n\n"
                                "They may be corrupt, or in a format that is not "
                                "supported yet.")
                                 .arg(failed.join(QLatin1String("\n"))));
    }
}

void MainWindow::openPath(const QString &path)
{
    if (!loadPath(path)) {
        QMessageBox::warning(this, tr("Open"),
                             tr("Could not open \"%1\".\n\n"
                                "The file may be corrupt, or in a format that is "
                                "not supported yet.")
                                 .arg(QFileInfo(path).fileName()));
    }
}

bool MainWindow::loadPath(const QString &path)
{
    // An animated GIF is several images in one file, and opening only the
    // first would throw the rest away without saying so.
    if (openAnimatedFrames(path)) {
        rememberRecentFile(path);
        refreshAll();
        fitOnScreen();
        return true;
    }

    if (!m_engine->openFile(path)) {
        // The engine reads PSD itself and delegates the rest to Qt's plugins;
        // if it declined, try decoding here and handing the pixels over.
        // Through a reader rather than QImage's constructor, so a photograph's
        // EXIF orientation is applied on this path too.
        QImageReader reader(path);
        reader.setAutoTransform(true);
        const QImage image = reader.read();
        if (image.isNull() || !m_engine->loadImage(image, path)) {
            return false;
        }
    }
    rememberRecentFile(path);
    refreshAll();
    fitOnScreen();
    return true;
}

bool MainWindow::saveDocument()
{
    // Without a known path this is really Save As.
    return saveDocumentAs();
}

bool MainWindow::saveDocumentAs()
{
    const QString path = askForFile(this, tr("Save As"), saveFilter(), QFileDialog::AcceptSave);
    if (path.isEmpty()) {
        return false;
    }

    // PSD is written by the engine; everything else by Qt's writers.
    if (path.endsWith(QStringLiteral(".psd"), Qt::CaseInsensitive)) {
        if (!m_engine->saveFile(path)) {
            QMessageBox::warning(this, tr("Save"), tr("Could not write \"%1\".").arg(path));
            return false;
        }
    } else {
        const QImage composite = m_engine->compositeImage();
        if (!composite.save(path)) {
            QMessageBox::warning(this, tr("Save"), tr("Could not write \"%1\".").arg(path));
            return false;
        }
        m_engine->markSavedAs(path);
    }

    updateWindowTitle();
    return true;
}

void MainWindow::exportSlices()
{
    if (!m_engine) {
        return;
    }
    const int count = m_engine->sliceCount();
    if (count <= 0) {
        return;
    }

    // Qt's own dialog here too, so every file chooser in the application looks
    // and behaves the same way.
    QFileDialog chooser(this, tr("Save Slices To"));
    chooser.setOption(QFileDialog::DontUseNativeDialog);
    chooser.setFileMode(QFileDialog::Directory);
    chooser.setOption(QFileDialog::ShowDirsOnly);
    const QString dir = chooser.exec() == QDialog::Accepted && !chooser.selectedFiles().isEmpty()
        ? chooser.selectedFiles().first()
        : QString();
    if (dir.isEmpty()) {
        return;
    }

    // Photoshop names slice files after the document with the slice number
    // appended, which is what makes a sliced layout reassemblable.
    QString base = QFileInfo(m_engine->getDocumentTitle()).completeBaseName();
    base.remove(QLatin1Char('*'));
    if (base.isEmpty()) {
        base = QStringLiteral("slice");
    }

    int written = 0;
    QStringList failures;
    for (int i = 0; i < count; ++i) {
        const rust::Vec<::std::int32_t> info = m_engine->sliceAt(i);
        if (info.size() < 6) {
            continue;
        }
        const QImage image = m_engine->sliceImage(i);
        if (image.isNull()) {
            continue;
        }

        const QString path =
            QStringLiteral("%1/%2_%3.png")
                .arg(dir, base, QString::number(info[4]).rightJustified(2, QLatin1Char('0')));
        if (image.save(path)) {
            ++written;
        } else {
            failures.append(QFileInfo(path).fileName());
        }
    }

    if (failures.isEmpty()) {
        statusBar()->showMessage(tr("Wrote %n slice(s) to %1", nullptr, written).arg(dir), 4000);
    } else {
        QMessageBox::warning(this, tr("Save Slices"),
                             tr("Wrote %1 of %2 slices. Could not write: %3")
                                 .arg(written)
                                 .arg(count)
                                 .arg(failures.join(QStringLiteral(", "))));
    }
}

void MainWindow::exportAs()
{
    if (!m_engine)
        return;

    const QImage composite = m_engine->compositeImage();
    if (composite.isNull())
        return;

    exportImageAs(composite, m_engine->getDocumentTitle());
}

void MainWindow::exportLayerAs()
{
    if (!m_engine)
        return;

    const int index = m_engine->getActiveLayerIndex();
    const QImage layer = m_engine->layerImage(index);
    if (layer.isNull())
        return;

    // Photoshop exports the layer's *content*, not its buffer: a layer may
    // hold a canvas-sized buffer with a few strokes in it, and exporting the
    // empty margin with it would be a surprise. The bounds are in document
    // space, so they come back to the layer's own by its offset.
    const QRect bounds = m_engine->layerContentBounds(index);
    QImage image = layer;
    if (!bounds.isEmpty()) {
        image = layer.copy(bounds.translated(-m_engine->layerOffsetX(index),
                                             -m_engine->layerOffsetY(index)));
    }
    if (image.isNull()) {
        QMessageBox::information(this, tr("Export As"),
                                 tr("This layer has nothing in it to export."));
        return;
    }

    exportImageAs(image, m_engine->layerName(index));
}

void MainWindow::exportImageAs(const QImage &image, const QString &name)
{
    ExportAsDialog dialog(image, name, this);
    if (dialog.exec() != QDialog::Accepted)
        return;

    // Build the save filter from the chosen format.
    QString formatFilter;
    QString defaultExt;
    switch (dialog.chosenFormat()) {
    case ExportAsDialog::PNG:
        formatFilter = tr("PNG (*.png)");
        defaultExt = QStringLiteral("png");
        break;
    case ExportAsDialog::JPG:
        formatFilter = tr("JPEG (*.jpg *.jpeg)");
        defaultExt = QStringLiteral("jpg");
        break;
    case ExportAsDialog::PNG8:
        formatFilter = tr("PNG (*.png)");
        defaultExt = QStringLiteral("png");
        break;
    case ExportAsDialog::GIF:
        formatFilter = tr("GIF (*.gif)");
        defaultExt = QStringLiteral("gif");
        break;
    }

    const QString path = askForFile(this, tr("Export As"), formatFilter, QFileDialog::AcceptSave);
    if (path.isEmpty())
        return;

    const QImage img = dialog.exportImage();
    bool ok = false;
    if (dialog.chosenFormat() == ExportAsDialog::JPG) {
        ok = img.save(path, "JPEG", dialog.jpegQuality());
    } else if (dialog.chosenFormat() == ExportAsDialog::GIF) {
        ok = writeGif(img, path);
    } else {
        ok = img.save(path);
    }

    if (!ok) {
        QMessageBox::warning(this, tr("Export"), tr("Could not write \"%1\".").arg(path));
    } else {
        statusBar()->showMessage(tr("Exported to %1").arg(path), 4000);
    }
}

void MainWindow::saveForWeb()
{
    if (!m_engine)
        return;

    const QImage composite = m_engine->compositeImage();
    if (composite.isNull())
        return;

    SaveForWebDialog dialog(composite, this);
    if (dialog.exec() != QDialog::Accepted)
        return;

    // Build a filter for the chosen format.
    const QString ext = dialog.fileExtension();
    const QString filter = QStringLiteral("%1 (*.%2)")
                               .arg(ext.toUpper(), ext);

    const QString path = askForFile(this, tr("Save Optimized As"), filter, QFileDialog::AcceptSave);
    if (path.isEmpty())
        return;

    const QImage img = dialog.exportImage();
    bool ok = false;
    if (dialog.chosenFormat() == SaveForWebDialog::JPEG) {
        ok = img.save(path, "JPEG", dialog.jpegQuality());
    } else if (dialog.chosenFormat() == SaveForWebDialog::GIF) {
        ok = writeGif(img, path);
    } else if (dialog.chosenFormat() == SaveForWebDialog::WBMP) {
        ok = img.save(path, "BMP");
    } else {
        ok = img.save(path);
    }

    if (!ok) {
        QMessageBox::warning(this, tr("Save for Web"),
                             tr("Could not write \"%1\".").arg(path));
    } else {
        statusBar()->showMessage(tr("Saved for web: %1").arg(path), 4000);
    }
}

void MainWindow::printDocument()
{
    if (!m_engine)
        return;

    const QImage composite = m_engine->compositeImage();
    if (composite.isNull())
        return;

    PrintDialog dialog(composite, this);
    dialog.exec();
}

void MainWindow::printOneCopy()
{
    if (!m_engine)
        return;

    const QImage composite = m_engine->compositeImage();
    if (composite.isNull())
        return;

    // Letter size at 72 DPI — the baseline Photoshop uses for the clipping
    // check. Images wider or taller than this at 1:1 will be clipped.
    constexpr double kLetterWidthIn = 8.5;
    constexpr double kLetterHeightIn = 11.0;
    constexpr double kDpi = 72.0;
    const double pageWidthPx = kLetterWidthIn * kDpi;
    const double pageHeightPx = kLetterHeightIn * kDpi;

    if (composite.width() > pageWidthPx || composite.height() > pageHeightPx) {
        QMessageBox box(QMessageBox::Warning, tr("PhotoRust"),
                        tr("The image is larger than the paper's printable area;\n"
                           "some clipping will occur."),
                        QMessageBox::NoButton, this);
        auto *proceed = box.addButton(tr("Proceed"), QMessageBox::AcceptRole);
        box.addButton(QMessageBox::Cancel);
        box.exec();
        if (box.clickedButton() != proceed)
            return;
    }

    const QString path = askForFile(this, tr("Save Print Output As"),
                                    tr("PDF Document (*.pdf *.PDF)"),
                                    QFileDialog::AcceptSave);
    if (path.isEmpty())
        return;

    QPrinter printer(QPrinter::HighResolution);
    printer.setOutputFormat(QPrinter::PdfFormat);
    printer.setOutputFileName(path);
    printer.setPageSize(QPageSize::Letter);
    printer.setCopyCount(1);

    QPainter painter(&printer);
    if (!painter.isActive()) {
        QMessageBox::warning(this, tr("Print"),
                             tr("Could not write \"%1\".").arg(path));
        return;
    }

    const QRectF pageRect = printer.pageLayout().paintRectPixels(printer.resolution());
    const double scaleX = pageRect.width() / (kLetterWidthIn * kDpi);
    const double scaleY = pageRect.height() / (kLetterHeightIn * kDpi);
    const double imgW = composite.width() * scaleX;
    const double imgH = composite.height() * scaleY;
    const double x = (pageRect.width() - imgW) / 2.0;
    const double y = (pageRect.height() - imgH) / 2.0;

    painter.drawImage(QRectF(x, y, imgW, imgH), composite);
    painter.end();

    statusBar()->showMessage(tr("Saved print output to %1").arg(path), 4000);
}

void MainWindow::undo()
{
    m_engine->undo();
    refreshAll();
}

void MainWindow::redo()
{
    m_engine->redo();
    refreshAll();
}

void MainWindow::showFillDialog()
{
    FillDialog dlg(m_engine, this);
    if (dlg.exec() != QDialog::Accepted)
        return;

    float opacity = static_cast<float>(dlg.opacity()) / 100.0f;
    int blendMode = dlg.blendModeIndex();
    if (dlg.isPatternFill()) {
        m_engine->fillPattern(dlg.selectedPatternIndex(), opacity, blendMode);
    } else {
        QColor c = dlg.fillColor();
        m_engine->fillColor(c.red(), c.green(), c.blue(), c.alpha(), opacity, blendMode);
    }
    refreshAll();
}

void MainWindow::showStrokeDialog()
{
    StrokeDialog dlg(m_engine, this);
    if (dlg.exec() != QDialog::Accepted)
        return;

    QColor c = dlg.strokeColor();
    float opacity = static_cast<float>(dlg.opacity()) / 100.0f;
    m_engine->strokeSelection(c.red(), c.green(), c.blue(),
                              dlg.strokeWidth(), opacity, dlg.location());
    refreshAll();
}

void MainWindow::clearSelection()
{
    m_engine->clearSelection();
    refreshAll();
}

void MainWindow::freeTransform()
{
    if (m_canvas) {
        m_canvas->beginFreeTransform();
    }
}

void MainWindow::transformRotate180()
{
    if (m_engine) { m_engine->rotateLayer(180); m_hasTransformed = true; m_transformAgainAction->setEnabled(true); refreshAll(); }
}

void MainWindow::transformRotate90CW()
{
    if (m_engine) { m_engine->rotateLayer(90); m_hasTransformed = true; m_transformAgainAction->setEnabled(true); refreshAll(); }
}

void MainWindow::transformRotate90CCW()
{
    if (m_engine) { m_engine->rotateLayer(270); m_hasTransformed = true; m_transformAgainAction->setEnabled(true); refreshAll(); }
}

void MainWindow::transformFlipHorizontal()
{
    if (m_engine) { m_engine->flipLayer(true); m_hasTransformed = true; m_transformAgainAction->setEnabled(true); refreshAll(); }
}

void MainWindow::transformFlipVertical()
{
    if (m_engine) { m_engine->flipLayer(false); m_hasTransformed = true; m_transformAgainAction->setEnabled(true); refreshAll(); }
}

void MainWindow::findReplaceText()
{
    FindReplaceTextDialog dlg(m_engine, m_canvas, this);
    dlg.exec();
    refreshAll();
}

void MainWindow::showImageSize()
{
    ImageSizeDialog dialog(m_engine, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }

    // Resolution is print metadata and changes no pixels, so it is applied
    // whether or not the image itself was resampled.
    m_engine->setImageResolution(float(dialog.resultResolution()));

    const int mode = dialog.resampleMode();
    if (mode >= 0
        && (dialog.resultWidth() != m_engine->getCanvasWidth()
            || dialog.resultHeight() != m_engine->getCanvasHeight())) {
        m_engine->resampleImage(dialog.resultWidth(), dialog.resultHeight(), mode);
    }
    refreshAll();
}

void MainWindow::showCanvasSize()
{
    CanvasSizeDialog dialog(m_engine, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    const QColor fill = dialog.extensionColor();
    m_engine->resizeCanvasAnchored(dialog.resultWidth(), dialog.resultHeight(),
                                   dialog.anchorX(), dialog.anchorY(),
                                   fill.red(), fill.green(), fill.blue(),
                                   fill.isValid() ? fill.alpha() : 0);
    refreshAll();
}

void MainWindow::rotateCanvas(double degrees)
{
    if (!m_engine) {
        return;
    }
    m_engine->rotateCanvas(float(degrees));
    refreshAll();
}

void MainWindow::flipCanvas(bool horizontal)
{
    if (!m_engine) {
        return;
    }
    m_engine->flipCanvas(horizontal);
    refreshAll();
}

void MainWindow::showArbitraryRotation()
{
    RotateCanvasDialog dialog(this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    rotateCanvas(dialog.degreesClockwise());
}

void MainWindow::cropToSelection()
{
    if (!m_engine || !m_engine->hasSelection()) {
        return;
    }
    // A selection of any shape crops to its bounding box, as in CS6 — the
    // canvas is a rectangle whatever the marquee was drawn with.
    const rust::Vec<::std::int32_t> bounds = m_engine->selectionBounds();
    if (bounds.size() < 4 || bounds[2] < 1 || bounds[3] < 1) {
        return;
    }
    // Pixels outside the new canvas are kept rather than deleted, the same
    // choice Canvas Size makes: enlarging the canvas again brings them back.
    // The Crop tool's "Delete Cropped Pixels" is what discards them.
    m_engine->cropTo(bounds[0], bounds[1], bounds[2], bounds[3], false);
    refreshAll();
}

void MainWindow::showTrim()
{
    if (!m_engine) {
        return;
    }
    TrimDialog dialog(m_engine, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    // A trim that finds nothing to cut reports false and leaves the document
    // — and its history — alone, so there is nothing to refresh.
    if (m_engine->trimImage(dialog.basis(), dialog.trimTop(), dialog.trimBottom(),
                            dialog.trimLeft(), dialog.trimRight())) {
        refreshAll();
    }
}

void MainWindow::showNewLayer()
{
    if (!m_engine) {
        return;
    }
    NewLayerDialog dialog(m_engine, false, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    m_engine->addLayerConfigured(dialog.layerName(), dialog.blendMode(),
                                 dialog.opacityPercent(), dialog.useClippingMask(),
                                 dialog.labelColor());
    refreshAll();
}

void MainWindow::showLayerFromBackground()
{
    if (!m_engine || !m_engine->hasBackgroundLayer()) {
        return;
    }
    NewLayerDialog dialog(m_engine, true, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    // Name, mode and opacity go over together: the conversion is one step in
    // the History panel, not three.
    if (m_engine->layerFromBackground(dialog.layerName(), dialog.blendMode(),
                                      dialog.opacityPercent())) {
        refreshAll();
    }
}

void MainWindow::showLayerStyle(const QString &effect)
{
    if (!m_engine) {
        return;
    }
    // The dialog writes into the engine live and commits on OK, so there is
    // nothing to apply here — only the repaint either outcome needs.
    LayerStyleDialog dialog(m_engine, m_engine->getActiveLayerIndex(), effect, this);
    dialog.exec();
    refreshAll();
}

void MainWindow::showNewFillLayer()
{
    if (!m_engine) {
        return;
    }
    // CS6 asks twice: first what the layer is called and how it blends, then
    // what colour it is.
    NewLayerDialog dialog(m_engine, false, this);
    dialog.setWindowTitle(tr("New Layer"));
    dialog.presetName(m_engine->suggestedLayerName(QStringLiteral("Color Fill")));
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }

    const QColor color = ColorPickerDialog::getColor(m_engine->foregroundColor(), this,
                                                     tr("Pick a Solid Color"));
    if (!color.isValid()) {
        return;
    }

    m_engine->addFillLayer(dialog.layerName(), color.red(), color.green(), color.blue(),
                           dialog.blendMode(), dialog.opacityPercent(),
                           dialog.useClippingMask(), dialog.labelColor());
    refreshAll();
}

void MainWindow::showNewAdjustmentLayer(const QString &kind)
{
    if (!m_engine) {
        return;
    }
    NewLayerDialog dialog(m_engine, false, this);
    dialog.setWindowTitle(tr("New Layer"));
    dialog.presetName(m_engine->suggestedLayerName(kind));
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }

    if (!m_engine->addAdjustmentLayerConfigured(kind, dialog.layerName(), dialog.blendMode(),
                                                dialog.opacityPercent(),
                                                dialog.useClippingMask(),
                                                dialog.labelColor())) {
        return;
    }
    refreshAll();

    // CS6 brings Properties forward on a new adjustment layer, which is where
    // its settings are — the layer arrives at its defaults and is expected to
    // be adjusted straight away.
    if (m_propertiesDock) {
        m_propertiesDock->show();
        m_propertiesDock->raise();
    }

    // Curves is the one adjustment with a curve to draw, so its dialog opens
    // on the new layer — editing what the layer carries rather than the pixels
    // beneath it.
    if (kind == QLatin1String("Curves")) {
        CurvesDialog curves(m_engine, m_engine->getActiveLayerIndex(), this);
        curves.exec();
        refreshAll();
    }
}

void MainWindow::showNewGradientFillLayer()
{
    if (!m_engine) {
        return;
    }
    NewLayerDialog naming(m_engine, false, this);
    naming.setWindowTitle(tr("New Layer"));
    naming.presetName(m_engine->suggestedLayerName(QStringLiteral("Gradient Fill")));
    if (naming.exec() != QDialog::Accepted) {
        return;
    }

    // CS6 makes the layer and *then* asks what it should pour, so the canvas
    // shows the fill while it is being chosen. Nothing reaches the History
    // panel unless the dialog is accepted.
    m_engine->beginFillLayerPreview(naming.layerName(), 1, naming.blendMode(),
                                    naming.opacityPercent(), naming.useClippingMask(),
                                    naming.labelColor());
    refreshAll();

    GradientFillDialog fill(m_engine, this);
    const bool keep = fill.exec() == QDialog::Accepted;
    m_engine->endFillLayerPreview(keep);
    refreshAll();
}

void MainWindow::showNewPatternFillLayer()
{
    if (!m_engine) {
        return;
    }
    NewLayerDialog naming(m_engine, false, this);
    naming.setWindowTitle(tr("New Layer"));
    naming.presetName(m_engine->suggestedLayerName(QStringLiteral("Pattern Fill")));
    if (naming.exec() != QDialog::Accepted) {
        return;
    }

    m_engine->beginFillLayerPreview(naming.layerName(), 2, naming.blendMode(),
                                    naming.opacityPercent(), naming.useClippingMask(),
                                    naming.labelColor());
    refreshAll();

    PatternFillDialog fill(m_engine, this);
    const bool keep = fill.exec() == QDialog::Accepted;
    m_engine->endFillLayerPreview(keep);
    refreshAll();
}

void MainWindow::showDuplicateLayer()
{
    if (!m_engine) {
        return;
    }
    const int index = m_engine->getActiveLayerIndex();
    DuplicateLayerDialog dialog(m_engine, index, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    if (m_engine->duplicateLayerAs(index, dialog.copyName(), dialog.destination(),
                                   dialog.newDocumentName())) {
        refreshAll();
    }
}

QList<int> MainWindow::selectedLayerIndices() const
{
    QList<int> indices = m_layersPanel ? m_layersPanel->selectedIndices() : QList<int>{};
    if (indices.isEmpty() && m_engine) {
        const int active = m_engine->getActiveLayerIndex();
        if (active >= 0) {
            indices.append(active);
        }
    }
    return indices;
}

QVector<int> MainWindow::selectedLayerVector() const
{
    const QList<int> indices = selectedLayerIndices();
    return QVector<int>(indices.begin(), indices.end());
}

void MainWindow::showNewGroup(bool fromSelection)
{
    if (!m_engine) {
        return;
    }
    NewLayerDialog dialog(m_engine, false, this);
    dialog.setWindowTitle(fromSelection ? tr("New Group from Layers") : tr("New Group"));
    dialog.presetName(m_engine->suggestedLayerName(QStringLiteral("Group")));
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }

    if (fromSelection) {
        const QVector<int> indices = selectedLayerVector();
        if (indices.isEmpty() || !m_engine->groupLayers(indices)) {
            return;
        }
        // `groupLayers` leaves the new folder active, so the dialog's settings
        // land on it through the ordinary per-layer calls.
        m_engine->setLayerName(m_engine->getActiveLayerIndex(), dialog.layerName());
    } else {
        m_engine->addLayerGroup(dialog.layerName());
    }

    const int group = m_engine->getActiveLayerIndex();
    m_engine->setLayerBlendMode(group, dialog.blendMode());
    m_engine->setLayerOpacity(group, dialog.opacityPercent());
    m_engine->setLayerLabel(group, dialog.labelColor());
    refreshAll();
}

void MainWindow::groupSelectedLayers()
{
    if (!m_engine) {
        return;
    }
    const QVector<int> indices = selectedLayerVector();
    if (indices.isEmpty() || !m_engine->groupLayers(indices)) {
        return;
    }
    refreshAll();
}

void MainWindow::ungroupSelectedLayers()
{
    if (!m_engine) {
        return;
    }
    if (m_engine->ungroupLayers(m_engine->getActiveLayerIndex())) {
        refreshAll();
    }
}

bool MainWindow::selectedLayersAreHidden() const
{
    if (!m_engine) {
        return false;
    }
    const QList<int> indices = selectedLayerIndices();
    if (indices.isEmpty()) {
        return false;
    }
    for (int index : indices) {
        if (m_engine->layerVisible(index)) {
            return false;
        }
    }
    return true;
}

void MainWindow::toggleSelectedLayersVisible()
{
    if (!m_engine) {
        return;
    }
    const QList<int> indices = selectedLayerIndices();
    if (indices.isEmpty()) {
        return;
    }
    // CS6 hides what is showing: with a mixed selection the entry reads Hide,
    // and hides the lot. Only when every one of them is already hidden does it
    // offer to bring them back.
    const bool hide = !selectedLayersAreHidden();
    for (int index : indices) {
        m_engine->setLayerVisible(index, !hide);
    }
    refreshAll();
}

void MainWindow::quickExportPng()
{
    if (!m_engine) {
        return;
    }
    const QImage composite = m_engine->compositeImage();
    if (composite.isNull()) {
        return;
    }

    // Quick Export asks nothing but where to put it: no format, no quality, no
    // preview. That is the whole point of it next to Export As.
    QString suggested = m_engine->documentName();
    const int dot = suggested.lastIndexOf(QLatin1Char('.'));
    if (dot > 0) {
        suggested.truncate(dot);
    }
    const QString path = askForFile(this, tr("Quick Export as PNG"), tr("PNG (*.png)"),
                                    QFileDialog::AcceptSave, suggested + QStringLiteral(".png"));
    if (path.isEmpty()) {
        return;
    }

    if (composite.save(path, "PNG")) {
        statusBar()->showMessage(tr("Exported to %1").arg(path), 4000);
    } else {
        QMessageBox::warning(this, tr("Quick Export as PNG"),
                             tr("Could not write \"%1\".").arg(path));
    }
}

void MainWindow::showDuplicateImage()
{
    if (!m_engine) {
        return;
    }
    DuplicateImageDialog dialog(m_engine, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    // The copy opens in its own tab and becomes the active document; the tab
    // bar follows the engine's `documentsChanged`.
    m_engine->duplicateDocument(dialog.copyName(), dialog.mergedOnly());
    refreshAll();
    fitOnScreen();
}

void MainWindow::applyAdjustment(const QString &name)
{
    // Adjustments that take parameters get a small prompt; the rest apply
    // straight away. Full CS6-style dialogs with live preview come later.
    float p1 = 0.0f;
    float p2 = 0.0f;
    float p3 = 1.0f;
    bool ok = true;

    if (name == QLatin1String("Posterize")) {
        PosterizeDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Threshold")) {
        ThresholdDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Brightness/Contrast")) {
        BrightnessContrastDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Levels")) {
        LevelsDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Curves")) {
        CurvesDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Exposure")) {
        ExposureDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Vibrance")) {
        VibranceDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Hue/Saturation")) {
        HueSaturationDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Color Balance")) {
        ColorBalanceDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Black & White")) {
        BlackWhiteDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Photo Filter")) {
        PhotoFilterDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Channel Mixer")) {
        ChannelMixerDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Selective Color")) {
        SelectiveColorDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Shadows/Highlights")) {
        ShadowsHighlightsDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("HDR Toning")) {
        // HDR Toning requires a flat image — ask to flatten first.
        if (m_engine->property("layerCount").toInt() > 1) {
            auto answer = QMessageBox::warning(
                this, tr("Adobe Photoshop"),
                tr("HDR Toning requires that the document be flattened before proceeding. "
                   "Flatten and continue?"),
                QMessageBox::Ok | QMessageBox::Cancel,
                QMessageBox::Ok);
            if (answer != QMessageBox::Ok)
                return;
            m_engine->flattenImage();
            refreshAll();
        }
        HdrToningDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    } else if (name == QLatin1String("Equalize")) {
        // No dialog, like Desaturate and Invert.
        m_engine->applyEqualize();
        refreshAll();
        return;
    } else if (name == QLatin1String("Replace Color")) {
        // Shown non-modally, unlike every other adjustment dialog.
        //
        // Its eyedropper has to see clicks on the canvas, and Qt delivers no
        // mouse events whatsoever to a window a modal dialog has blocked — so
        // a modal version cannot sample the image at all, however it filters
        // events. The canvas goes into colour-sampling mode for the dialog's
        // lifetime, which stops the active tool from painting on the image
        // while the user is picking from it.
        if (m_replaceColorDialog) {
            m_replaceColorDialog->raise();
            m_replaceColorDialog->activateWindow();
            return;
        }
        auto *dlg = new ReplaceColorDialog(m_engine, this);
        m_replaceColorDialog = dlg;
        dlg->setAttribute(Qt::WA_DeleteOnClose);
        if (m_canvas) {
            m_canvas->setColorSampling(true);
            connect(m_canvas, &CanvasView::colorSampled,
                    dlg, &ReplaceColorDialog::addSample);
        }
        connect(dlg, &QDialog::finished, this, [this] {
            if (m_canvas) {
                m_canvas->setColorSampling(false);
            }
            m_replaceColorDialog = nullptr;
            refreshAll();
        });
        dlg->show();
        return;
    } else if (name == QLatin1String("Gradient Map")) {
        GradientMapDialog dlg(m_engine, this);
        if (dlg.exec() != QDialog::Accepted) {
            refreshAll();
            return;
        }
        refreshAll();
        return;
    }

    if (!ok) {
        return;
    }
    m_engine->applyAdjustment(name, p1, p2, p3);
    refreshAll();
}

void MainWindow::applyFilter(const QString &name)
{
    float p1 = 0.0f;
    float p2 = 0.0f;
    bool ok = true;

    if (name == QLatin1String("Gaussian Blur")) {
        p1 = float(QInputDialog::getDouble(this, name, tr("Radius (pixels):"), 2.0, 0.1,
                                           250.0, 1, &ok));
    } else if (name == QLatin1String("Unsharp Mask")) {
        p1 = float(QInputDialog::getDouble(this, name, tr("Amount:"), 1.0, 0.0, 5.0, 2,
                                           &ok));
        if (ok) {
            p2 = float(QInputDialog::getDouble(this, name, tr("Radius (pixels):"), 1.0,
                                               0.1, 250.0, 1, &ok));
        }
    } else if (name == QLatin1String("Add Noise")) {
        p1 = float(QInputDialog::getDouble(this, name, tr("Amount (0-1):"), 0.1, 0.0, 1.0,
                                           2, &ok));
    }

    if (!ok) {
        return;
    }
    m_engine->applyFilter(name, p1, p2);
    refreshAll();
}

void MainWindow::zoomIn()
{
    m_canvas->zoomIn();
}

void MainWindow::zoomOut()
{
    m_canvas->zoomOut();
}

void MainWindow::fitOnScreen()
{
    m_canvas->fitToWindow();
}

void MainWindow::actualPixels()
{
    m_canvas->actualPixels();
}

// --------------------------------------------------------------- reactions --

void MainWindow::onToolChanged(ToolId tool, int variant)
{
    // Leaving the Type tool commits whatever text is in progress, the way
    // switching tools does in real Photoshop.
    if (m_activeTool == ToolId::Type && tool != ToolId::Type && m_canvas) {
        m_canvas->commitTypeEdit();
    }

    m_activeTool = tool;
    m_activeVariant = variant;

    m_canvas->setActiveTool(tool);
    if (tool == ToolId::Healing) {
        m_canvas->setHealingType(static_cast<HealingType>(variant));
    }
    // Four tools behind one button: text or mask, each way up. This has to land
    // before the options bar is built, which reads `m_typeVertical` for its
    // alignment buttons.
    if (tool == ToolId::Type) {
        const auto kind = static_cast<TypeTool>(variant);
        m_typeVertical = kind == TypeTool::Vertical || kind == TypeTool::VerticalMask;
        m_canvas->setTypeVertical(m_typeVertical);
        m_canvas->setTypeMask(kind == TypeTool::HorizontalMask
                              || kind == TypeTool::VerticalMask);
    }
    // Aliasing follows the brush variant, and has to be set before the options
    // bar pushes the rest of the brush settings.
    m_pencilMode = tool == ToolId::Brush && brushIsPencil(static_cast<BrushType>(variant));
    m_canvas->setReplaceMode(tool == ToolId::Brush
                             && brushReplacesColor(static_cast<BrushType>(variant)));
    m_canvas->setMixerMode(tool == ToolId::Brush
                           && brushMixesColor(static_cast<BrushType>(variant)));
    m_canvas->setRetouchMode(tool == ToolId::Blur || tool == ToolId::Dodge);
    if (m_engine && tool == ToolId::Blur) {
        // Which of the six strokes is decided in the engine, so the canvas has
        // one path for both buttons rather than six.
        m_engine->setFocusTool(variant);
        pushBlurOptions();
    } else if (m_engine && tool == ToolId::Dodge) {
        m_engine->setToneTool(variant);
        pushToneOptions();
    }
    if (tool == ToolId::Marquee) {
        m_canvas->setMarqueeType(static_cast<MarqueeType>(variant));
    } else if (tool == ToolId::Lasso) {
        m_canvas->setLassoType(static_cast<LassoType>(variant));
    } else if (tool == ToolId::QuickSelect) {
        m_canvas->setQuickSelectType(static_cast<QuickSelectType>(variant));
    } else if (tool == ToolId::Gradient) {
        m_canvas->setGradientTool(static_cast<GradientTool>(variant));
    } else if (tool == ToolId::Pen) {
        m_canvas->setPenTool(static_cast<PenTool>(variant));
    } else if (tool == ToolId::PathSelect) {
        m_canvas->setPathSelectTool(static_cast<PathSelectTool>(variant));
    } else if (tool == ToolId::Hand) {
        m_canvas->setHandTool(static_cast<HandTool>(variant));
    } else if (tool == ToolId::Eraser) {
        m_canvas->setEraserType(static_cast<EraserType>(variant));
    } else if (tool == ToolId::CloneStamp) {
        m_canvas->setCloneTool(static_cast<CloneType>(variant));
    } else if (tool == ToolId::Crop) {
        m_canvas->setCropType(static_cast<CropType>(variant));
    } else if (tool == ToolId::Eyedropper) {
        const auto kind = static_cast<EyedropperType>(variant);
        m_canvas->setEyedropperType(kind);
        // Both the Color Sampler and the Ruler put their numbers in the Info
        // panel rather than the options bar, so bring it forward for either.
        const bool wantsInfo = kind == EyedropperType::ColorSampler
            || kind == EyedropperType::Ruler;
        if (wantsInfo && m_infoDock) {
            m_infoDock->show();
            m_infoDock->raise();
        }
        if (m_infoPanel) {
            m_infoPanel->setRulerMode(kind == EyedropperType::Ruler);
            m_infoPanel->setHint(infoHintFor(kind));
        }
    } else if (m_infoPanel) {
        // Leaving the eyedropper group puts the panel back to CMYK.
        m_infoPanel->setRulerMode(false);
        m_infoPanel->setHint(infoHintFor(EyedropperType::Eyedropper));
    }
    populateOptionsBar(tool, variant);
    statusBar()->showMessage(toolVariantName(tool, variant), 2000);
}

void MainWindow::onDocumentChanged()
{
    m_canvas->refresh();
    updateWindowTitle();
    // Selecting a different type layer re-points the Type options bar at it.
    syncTypeBarToActiveLayer();
}

void MainWindow::onCursorMoved(const QPointF &pos)
{
    m_statusPosition->setText(QStringLiteral("X: %1   Y: %2")
                                  .arg(int(pos.x()))
                                  .arg(int(pos.y())));
}

void MainWindow::onZoomChanged(double zoom)
{
    if (!m_statusZoom) {
        return;
    }
    // Leave the field alone while it is being typed into: overwriting it
    // mid-edit would fight the user, and zoom changes arrive from the wheel and
    // the View menu too.
    if (m_statusZoom->hasFocus()) {
        return;
    }
    // Whole numbers read as "400%", not "400.0%"; fractional stops such as
    // 66.7% keep their decimal.
    const double percent = zoom * 100.0;
    const int decimals = qFuzzyCompare(percent, qRound(percent)) ? 0 : 1;
    m_statusZoom->setText(QStringLiteral("%1%").arg(percent, 0, 'f', decimals));
}

void MainWindow::applyTypedZoom()
{
    if (!m_statusZoom || !m_canvas) {
        return;
    }
    // Accept "400", "400%", "400 %" and a comma decimal separator, since the
    // field is small and people type what is quickest.
    QString text = m_statusZoom->text().trimmed();
    text.remove(QLatin1Char('%'));
    text.replace(QLatin1Char(','), QLatin1Char('.'));

    bool ok = false;
    const double percent = text.trimmed().toDouble(&ok);
    if (!ok || percent <= 0.0) {
        // Unparseable: put the real value back rather than guessing.
        onZoomChanged(m_canvas->zoom());
        return;
    }

    // The canvas clamps to the range CS6 allows, so out-of-range input lands on
    // the nearest limit rather than being rejected.
    m_canvas->setZoom(percent / 100.0);
    // setZoom only signals when the value actually changed, so refresh the text
    // directly — typing 400% when already at 400% should still tidy up "400".
    m_statusZoom->clearFocus();
    onZoomChanged(m_canvas->zoom());
}

void MainWindow::refreshAll()
{
    m_canvas->refresh();
    m_layersPanel->refresh();
    m_historyPanel->refresh();
    m_propertiesPanel->refresh();
    updateWindowTitle();

    m_statusDocSize->setText(tr("%1 × %2 px")
                                 .arg(m_engine->getCanvasWidth())
                                 .arg(m_engine->getCanvasHeight()));
}

void MainWindow::updateWindowTitle()
{
    const QString title = m_engine ? m_engine->getDocumentTitle() : QString();
    setWindowTitle(title.isEmpty() ? tr("PhotoRust")
                                   : tr("%1 — PhotoRust").arg(title));
    if (m_statusDocSize && m_engine) {
        m_statusDocSize->setText(tr("%1 × %2 px")
                                     .arg(m_engine->getCanvasWidth())
                                     .arg(m_engine->getCanvasHeight()));
    }
}

bool MainWindow::confirmDiscardChanges()
{
    if (!m_engine || !m_engine->getModified()) {
        return true;
    }
    // Built by hand rather than through QMessageBox::question so the buttons
    // carry Photoshop's own wording instead of whatever the platform theme
    // substitutes for the standard roles.
    QMessageBox box(QMessageBox::Question, tr("PhotoRust"),
                    tr("Save changes to \"%1\" before closing?")
                        .arg(m_engine->getDocumentTitle()),
                    QMessageBox::NoButton, this);
    QPushButton *save = box.addButton(tr("&Yes"), QMessageBox::AcceptRole);
    QPushButton *discard = box.addButton(tr("&No"), QMessageBox::DestructiveRole);
    box.addButton(tr("Cancel"), QMessageBox::RejectRole);
    box.setDefaultButton(save);
    unsqueezeButtons(&box);
    box.exec();

    if (box.clickedButton() == save) {
        return saveDocument();
    }
    return box.clickedButton() == discard;
}

bool MainWindow::confirmDiscardAll()
{
    if (!m_engine) {
        return true;
    }
    // Every open document gets its own prompt, and is brought into view first so
    // the decision is made while looking at the right image. Any Cancel stops
    // the whole thing.
    for (int i = 0; i < m_engine->documentCount(); ++i) {
        if (!m_engine->documentModifiedAt(i)) {
            continue;
        }
        m_engine->setActiveDocument(i);
        refreshDocumentTabs();
        if (!confirmDiscardChanges()) {
            return false;
        }
    }
    return true;
}

void MainWindow::cut()
{
    if (!copy()) {
        return;
    }
    // Cut is a copy followed by a clear, and the clear is the same one the
    // menu's own Clear does — so a cut leaves exactly what deleting would.
    clearSelection();
}

bool MainWindow::copy()
{
    return copyToClipboard(false);
}

bool MainWindow::copyMerged()
{
    return copyToClipboard(true);
}

bool MainWindow::copyToClipboard(bool merged)
{
    if (!m_engine) {
        return false;
    }

    const QImage region = m_engine->copySelection(merged);
    if (region.isNull()) {
        statusBar()->showMessage(tr("Nothing is selected to copy"), 3000);
        return false;
    }

    // The system clipboard, so what is copied here can be pasted into another
    // application — and what another application copied can be pasted here.
    QGuiApplication::clipboard()->setImage(region);
    // Where it came from, which the clipboard itself cannot carry. Kept so
    // Paste in Place can put it back, and dropped the moment anything else
    // takes the clipboard over.
    m_copyOrigin = QPoint(m_engine->copyOriginX(), m_engine->copyOriginY());
    m_copyIsOurs = true;
    return true;
}

void MainWindow::paste()
{
    // Photoshop drops a plain paste in the middle of what you are looking at,
    // which is where you are working.
    const QImage image = QGuiApplication::clipboard()->image();
    if (image.isNull()) {
        statusBar()->showMessage(tr("The clipboard has no image in it"), 3000);
        return;
    }

    const QPointF centre = m_canvas->widgetToDocument(
        QPointF(m_canvas->width() / 2.0, m_canvas->height() / 2.0));
    const QPoint at(qRound(centre.x()) - image.width() / 2,
                    qRound(centre.y()) - image.height() / 2);
    pasteAt(image, at, 0);
}

void MainWindow::pasteInPlace()
{
    const QImage image = QGuiApplication::clipboard()->image();
    if (image.isNull()) {
        statusBar()->showMessage(tr("The clipboard has no image in it"), 3000);
        return;
    }
    // Only a copy made here knows where it came from; anything from another
    // application has no place in this document and goes to the top-left.
    pasteAt(image, m_copyIsOurs ? m_copyOrigin : QPoint(0, 0), 0);
}

void MainWindow::pasteInto()
{
    pasteConfined(1);
}

void MainWindow::pasteOutside()
{
    pasteConfined(2);
}

void MainWindow::pasteConfined(int mode)
{
    if (!m_engine || !m_engine->hasSelection()) {
        QMessageBox::information(this, tr("Paste"),
                                 tr("Paste Into and Paste Outside need a selection to "
                                    "paste inside or outside of."));
        return;
    }

    const QImage image = QGuiApplication::clipboard()->image();
    if (image.isNull()) {
        statusBar()->showMessage(tr("The clipboard has no image in it"), 3000);
        return;
    }

    // Into and Outside land where they came from when that is known, so the
    // pasted pixels line up with the hole they are going into.
    pasteAt(image, m_copyIsOurs ? m_copyOrigin : QPoint(0, 0), mode);
}

void MainWindow::pasteAt(const QImage &image, const QPoint &at, int mode)
{
    if (!m_engine->pasteImage(image, at.x(), at.y(), mode)) {
        QMessageBox::warning(this, tr("Paste"), tr("Could not paste the clipboard image."));
        return;
    }
    refreshAll();
}

void MainWindow::closeAllDocuments()
{
    // One prompt per unsaved document, and any Cancel abandons the whole thing
    // — the same rule quitting follows, and for the same reason.
    if (!confirmDiscardAll()) {
        return;
    }

    // From the back, so closing one does not renumber the ones still to go.
    for (int i = m_engine->documentCount() - 1; i >= 0; --i) {
        m_engine->closeDocument(i);
    }
    refreshAll();
    refreshDocumentTabs();
}

void MainWindow::showFileInfo()
{
    FileInfoDialog::show(m_engine, m_engine ? m_engine->documentPath() : QString(), this);
}

QStringList MainWindow::recentFiles() const
{
    return QSettings().value(QStringLiteral("recentFiles")).toStringList();
}

void MainWindow::rememberRecentFile(const QString &path)
{
    if (path.isEmpty()) {
        return;
    }

    // Most recent first, no duplicates, and a bounded list — the same shape
    // every application's is, because it is the one that stays useful.
    QStringList files = recentFiles();
    files.removeAll(path);
    files.prepend(path);
    while (files.size() > kMaxRecentFiles) {
        files.removeLast();
    }
    QSettings().setValue(QStringLiteral("recentFiles"), files);
}

void MainWindow::refreshRecentMenu()
{
    if (!m_recentMenu) {
        return;
    }
    m_recentMenu->clear();

    const QStringList files = recentFiles();
    if (files.isEmpty()) {
        QAction *none = m_recentMenu->addAction(tr("No Recent Files"));
        none->setEnabled(false);
        return;
    }

    for (const QString &path : files) {
        // The file name alone would be ambiguous across folders, so the whole
        // path is the tooltip and the name carries the menu.
        QAction *entry = m_recentMenu->addAction(QFileInfo(path).fileName());
        entry->setToolTip(path);
        // A file that has since been moved or deleted is shown greyed rather
        // than dropped: it tells the user where it went, and the list is a
        // history, not an index of what still exists.
        entry->setEnabled(QFileInfo::exists(path));
        connect(entry, &QAction::triggered, this, [this, path] { openPath(path); });
    }

    m_recentMenu->addSeparator();
    QAction *clear = m_recentMenu->addAction(tr("Clear Recent File List"));
    connect(clear, &QAction::triggered, this, [this] {
        QSettings().remove(QStringLiteral("recentFiles"));
        refreshRecentMenu();
    });
}

void MainWindow::closeDocument()
{
    if (m_documentTabs) {
        onTabCloseRequested(m_documentTabs->currentIndex());
    }
}

void MainWindow::closeEvent(QCloseEvent *event)
{
    if (confirmDiscardAll()) {
        event->accept();
    } else {
        event->ignore();
    }
}

// ------------------------------------------------- Transform Options Bar --

void MainWindow::showTransformOptionsBar()
{
    if (!m_canvas || m_transformBarActive) return;
    m_transformBarActive = true;
    m_preTransformTool = m_activeTool;
    m_preTransformVariant = m_activeVariant;

    m_optionsBar->clear();
    m_brushOpacity = nullptr;
    m_brushFlow = nullptr;
    m_brushTipButton = nullptr;
    m_mixerLoadButton = nullptr;

    auto addLabel = [&](const QString &text) {
        auto *l = new QLabel(text, m_optionsBar);
        m_optionsBar->addWidget(l);
    };
    auto addSpin = [&](const QString &suffix, double min, double max,
                       double val, int decimals) -> QDoubleSpinBox * {
        auto *s = new QDoubleSpinBox(m_optionsBar);
        s->setRange(min, max);
        s->setDecimals(decimals);
        s->setSuffix(suffix);
        s->setValue(val);
        s->setButtonSymbols(QAbstractSpinBox::NoButtons);
        s->setFixedWidth(80);
        s->setReadOnly(true);
        m_optionsBar->addWidget(s);
        return s;
    };

    addLabel(tr("X:"));
    addSpin(QStringLiteral(" px"), -99999, 99999, 0, 2);
    addLabel(tr("Y:"));
    addSpin(QStringLiteral(" px"), -99999, 99999, 0, 2);
    m_optionsBar->addSeparator();

    addLabel(tr("W:"));
    addSpin(QStringLiteral("%"), -99999, 99999, 100, 2);
    addLabel(tr("H:"));
    addSpin(QStringLiteral("%"), -99999, 99999, 100, 2);
    m_optionsBar->addSeparator();

    addLabel(QStringLiteral("∠"));
    addSpin(QStringLiteral("°"), -360, 360, 0, 2);
    m_optionsBar->addSeparator();

    addLabel(tr("H:"));
    addSpin(QStringLiteral("°"), -89, 89, 0, 2);
    addLabel(tr("V:"));
    addSpin(QStringLiteral("°"), -89, 89, 0, 2);

    m_optionsBar->addSeparator();

    auto *cancelBtn = new QToolButton(m_optionsBar);
    cancelBtn->setIcon(QIcon::fromTheme(QStringLiteral("dialog-cancel")));
    cancelBtn->setToolTip(tr("Cancel Transform (Esc)"));
    cancelBtn->setText(QStringLiteral("✘"));
    connect(cancelBtn, &QToolButton::clicked, this, [this] {
        if (m_canvas) m_canvas->cancelFreeTransform();
    });
    m_optionsBar->addWidget(cancelBtn);

    auto *commitBtn = new QToolButton(m_optionsBar);
    commitBtn->setIcon(QIcon::fromTheme(QStringLiteral("dialog-ok")));
    commitBtn->setToolTip(tr("Commit Transform (Enter)"));
    commitBtn->setText(QStringLiteral("✔"));
    connect(commitBtn, &QToolButton::clicked, this, [this] {
        if (m_canvas) m_canvas->commitFreeTransform();
    });
    m_optionsBar->addWidget(commitBtn);

    updateTransformReadouts();
}

void MainWindow::hideTransformOptionsBar()
{
    if (!m_transformBarActive) return;
    m_transformBarActive = false;
    populateOptionsBar(m_preTransformTool, m_preTransformVariant);
}

void MainWindow::updateTransformReadouts()
{
    if (!m_transformBarActive || !m_canvas) return;

    QList<QDoubleSpinBox *> spins = m_optionsBar->findChildren<QDoubleSpinBox *>();
    if (spins.size() < 7) return;

    const QRectF orig = m_canvas->transformOrigBounds();
    const QRectF cur = m_canvas->transformBounds();
    const bool isQuad = m_canvas->transformMode() == CanvasView::TransformMode::Skew
                     || m_canvas->transformMode() == CanvasView::TransformMode::Distort
                     || m_canvas->transformMode() == CanvasView::TransformMode::Perspective;

    double cx, cy, wPct, hPct;
    if (isQuad) {
        QRectF qb = m_canvas->transformQuad().boundingRect();
        cx = qb.center().x();
        cy = qb.center().y();
        wPct = orig.width() > 0 ? (qb.width() / orig.width()) * 100.0 : 100.0;
        hPct = orig.height() > 0 ? (qb.height() / orig.height()) * 100.0 : 100.0;
    } else {
        cx = cur.center().x();
        cy = cur.center().y();
        wPct = orig.width() > 0 ? (cur.width() / orig.width()) * 100.0 : 100.0;
        hPct = orig.height() > 0 ? (cur.height() / orig.height()) * 100.0 : 100.0;
    }

    spins[0]->setValue(cx);
    spins[1]->setValue(cy);
    spins[2]->setValue(wPct);
    spins[3]->setValue(hPct);
    spins[4]->setValue(m_canvas->transformRotation());
    spins[5]->setValue(0);
    spins[6]->setValue(0);
}

// ------------------------------------------------- Keyboard Shortcuts --

void MainWindow::editKeyboardShortcuts()
{
    KeyboardShortcutsDialog dlg(m_registry, this);
    dlg.exec();
}

void MainWindow::editColorSettings()
{
    ColorSettingsDialog dlg(this);
    dlg.exec();
}

// ----------------------------------------------------- Auto-Align Layers --

void MainWindow::autoAlignLayers()
{
    if (!m_engine) return;

    const int count = m_engine->property("layerCount").toInt();
    if (count < 2) {
        QMessageBox::information(this, tr("Auto-Align Layers"),
            tr("Auto-Align Layers requires at least two layers."));
        return;
    }

    // Build the dialog.
    QDialog dlg(this);
    dlg.setWindowTitle(tr("Auto-Align Layers"));
    dlg.setMinimumWidth(460);

    auto *mainLayout = new QVBoxLayout(&dlg);

    // Projection group.
    auto *projGroup = new QGroupBox(tr("Projection"), &dlg);
    auto *projLayout = new QGridLayout(projGroup);

    auto *autoRadio = new QRadioButton(tr("Auto"), projGroup);
    auto *perspRadio = new QRadioButton(tr("Perspective"), projGroup);
    auto *collageRadio = new QRadioButton(tr("Collage"), projGroup);
    auto *cylRadio = new QRadioButton(tr("Cylindrical"), projGroup);
    auto *sphRadio = new QRadioButton(tr("Spherical"), projGroup);
    auto *reposRadio = new QRadioButton(tr("Reposition"), projGroup);
    autoRadio->setChecked(true);

    projLayout->addWidget(autoRadio, 0, 0);
    projLayout->addWidget(perspRadio, 0, 1);
    projLayout->addWidget(collageRadio, 0, 2);
    projLayout->addWidget(cylRadio, 1, 0);
    projLayout->addWidget(sphRadio, 1, 1);
    projLayout->addWidget(reposRadio, 1, 2);

    mainLayout->addWidget(projGroup);

    // Lens Correction group.
    auto *lensGroup = new QGroupBox(tr("Lens Correction"), &dlg);
    auto *lensLayout = new QVBoxLayout(lensGroup);
    auto *vignetteCheck = new QCheckBox(tr("Vignette Removal"), lensGroup);
    auto *geoCheck = new QCheckBox(tr("Geometric Distortion"), lensGroup);
    lensLayout->addWidget(vignetteCheck);
    lensLayout->addWidget(geoCheck);
    mainLayout->addWidget(lensGroup);

    // Buttons.
    auto *buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dlg);
    connect(buttons, &QDialogButtonBox::accepted, &dlg, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dlg, &QDialog::reject);
    mainLayout->addWidget(buttons);

    if (dlg.exec() != QDialog::Accepted) return;

    // Check overlap between all layer pairs.
    const int active = m_engine->getActiveLayerIndex();
    QRect activeBounds = m_engine->layerContentBounds(active);
    if (activeBounds.isEmpty()) {
        QMessageBox::information(this, tr("Auto-Align Layers"),
            tr("The active layer has no content."));
        return;
    }

    bool hasOverlap = false;
    for (int i = 0; i < count; ++i) {
        if (i == active) continue;
        QRect other = m_engine->layerContentBounds(i);
        if (other.isEmpty()) continue;
        QRect inter = activeBounds.intersected(other);
        if (!inter.isEmpty()) {
            double overlapArea = double(inter.width()) * inter.height();
            double activeArea = double(activeBounds.width()) * activeBounds.height();
            double otherArea = double(other.width()) * other.height();
            double minArea = std::min(activeArea, otherArea);
            if (minArea > 0 && (overlapArea / minArea) >= 0.1) {
                hasOverlap = true;
                break;
            }
        }
    }

    if (!hasOverlap) {
        QMessageBox::information(this, tr("Auto-Align Layers"),
            tr("Layers do not overlap enough to detect alignment. "
               "In general, images intended for alignment should "
               "overlap by approximately 40%."));
        return;
    }

    QMessageBox::information(this, tr("Auto-Align Layers"),
        tr("Auto-alignment is not yet implemented. "
           "The selected layers overlap sufficiently for alignment."));
}
