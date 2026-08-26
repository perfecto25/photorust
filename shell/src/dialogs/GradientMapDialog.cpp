#include "GradientMapDialog.h"
#include "GradientEditorDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QEvent>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

static const char *kGradientNames[] = {
    "Foreground to Background",
    "Foreground to Transparent",
    "Black, White",
    "Red, Green",
    "Violet, Orange",
    "Blue, Red, Yellow",
    "Blue, Yellow, Blue",
    "Orange, Yellow, Orange",
    "Violet, Green, Orange",
    "Yellow, Violet, Orange, Blue",
    "Copper",
    "Chrome",
    "Spectrum",
    "Transparent Rainbow",
    "Transparent Stripes",
};

static constexpr int kGradientCount = static_cast<int>(std::size(kGradientNames));

static QString stopsToString(const QVector<GradientColorStop> &stops)
{
    QStringList parts;
    for (const auto &s : stops) {
        parts.append(QStringLiteral("%1,%2,%3,%4")
                         .arg(static_cast<double>(s.position), 0, 'f', 4)
                         .arg(s.color.red())
                         .arg(s.color.green())
                         .arg(s.color.blue()));
    }
    return parts.join(QLatin1Char(';'));
}

GradientMapDialog::GradientMapDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
    , m_currentGradient(QStringLiteral("Foreground to Background"))
{
    setWindowTitle(tr("Gradient Map"));
    setFixedSize(420, 200);

    auto *outer = new QHBoxLayout(this);

    // -- left column -----------------------------------------------------------
    auto *left = new QVBoxLayout;

    // "Gradient Used for Grayscale Mapping" group
    auto *gradGroup = new QGroupBox(tr("Gradient Used for Grayscale Mapping"));
    auto *gradLayout = new QVBoxLayout(gradGroup);

    m_gradientBtn = new QToolButton;
    m_gradientBtn->setPopupMode(QToolButton::MenuButtonPopup);
    m_gradientBtn->setIconSize(QSize(200, 20));
    m_gradientBtn->setToolButtonStyle(Qt::ToolButtonIconOnly);
    m_gradientBtn->setMinimumWidth(210);

    // Clicking the button directly opens the editor;
    // the dropdown arrow shows the preset menu
    auto *presetMenu = new QMenu(m_gradientBtn);
    for (int i = 0; i < kGradientCount; ++i) {
        const QString name = QString::fromUtf8(kGradientNames[i]);
        QImage img = m_engine->gradientPreview(name, 64, 16);
        auto *action = presetMenu->addAction(QIcon(QPixmap::fromImage(img)), name);
        connect(action, &QAction::triggered, this, [this, name] {
            m_currentGradient = name;
            m_useCustom = false;
            updateSwatchIcon();
            onValueChanged();
        });
    }
    m_gradientBtn->setMenu(presetMenu);

    connect(m_gradientBtn, &QToolButton::clicked,
            this, &GradientMapDialog::openGradientEditor);

    gradLayout->addWidget(m_gradientBtn);
    left->addWidget(gradGroup);

    left->addSpacing(4);

    // Gradient Options group
    auto *optGroup = new QGroupBox(tr("Gradient Options"));
    auto *optLayout = new QVBoxLayout(optGroup);

    m_dither = new QCheckBox(tr("Dither"));
    optLayout->addWidget(m_dither);

    m_reverse = new QCheckBox(tr("Reverse"));
    optLayout->addWidget(m_reverse);

    left->addWidget(optGroup);

    left->addStretch();

    outer->addLayout(left, 1);

    // -- right column: buttons -------------------------------------------------
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(70);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(70);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addSpacing(10);
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    btnCol->addWidget(m_preview);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    // -- connections -----------------------------------------------------------
    connect(m_dither, &QCheckBox::toggled, this, [this] { onValueChanged(); });
    connect(m_reverse, &QCheckBox::toggled, this, [this] { onValueChanged(); });

    connect(m_preview, &QCheckBox::toggled, this, [this](bool checked) {
        if (checked)
            applyPreview();
        else
            revertPreview();
    });

    connect(okBtn, &QPushButton::clicked, this, [this] {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);

    updateSwatchIcon();
    applyPreview();
}

GradientMapDialog::~GradientMapDialog()
{
    revertPreview();
}

void GradientMapDialog::onValueChanged()
{
    applyPreview();
}

void GradientMapDialog::updateSwatchIcon()
{
    if (!m_engine) return;
    if (m_useCustom) {
        QImage img = m_engine->customGradientPreview(
            stopsToString(m_customStops), 200, 20);
        m_gradientBtn->setIcon(QIcon(QPixmap::fromImage(img)));
    } else {
        QImage img = m_engine->gradientPreview(m_currentGradient, 200, 20);
        m_gradientBtn->setIcon(QIcon(QPixmap::fromImage(img)));
    }
}

void GradientMapDialog::openGradientEditor()
{
    QVector<GradientColorStop> initial;
    if (m_useCustom) {
        initial = m_customStops;
    } else {
        initial = GradientEditorDialog::stopsFromPresetName(m_engine, m_currentGradient);
    }

    GradientEditorDialog editor(m_engine, initial, this);
    if (editor.exec() == QDialog::Accepted) {
        m_customStops = editor.resultStops();
        m_useCustom = true;
        updateSwatchIcon();
        onValueChanged();
    }
}

void GradientMapDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    if (m_useCustom) {
        m_engine->applyGradientMapCustom(
            stopsToString(m_customStops),
            m_reverse->isChecked(),
            m_dither->isChecked());
    } else {
        m_engine->applyGradientMap(
            m_currentGradient,
            m_reverse->isChecked(),
            m_dither->isChecked());
    }
    m_previewApplied = true;
}

void GradientMapDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

bool GradientMapDialog::eventFilter(QObject *obj, QEvent *event)
{
    return QDialog::eventFilter(obj, event);
}
