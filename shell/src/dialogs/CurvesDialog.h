#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QImage>
#include <QMouseEvent>
#include <QWidget>

class Engine;

class CurveWidget : public QWidget
{
    Q_OBJECT
public:
    explicit CurveWidget(QWidget *parent = nullptr);

    void setHistogram(const QImage &img, int channel);
    void setShowHistogram(bool v) { m_showHisto = v; update(); }
    void setShowBaseline(bool v) { m_showBaseline = v; update(); }

    void resetCurve();
    void setPoints(const QVector<QPointF> &pts);
    QVector<QPointF> points() const { return m_points; }

    void buildLut(uint8_t lut[256]) const;

signals:
    void curveChanged();

protected:
    void paintEvent(QPaintEvent *) override;
    void mousePressEvent(QMouseEvent *) override;
    void mouseMoveEvent(QMouseEvent *) override;
    void mouseReleaseEvent(QMouseEvent *) override;

private:
    void interpolate();
    QPointF toWidget(QPointF p) const;
    QPointF fromWidget(QPointF p) const;

    QVector<QPointF> m_points;
    float m_curve[256]{};
    int m_dragging = -1;
    int m_histo[256]{};
    int m_histoPeak = 1;
    bool m_showHisto = true;
    bool m_showBaseline = true;
    static constexpr int kSize = 256;
};

class CurvesDialog : public QDialog
{
    Q_OBJECT
public:
    explicit CurvesDialog(Engine *engine, QWidget *parent = nullptr);
    ~CurvesDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void rebuildHistogram();
    void applyPreset(int index);

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;
    bool m_applyingPreset = false;
    QImage m_originalImage;

    QComboBox *m_presetCombo = nullptr;
    QComboBox *m_channelCombo = nullptr;
    CurveWidget *m_curveWidget = nullptr;
    QCheckBox *m_preview = nullptr;
    QCheckBox *m_histoCheck = nullptr;
    QCheckBox *m_baselineCheck = nullptr;
};
