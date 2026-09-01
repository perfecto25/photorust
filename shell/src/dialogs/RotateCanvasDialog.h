#pragma once

#include <QDialog>

class QDoubleSpinBox;
class QRadioButton;

/// Photoshop's Image ▸ Image Rotation ▸ Arbitrary.
///
/// An angle and which way to turn. The engine works in clockwise degrees, so
/// the two radio buttons are only a sign on the number the dialog reports.
class RotateCanvasDialog : public QDialog
{
    Q_OBJECT
public:
    explicit RotateCanvasDialog(QWidget *parent = nullptr);

    /// The chosen angle, always clockwise.
    double degreesClockwise() const;

private:
    QDoubleSpinBox *m_angle = nullptr;
    QRadioButton *m_clockwise = nullptr;
};
