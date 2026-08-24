#include "FindReplaceTextDialog.h"

#include "canvas/CanvasView.h"
#include "photorust_core/src/bridge.cxxqt.h"

#include <QCheckBox>
#include <QFont>
#include <QFontMetricsF>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QImage>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPainter>
#include <QPushButton>
#include <QVBoxLayout>

FindReplaceTextDialog::FindReplaceTextDialog(Engine *engine, CanvasView *canvas,
                                               QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
    , m_canvas(canvas)
{
    setWindowTitle(tr("Find And Replace Text"));
    setFixedSize(480, 300);

    auto *mainLayout = new QVBoxLayout(this);
    mainLayout->setSpacing(10);
    mainLayout->setContentsMargins(16, 16, 16, 16);

    // -- Find What / Change To fields --
    auto *fieldGrid = new QGridLayout;
    fieldGrid->setSpacing(6);

    fieldGrid->addWidget(new QLabel(tr("Find What:")), 0, 0, Qt::AlignRight);
    m_findEdit = new QLineEdit;
    fieldGrid->addWidget(m_findEdit, 0, 1);

    fieldGrid->addWidget(new QLabel(tr("Change To:")), 1, 0, Qt::AlignRight);
    m_changeEdit = new QLineEdit;
    fieldGrid->addWidget(m_changeEdit, 1, 1);

    mainLayout->addLayout(fieldGrid);

    // -- Checkboxes --
    auto *checkLayout = new QVBoxLayout;
    checkLayout->setSpacing(4);

    m_searchAllLayers = new QCheckBox(tr("Search All Layers"));
    m_searchAllLayers->setChecked(true);
    checkLayout->addWidget(m_searchAllLayers);

    m_forward = new QCheckBox(tr("Forward"));
    m_forward->setChecked(true);
    checkLayout->addWidget(m_forward);

    m_caseSensitive = new QCheckBox(tr("Case Sensitive"));
    checkLayout->addWidget(m_caseSensitive);

    m_wholeWord = new QCheckBox(tr("Whole Word Only"));
    checkLayout->addWidget(m_wholeWord);

    m_ignoreAccents = new QCheckBox(tr("Ignore Accents"));
    checkLayout->addWidget(m_ignoreAccents);

    mainLayout->addLayout(checkLayout);
    mainLayout->addStretch();

    // -- Buttons --
    auto *btnLayout = new QHBoxLayout;
    btnLayout->setSpacing(6);

    auto *doneBtn = new QPushButton(tr("Done"));
    m_findNextBtn = new QPushButton(tr("Find Next"));
    m_changeBtn = new QPushButton(tr("Change"));
    m_changeAllBtn = new QPushButton(tr("Change All"));
    m_changeFindBtn = new QPushButton(tr("Change/Find"));

    btnLayout->addWidget(doneBtn);
    btnLayout->addStretch();
    btnLayout->addWidget(m_findNextBtn);
    btnLayout->addWidget(m_changeBtn);
    btnLayout->addWidget(m_changeAllBtn);
    btnLayout->addWidget(m_changeFindBtn);

    mainLayout->addLayout(btnLayout);

    connect(doneBtn, &QPushButton::clicked, this, &QDialog::accept);
    connect(m_findNextBtn, &QPushButton::clicked, this, &FindReplaceTextDialog::findNext);
    connect(m_changeBtn, &QPushButton::clicked, this, &FindReplaceTextDialog::changeText);
    connect(m_changeAllBtn, &QPushButton::clicked, this, &FindReplaceTextDialog::changeAll);
    connect(m_changeFindBtn, &QPushButton::clicked, this, &FindReplaceTextDialog::changeAndFind);
    connect(m_findEdit, &QLineEdit::textChanged, this, &FindReplaceTextDialog::updateButtons);

    updateButtons();
}

FindReplaceTextDialog::~FindReplaceTextDialog()
{
    if (m_canvas) {
        m_canvas->clearSearchHighlight();
    }
}

QString FindReplaceTextDialog::layerFullText(int layerIndex) const
{
    const int runCount = m_engine->layerTextRunCount(layerIndex);
    QString result;
    for (int r = 0; r < runCount; ++r) {
        result += m_engine->layerTextRunText(layerIndex, r);
    }
    return result;
}

static QString stripAccents(const QString &s)
{
    QString norm = s.normalized(QString::NormalizationForm_KD);
    QString out;
    out.reserve(norm.size());
    for (const QChar &ch : norm) {
        if (ch.category() != QChar::Mark_NonSpacing) {
            out.append(ch);
        }
    }
    return out;
}

FindReplaceTextDialog::Match FindReplaceTextDialog::findInLayer(int layerIndex, int startChar) const
{
    Match result;
    const QString needle = m_findEdit->text();
    if (needle.isEmpty()) {
        return result;
    }

    QString haystack = layerFullText(layerIndex);
    QString searchNeedle = needle;

    if (m_ignoreAccents->isChecked()) {
        haystack = stripAccents(haystack);
        searchNeedle = stripAccents(searchNeedle);
    }

    const auto cs = m_caseSensitive->isChecked() ? Qt::CaseSensitive : Qt::CaseInsensitive;
    const bool forward = m_forward->isChecked();

    int pos = -1;
    if (forward) {
        int from = qMax(0, startChar);
        pos = haystack.indexOf(searchNeedle, from, cs);
    } else {
        int from = (startChar < 0) ? haystack.length() - 1 : startChar;
        pos = haystack.lastIndexOf(searchNeedle, from, cs);
    }

    if (pos < 0) {
        return result;
    }

    if (m_wholeWord->isChecked()) {
        auto isWordChar = [](QChar c) { return c.isLetterOrNumber() || c == QLatin1Char('_'); };
        while (pos >= 0) {
            bool atStart = (pos == 0 || !isWordChar(haystack[pos - 1]));
            bool atEnd = (pos + searchNeedle.length() >= haystack.length()
                         || !isWordChar(haystack[pos + searchNeedle.length()]));
            if (atStart && atEnd) {
                break;
            }
            if (forward) {
                pos = haystack.indexOf(searchNeedle, pos + 1, cs);
            } else {
                pos = haystack.lastIndexOf(searchNeedle, pos - 1, cs);
            }
        }
        if (pos < 0) {
            return result;
        }
    }

    result.layerIndex = layerIndex;
    result.charOffset = pos;
    result.length = needle.length();
    return result;
}

void FindReplaceTextDialog::findNext()
{
    if (!m_engine || m_findEdit->text().isEmpty()) {
        return;
    }

    const int layerCount = m_engine->getLayerCount();
    if (layerCount <= 0) {
        QMessageBox::information(this, tr("Find And Replace Text"),
                                 tr("The search text was not found."));
        return;
    }

    int startLayer = (m_currentMatch.layerIndex >= 0) ? m_currentMatch.layerIndex : 0;
    int startChar = -1;
    if (m_currentMatch.layerIndex >= 0) {
        startChar = m_forward->isChecked()
            ? m_currentMatch.charOffset + 1
            : m_currentMatch.charOffset - 1;
    }

    auto tryLayer = [&](int layerIndex, int fromChar) -> bool {
        if (m_engine->layerKind(layerIndex) != 2) {
            return false;
        }
        Match m = findInLayer(layerIndex, fromChar);
        if (m.layerIndex >= 0) {
            m_currentMatch = m;
            m_engine->setActiveLayer(layerIndex);
            if (m_canvas) {
                m_canvas->setSearchHighlight(m.layerIndex, m.charOffset, m.length);
            }
            updateButtons();
            return true;
        }
        return false;
    };

    if (tryLayer(startLayer, startChar)) {
        return;
    }

    if (m_searchAllLayers->isChecked()) {
        const bool forward = m_forward->isChecked();
        for (int i = 1; i < layerCount; ++i) {
            int idx = forward
                ? (startLayer + i) % layerCount
                : (startLayer - i + layerCount) % layerCount;
            if (tryLayer(idx, -1)) {
                return;
            }
        }
    }

    if (m_canvas) {
        m_canvas->clearSearchHighlight();
    }
    QMessageBox::information(this, tr("Find And Replace Text"),
                             tr("The search text was not found."));
    m_currentMatch = Match();
    updateButtons();
}

bool FindReplaceTextDialog::replaceMatch(const Match &match)
{
    if (match.layerIndex < 0 || !m_engine) {
        return false;
    }

    const int layerIndex = match.layerIndex;
    const int runCount = m_engine->layerTextRunCount(layerIndex);
    if (runCount <= 0) {
        return false;
    }

    // Build full text and run info.
    struct RunInfo {
        QString text;
        QString family;
        QString style;
        float size;
        QColor color;
    };
    QList<RunInfo> runs;
    QString fullText;

    for (int r = 0; r < runCount; ++r) {
        RunInfo ri;
        ri.text = m_engine->layerTextRunText(layerIndex, r);
        ri.family = m_engine->layerTextRunFamily(layerIndex, r);
        ri.style = m_engine->layerTextRunStyle(layerIndex, r);
        ri.size = m_engine->layerTextRunSize(layerIndex, r);
        ri.color = m_engine->layerTextRunColor(layerIndex, r);
        runs.append(ri);
        fullText += ri.text;
    }

    // Perform the replacement in the full text.
    const QString replacement = m_changeEdit->text();
    QString newText = fullText.left(match.charOffset) + replacement
                    + fullText.mid(match.charOffset + match.length);

    // Rebuild runs with the new text, preserving formatting.
    // Map each character position in the new text back to a run's formatting.
    const int delta = replacement.length() - match.length;
    QList<RunInfo> newRuns;
    int pos = 0;
    for (const RunInfo &ri : std::as_const(runs)) {
        int runStart = pos;
        int runEnd = pos + ri.text.length();
        pos = runEnd;

        int newRunStart = runStart;
        int newRunEnd = runEnd;

        if (runStart >= match.charOffset + match.length) {
            newRunStart += delta;
            newRunEnd += delta;
        } else if (runEnd <= match.charOffset) {
            // Before the replacement, unchanged.
        } else {
            // This run overlaps the replacement region.
            if (runStart < match.charOffset) {
                newRunStart = runStart;
            } else {
                newRunStart = match.charOffset;
            }
            if (runEnd > match.charOffset + match.length) {
                newRunEnd = runEnd + delta;
            } else {
                newRunEnd = match.charOffset + replacement.length();
            }
        }

        if (newRunEnd > newRunStart && newRunStart < newText.length()) {
            newRunEnd = qMin(newRunEnd, newText.length());
            RunInfo nr = ri;
            nr.text = newText.mid(newRunStart, newRunEnd - newRunStart);
            newRuns.append(nr);
        }
    }

    // If runs ended up empty, bail.
    if (newRuns.isEmpty()) {
        return false;
    }

    // Render the text to an image using the same approach as the Type tool.
    const int align = m_engine->layerTextAlign(layerIndex);
    const bool antialias = m_engine->layerTextAntialias(layerIndex);
    const bool vertical = m_engine->layerTextVertical(layerIndex);
    const float originX = m_engine->layerTextOriginX(layerIndex);
    const float originY = m_engine->layerTextOriginY(layerIndex);

    // Lay out text and compute bounds.
    struct GlyphLine {
        QFont font;
        QColor color;
        QString text;
        qreal x, y, ascent, width, height;
    };
    QList<QList<GlyphLine>> lines;
    {
        QStringList textLines = newText.split(QLatin1Char('\n'));
        qreal yOffset = 0;
        qreal xOffset = 0;

        for (const QString &lineStr : textLines) {
            QList<GlyphLine> lineGlyphs;
            qreal lineX = 0;
            qreal lineHeight = 0;

            // Map characters in this line back to runs.
            int lineCharStart = 0;
            for (int li = 0; li < textLines.indexOf(lineStr); ++li) {
                lineCharStart += textLines[li].length() + 1; // +1 for \n
            }

            // Walk the runs that overlap this line.
            int charPos = 0;
            for (const RunInfo &ri : std::as_const(newRuns)) {
                int runStart = charPos;
                int runEnd = charPos + ri.text.length();
                charPos = runEnd;

                // Find overlap with this line.
                // The line spans [lineCharStart, lineCharStart + lineStr.length())
                int overlapStart = qMax(runStart, lineCharStart);
                int overlapEnd = qMin(runEnd, lineCharStart + lineStr.length());
                if (overlapStart >= overlapEnd) {
                    continue;
                }

                QString segText = newText.mid(overlapStart, overlapEnd - overlapStart);
                QFont font(ri.family);
                font.setStyleName(ri.style);
                font.setPixelSize(qRound(ri.size));
                QFontMetricsF fm(font);

                GlyphLine gl;
                gl.font = font;
                gl.color = ri.color;
                gl.text = segText;
                gl.ascent = fm.ascent();
                gl.width = fm.horizontalAdvance(segText);
                gl.height = fm.height();

                if (vertical) {
                    gl.x = 0;
                    gl.y = lineX;
                    lineX += gl.height;
                } else {
                    gl.x = lineX;
                    gl.y = 0;
                    lineX += gl.width;
                }

                lineHeight = qMax(lineHeight, fm.height());
                lineGlyphs.append(gl);
            }

            if (!lineGlyphs.isEmpty()) {
                // Apply offsets.
                for (GlyphLine &gl : lineGlyphs) {
                    if (vertical) {
                        gl.x += xOffset;
                    } else {
                        gl.y += yOffset;
                    }
                }
            }

            lines.append(lineGlyphs);
            if (vertical) {
                xOffset -= lineHeight;  // columns go right to left
            } else {
                yOffset += lineHeight;
            }
        }
    }

    // Compute bounding box.
    QRectF bbox;
    for (const auto &line : std::as_const(lines)) {
        for (const GlyphLine &gl : line) {
            QRectF r(gl.x, gl.y, vertical ? gl.height : gl.width,
                     vertical ? gl.width : gl.height);
            bbox = bbox.united(r);
        }
    }

    const int pad = 2;
    QPointF origin(originX, originY);
    QRect pixelBounds = bbox.translated(origin).toAlignedRect().adjusted(-pad, -pad, pad, pad);

    QImage image(qMax(1, pixelBounds.width()), qMax(1, pixelBounds.height()),
                 QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::transparent);

    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing, antialias);
    painter.setRenderHint(QPainter::TextAntialiasing, antialias);

    for (const auto &line : std::as_const(lines)) {
        for (const GlyphLine &gl : line) {
            painter.setFont(gl.font);
            painter.setPen(gl.color);
            QPointF drawPos(origin.x() + gl.x - pixelBounds.left(),
                            origin.y() + gl.y + gl.ascent - pixelBounds.top());
            painter.drawText(drawPos, gl.text);
        }
    }
    painter.end();

    // Update through the bridge.
    QString layerName = newText.section(QLatin1Char('\n'), 0, 0).trimmed();
    if (layerName.isEmpty()) {
        layerName = tr("Type Layer");
    }

    m_engine->beginTextRuns();
    for (const RunInfo &ri : std::as_const(newRuns)) {
        m_engine->addTextRun(ri.text, ri.family, ri.style, ri.size, ri.color);
    }

    m_engine->beginTextEdit(layerIndex);
    m_engine->updateTextLayer(layerIndex, image, pixelBounds.left(), pixelBounds.top(),
                              layerName, align, antialias, vertical, originX, originY);

    return true;
}

