#pragma once

#include <QColor>
#include <QIcon>
#include <QPointF>
#include <QWidget>

class Engine;
class QGridLayout;
class QLabel;

/// The Info panel.
///
/// CS6's live readout of whatever is under the cursor, in the layout the
/// original uses: RGB and CMYK across the top, cursor position and selection
/// size beneath, then a two-column grid of colour samplers, the document's
/// memory footprint, and a hint line.
///
/// Everything here is a readout — the panel never writes to the document. It
/// is driven by the canvas's `cursorMoved` signal and by the engine's
/// selection and annotation change signals.
class InfoPanel : public QWidget
{
    Q_OBJECT

public:
    explicit InfoPanel(Engine *engine, QWidget *parent = nullptr);

public slots:
    /// The cursor moved to a document-space point. Off-canvas positions blank
    /// the colour readouts rather than showing the last valid sample, which is
    /// what CS6 does when you leave the image.
    void setCursorPosition(const QPointF &documentPos);
    /// The cursor left the canvas entirely.
    void clearCursorPosition();
    /// Rebuild the sampler grid and refresh its values.
    void refreshSamplers();
    /// Update the selection width and height. Ignored in ruler mode, where
    /// the W/H block belongs to the ruler instead.
    void refreshSelection();
    /// Update the ruler's angle, length and deltas.
    void refreshRuler();

    /// Swap the panel between its normal layout and the Ruler tool's.
    ///
    /// CS6 replaces the CMYK block with the ruler's **A** (angle) and **L**
    /// (length), and repoints W/H at the ruler's deltas rather than the
    /// selection's size.
    void setRulerMode(bool on);
    /// Update the "Doc:" line.
    void refreshDocumentSize();
    /// Everything at once, for a document swap.
    void refresh();

    /// The hint line at the bottom, which CS6 changes per tool.
    void setHint(const QString &hint);

private:
    /// One labelled readout block: an icon, then rows of "K : value".
    struct Readout {
        /// The block widget, so the whole thing can be swapped out when the
        /// panel changes mode.
        QWidget *widget = nullptr;
        QList<QLabel *> values;
    };

    /// Build a block of `keys` rows at `row`/`column` of the grid. `tag` is
    /// the sampler number, shown on the first row when present.
    Readout *addReadout(QGridLayout *grid, int row, int column, const QStringList &keys,
                        const QIcon &icon, const QString &footer = QString(),
                        const QString &tag = QString());
    /// Set a block's values, or blank them all when `values` is empty.
    static void setValues(Readout *readout, const QStringList &values);

    Engine *m_engine = nullptr;

    QGridLayout *m_grid = nullptr;
    Readout *m_rgb = nullptr;
    /// The top-right block: CMYK normally, the ruler's A/L in ruler mode.
    Readout *m_cmyk = nullptr;
    bool m_rulerMode = false;
    Readout *m_position = nullptr;
    Readout *m_size = nullptr;
    /// One block per placed sampler, rebuilt when the sampler list changes.
    QList<Readout *> m_samplers;
    /// Widgets belonging to the sampler rows, so they can be torn down without
    /// disturbing the fixed blocks above them.
    QList<QWidget *> m_samplerWidgets;

    QLabel *m_docSize = nullptr;
    QLabel *m_hint = nullptr;

    /// Grid row the sampler blocks start at; the rows above are fixed.
    static constexpr int kSamplerFirstRow = 2;
};
