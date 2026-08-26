#pragma once

#include <QColor>
#include <QDialog>
#include <QLabel>
#include <QLineEdit>
#include <QSpinBox>
#include <QWidget>

class Engine;

struct GradientColorStop {
    float position; // 0..1
    QColor color;
};

class GradientStopBar : public QWidget
{
    Q_OBJECT
public:
    explicit GradientStopBar(QWidget *parent = nullptr);

    void setStops(const QVector<GradientColorStop> &stops);
    QVector<GradientColorStop> stops() const { return m_stops; }

    int selectedIndex() const { return m_selected; }

    QSize sizeHint() const override;

signals:
    void stopsChanged();
    void stopSelected(int index);

protected:
    void paintEvent(QPaintEvent *) override;
    void mousePressEvent(QMouseEvent *) override;
    void mouseMoveEvent(QMouseEvent *) override;
    void mouseReleaseEvent(QMouseEvent *) override;
    void mouseDoubleClickEvent(QMouseEvent *) override;

private:
    QRect barRect() const;
    int hitTest(const QPoint &pos) const;
    float posFromX(int x) const;

    QVector<GradientColorStop> m_stops;
    int m_selected = -1;
    int m_dragging = -1;
    bool m_dragRemove = false;
};

class GradientEditorDialog : public QDialog
{
    Q_OBJECT
public:
    explicit GradientEditorDialog(Engine *engine,
                                  const QVector<GradientColorStop> &initial,
                                  QWidget *parent = nullptr);

    QVector<GradientColorStop> resultStops() const;

    static QVector<GradientColorStop> stopsFromPresetName(
        Engine *engine, const QString &name);

protected:
    bool eventFilter(QObject *obj, QEvent *event) override;

private:
    void onStopsChanged();
    void onStopSelected(int index);
    void updatePreview();
    void pickStopColor();

    Engine *m_engine = nullptr;

    GradientStopBar *m_stopBar = nullptr;
    QLabel *m_previewLabel = nullptr;
    QLineEdit *m_nameEdit = nullptr;
    QSpinBox *m_locationSpin = nullptr;
    QLabel *m_colorSwatch = nullptr;
};
