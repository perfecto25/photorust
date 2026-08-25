#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QDoubleSpinBox>
#include <QImage>
#include <QMouseEvent>
#include <QSpinBox>
#include <QWidget>

class Engine;

class HistogramWidget : public QWidget
{
    Q_OBJECT
public:
    explicit HistogramWidget(QWidget *parent = nullptr);
    void setImage(const QImage &img, int channel = 0);

protected:
    void paintEvent(QPaintEvent *) override;

private:
    int m_histogram[256]{};
    int m_peak = 1;
};

class TriangleSlider : public QWidget
{
    Q_OBJECT
public:
    explicit TriangleSlider(int count, int globalMin, int globalMax, QWidget *parent = nullptr);
    void setRange(int index, int min, int max);
    void setValue(int index, int val);
    void setColor(int index, const QColor &c);
    int value(int index) const;

signals:
    void valueChanged(int index, int value);

protected:
    void paintEvent(QPaintEvent *) override;
    void mousePressEvent(QMouseEvent *) override;
    void mouseMoveEvent(QMouseEvent *) override;
    void mouseReleaseEvent(QMouseEvent *) override;

private:
    int xForValue(int index) const;
    int valueForX(int index, int x) const;

    struct Thumb { int min; int max; int val; QColor color; };
    QVector<Thumb> m_thumbs;
    int m_globalMin;
    int m_globalMax;
    int m_dragging = -1;
    static constexpr int kMargin = 6;
    static constexpr int kThumbH = 10;
};

class LevelsDialog : public QDialog
{
    Q_OBJECT
public:
    explicit LevelsDialog(Engine *engine, QWidget *parent = nullptr);
    ~LevelsDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();
    void rebuildHistogram();
    void applyPreset(int index);
    void markCustom();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;
    bool m_updatingFromPreset = false;
    QImage m_originalImage;

    QComboBox *m_presetCombo = nullptr;
    QComboBox *m_channelCombo = nullptr;
    HistogramWidget *m_histogram = nullptr;

    TriangleSlider *m_inputSlider = nullptr;
    QSpinBox *m_inBlack = nullptr;
    QDoubleSpinBox *m_gamma = nullptr;
    QSpinBox *m_inWhite = nullptr;

    TriangleSlider *m_outputSlider = nullptr;
    QSpinBox *m_outBlack = nullptr;
    QSpinBox *m_outWhite = nullptr;

    QCheckBox *m_preview = nullptr;
};