void FindReplaceTextDialog::changeText()
{
    if (m_currentMatch.layerIndex < 0) {
        return;
    }
    replaceMatch(m_currentMatch);
    m_currentMatch = Match();
    if (m_canvas) {
        m_canvas->clearSearchHighlight();
    }
    updateButtons();
}

void FindReplaceTextDialog::changeAll()
{
    if (!m_engine || m_findEdit->text().isEmpty()) {
        return;
    }

    int count = 0;
    const int layerCount = m_engine->getLayerCount();

    for (int pass = 0; pass < 100; ++pass) {
        bool found = false;
        for (int i = 0; i < layerCount; ++i) {
            if (m_engine->layerKind(i) != 2) {
                continue;
            }
            if (!m_searchAllLayers->isChecked()) {
                int active = m_engine->getActiveLayerIndex();
                if (i != active) {
                    continue;
                }
            }

            Match m = findInLayer(i, -1);
            if (m.layerIndex >= 0) {
                replaceMatch(m);
                ++count;
                found = true;
                break;
            }
        }
        if (!found) {
            break;
        }
    }

    m_currentMatch = Match();
    updateButtons();

    QMessageBox::information(this, tr("Find And Replace Text"),
                             tr("%1 replacement(s) made.").arg(count));
}

void FindReplaceTextDialog::changeAndFind()
{
    if (m_currentMatch.layerIndex >= 0) {
        replaceMatch(m_currentMatch);
        m_currentMatch = Match();
    }
    findNext();
}

void FindReplaceTextDialog::updateButtons()
{
    const bool hasText = !m_findEdit->text().isEmpty();
    const bool hasMatch = m_currentMatch.layerIndex >= 0;

    m_findNextBtn->setEnabled(hasText);
    m_changeBtn->setEnabled(hasMatch);
    m_changeAllBtn->setEnabled(hasText);
    m_changeFindBtn->setEnabled(hasMatch);
}
