#include "ColorSettingsDialog.h"

#include <QBoxLayout>
#include <QDialogButtonBox>
#include <QDir>
#include <QFile>
#include <QGridLayout>
#include <QGroupBox>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPushButton>
#include <QStandardPaths>

// ---------------------------------------------------------------------------
// Preset descriptions — CS6 shows these in the Description box at the bottom
// ---------------------------------------------------------------------------

static const struct {
    const char *name;
    const char *description;
} kPresets[] = {
    {"North America General Purpose 2",
     "North America General Purpose 2:  General-purpose color settings for screen "
     "and print in North America. Profile warnings are disabled."},
    {"North America Prepress 2",
     "North America Prepress 2:  Preparation of content for common printing "
     "conditions in North America. CMYK values are preserved. Profile warnings are enabled."},
    {"North America Web/Internet",
     "North America Web/Internet:  Preparation of content for non-print usage like "
     "the World Wide Web (WWW) in North America. RGB content is converted to sRGB."},
    {"Europe General Purpose 3",
     "Europe General Purpose 3:  General-purpose color settings for screen and "
     "print in Europe. Profile warnings are disabled."},
    {"Europe Prepress 3",
     "Europe Prepress 3:  Preparation of content for common printing conditions "
     "in Europe. CMYK values are preserved. Profile warnings are enabled."},
    {"Europe Web/Internet 2",
     "Europe Web/Internet 2:  Preparation of content for non-print usage like "
     "the World Wide Web (WWW) in Europe. RGB content is converted to sRGB."},
    {"Japan Color for Newspaper",
     "Japan Color for Newspaper:  Preparation of content for newspaper "
     "reproduction in Japan."},
    {"Japan General Purpose 2",
     "Japan General Purpose 2:  General-purpose color settings for screen and "
     "print in Japan. Profile warnings are disabled."},
    {"Japan Magazine Advertisement Color",
     "Japan Magazine Advertisement Color:  Preparation of content for magazine "
     "advertising in Japan."},
    {"Japan Prepress 2",
     "Japan Prepress 2:  Preparation of content for common printing conditions "
     "in Japan. CMYK values are preserved. Profile warnings are enabled."},
    {"Japan Web/Internet",
     "Japan Web/Internet:  Preparation of content for non-print usage like "
     "the World Wide Web (WWW) in Japan. RGB content is converted to sRGB."},
    {"Monitor Color",
     "Monitor Color:  Preparation of content for video and on-screen presentation. "
     "Emulates the behavior of most video applications. Profile warnings are disabled."},
};

// ---------------------------------------------------------------------------
// Working space profile lists — matching CS6's dropdown contents
// ---------------------------------------------------------------------------

static const char *kRgbProfiles[] = {
    "sRGB IEC61966-2.1",
    "Adobe RGB (1998)",
    "Apple RGB",
    "ColorMatch RGB",
    "ProPhoto RGB",
    "CIE RGB",
    "e-sRGB",
    "HDTV (Rec. 709)",
    "PAL/SECAM",
    "ROMM-RGB",
    "SDTV NTSC",
    "SDTV PAL",
    "SMPTE-C",
    "Wide Gamut RGB",
    nullptr
};

static const char *kCmykProfiles[] = {
    "U.S. Web Coated (SWOP) v2",
    "Coated FOGRA27 (ISO 12647-2:2004)",
    "Coated FOGRA39 (ISO 12647-2:2004)",
    "Coated GRACoL 2006 (ISO 12647-2:2004)",
    "Japan Color 2001 Coated",
    "Japan Color 2001 Uncoated",
    "Japan Color 2002 Newspaper",
    "Japan Color 2003 Web Coated",
    "Japan Web Coated (Ad)",
    "U.S. Sheetfed Coated v2",
    "U.S. Sheetfed Uncoated v2",
    "U.S. Web Coated (SWOP) v2",
    "U.S. Web Uncoated v2",
    "Uncoated FOGRA29 (ISO 12647-2:2004)",
    "US Newsprint (SNAP 2007)",
    "Web Coated FOGRA28 (ISO 12647-2:2004)",
    "Web Coated SWOP 2006 Grade 3 Paper",
    "Web Coated SWOP 2006 Grade 5 Paper",
    "Euroscale Coated v2",
    "Euroscale Uncoated v2",
    "Photoshop 4 Default CMYK",
    "Photoshop 5 Default CMYK",
    nullptr
};

static const char *kGrayProfiles[] = {
    "Dot Gain 20%",
    "Dot Gain 10%",
    "Dot Gain 15%",
    "Dot Gain 25%",
    "Dot Gain 30%",
    "Gray Gamma 1.8",
    "Gray Gamma 2.2",
    "sGray",
    nullptr
};

static const char *kSpotProfiles[] = {
    "Dot Gain 20%",
    "Dot Gain 10%",
    "Dot Gain 15%",
    "Dot Gain 25%",
    "Dot Gain 30%",
    nullptr
};

static const char *kDescriptions[] = {
    // Working spaces
    "sRGB IEC61966-2.1:  RGB working space recommended by HP and Microsoft.  This standard "
     "space is endorsed by many hardware and software manufacturers.  It is becoming the "
     "de facto standard for many scanners, low-end printers, and software applications.  Ideal "
     "space for Web work, but not recommended for prepress work (because of its limited color gamut).",
    "Adobe RGB (1998):  Provides a fairly large gamut of RGB colors and consists of colors that "
     "can be displayed on a computer monitor.",
    "U.S. Web Coated (SWOP) v2:  Produces quality separations using U.S. inks under the "
     "following printing conditions: 300% total area of ink coverage, negative plate, coated "
     "publication-grade stock.",
    "Dot Gain 20%:  Uses a space that reflects a dot gain of 20%.",

    // Policies
    "Preserve Embedded Profiles:  Preserves the embedded color profile in a newly opened "
     "document even if the color profile does not match the current working space.",
    "Off:  Does not apply any color management policy when opening documents or importing colors.",
    "Convert to Working:  Converts imported colors to the current working space.",

    // Intents
    "Perceptual:  Aims to preserve the visual relationship between colors.  Colors may "
     "change, but the result is perceived as natural.",
    "Saturation:  Aims to produce vivid colors at the expense of color accuracy.",
    "Relative Colorimetric:  Attempts to match the media-relative Lab coordinates of the "
     "destination colors to the media-relative Lab coordinates of the source colors.  The source "
     "white point is mapped to the destination white point.  Recommended for most color "
     "conversions, especially when most source colors are already inside the destination gamut.",
    "Absolute Colorimetric:  Leaves colors that fall inside the destination gamut unchanged.  "
     "Colors that are out of gamut are clipped.",

    // Working Spaces generic
    "Working Spaces:  The working space specifies the working color profile for each color "
     "model.  (A color profile defines how a color's numeric values map to its visual appearance.) "
     " The working space is used for documents that are not color-managed, and for newly "
     "created documents that are color-managed.",
    nullptr
};

// ---------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------

ColorSettingsDialog::ColorSettingsDialog(QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle(tr("Color Settings"));
    resize(820, 560);
    buildUi();
    loadSettings();
}

static QComboBox *makeCombo(const char *const profiles[], QWidget *parent)
{
    auto *combo = new QComboBox(parent);
    for (int i = 0; profiles[i]; ++i)
        combo->addItem(QString::fromLatin1(profiles[i]));
    return combo;
}

void ColorSettingsDialog::buildUi()
{
    auto *outer = new QVBoxLayout(this);

    // -- Settings preset row -----------------------------------------------
    auto *presetRow = new QHBoxLayout;
    presetRow->addWidget(new QLabel(tr("Settings:")));
    m_settingsPreset = new QComboBox;
    for (const auto &p : kPresets)
        m_settingsPreset->addItem(QString::fromLatin1(p.name));
    m_settingsPreset->setMinimumWidth(260);
    presetRow->addWidget(m_settingsPreset);
    presetRow->addStretch();
    outer->addLayout(presetRow);

    // -- main two-column area: left groups + right groups + buttons ---------
    auto *mainRow = new QHBoxLayout;

    // ---- left column: Working Spaces + Color Management Policies ---------
    auto *leftCol = new QVBoxLayout;

    // Working Spaces group
    auto *wsGroup = new QGroupBox(tr("Working Spaces"));
    auto *wsGrid = new QGridLayout(wsGroup);
    wsGrid->setColumnStretch(1, 1);

    wsGrid->addWidget(new QLabel(tr("RGB:")), 0, 0, Qt::AlignRight);
    m_rgbSpace = makeCombo(kRgbProfiles, wsGroup);
    wsGrid->addWidget(m_rgbSpace, 0, 1);

    wsGrid->addWidget(new QLabel(tr("CMYK:")), 1, 0, Qt::AlignRight);
    m_cmykSpace = makeCombo(kCmykProfiles, wsGroup);
    wsGrid->addWidget(m_cmykSpace, 1, 1);

    wsGrid->addWidget(new QLabel(tr("Gray:")), 2, 0, Qt::AlignRight);
    m_graySpace = makeCombo(kGrayProfiles, wsGroup);
    wsGrid->addWidget(m_graySpace, 2, 1);

    wsGrid->addWidget(new QLabel(tr("Spot:")), 3, 0, Qt::AlignRight);
    m_spotSpace = makeCombo(kSpotProfiles, wsGroup);
    wsGrid->addWidget(m_spotSpace, 3, 1);

    leftCol->addWidget(wsGroup);

    // Color Management Policies group
    auto *cmGroup = new QGroupBox(tr("Color Management Policies"));
    auto *cmGrid = new QGridLayout(cmGroup);
    cmGrid->setColumnStretch(1, 1);

    const char *policies[] = {"Off", "Preserve Embedded Profiles",
                              "Convert to Working", nullptr};

    auto makePolicy = [&](const char *const p[]) {
        auto *c = new QComboBox(cmGroup);
        for (int i = 0; p[i]; ++i)
            c->addItem(QString::fromLatin1(p[i]));
        c->setCurrentIndex(1);
        return c;
    };

    cmGrid->addWidget(new QLabel(tr("RGB:")), 0, 0, Qt::AlignRight);
    m_rgbPolicy = makePolicy(policies);
    cmGrid->addWidget(m_rgbPolicy, 0, 1, 1, 3);

    cmGrid->addWidget(new QLabel(tr("CMYK:")), 1, 0, Qt::AlignRight);
    const char *cmykPolicies[] = {"Off", "Preserve Embedded Profiles",
                                  "Convert to Working CMYK", nullptr};
    m_cmykPolicy = makePolicy(cmykPolicies);
    cmGrid->addWidget(m_cmykPolicy, 1, 1, 1, 3);

    cmGrid->addWidget(new QLabel(tr("Gray:")), 2, 0, Qt::AlignRight);
    m_grayPolicy = makePolicy(policies);
    cmGrid->addWidget(m_grayPolicy, 2, 1, 1, 3);

    m_mismatchAskOpen = new QCheckBox(tr("Ask When Opening"));
    m_mismatchAskPaste = new QCheckBox(tr("Ask When Pasting"));
    m_missingAskOpen = new QCheckBox(tr("Ask When Opening"));

    auto *mismatchRow = new QHBoxLayout;
    mismatchRow->addWidget(new QLabel(tr("Profile Mismatches:")));
    mismatchRow->addWidget(m_mismatchAskOpen);
    mismatchRow->addWidget(m_mismatchAskPaste);
    mismatchRow->addStretch();
    cmGrid->addLayout(mismatchRow, 3, 0, 1, 4);

    auto *missingRow = new QHBoxLayout;
    missingRow->addWidget(new QLabel(tr("Missing Profiles:")));
    missingRow->addWidget(m_missingAskOpen);
    missingRow->addStretch();
    cmGrid->addLayout(missingRow, 4, 0, 1, 4);

    leftCol->addWidget(cmGroup);
    leftCol->addStretch();

    mainRow->addLayout(leftCol, 1);

    // ---- right column: Conversion Options + Advanced Controls ------------
    auto *rightCol = new QVBoxLayout;

    // Conversion Options group
    auto *coGroup = new QGroupBox(tr("Conversion Options"));
    auto *coGrid = new QGridLayout(coGroup);
    coGrid->setColumnStretch(1, 1);

    coGrid->addWidget(new QLabel(tr("Engine:")), 0, 0, Qt::AlignRight);
    m_engine = new QComboBox(coGroup);
    m_engine->addItem(tr("Adobe (ACE)"));
    m_engine->addItem(tr("Apple CMM"));
    m_engine->addItem(tr("Apple ColorSync"));
    coGrid->addWidget(m_engine, 0, 1);

    coGrid->addWidget(new QLabel(tr("Intent:")), 1, 0, Qt::AlignRight);
    m_intent = new QComboBox(coGroup);
    m_intent->addItem(tr("Perceptual"));
    m_intent->addItem(tr("Saturation"));
    m_intent->addItem(tr("Relative Colorimetric"));
    m_intent->addItem(tr("Absolute Colorimetric"));
    m_intent->setCurrentIndex(2);
    coGrid->addWidget(m_intent, 1, 1);

    m_blackPoint = new QCheckBox(tr("Use Black Point Compensation"));
    m_blackPoint->setChecked(true);
    coGrid->addWidget(m_blackPoint, 2, 0, 1, 2);

    m_dither = new QCheckBox(tr("Use Dither (8-bit/channel images)"));
    m_dither->setChecked(true);
    coGrid->addWidget(m_dither, 3, 0, 1, 2);

    m_sceneReferred = new QCheckBox(tr("Compensate for Scene-referred Profiles"));
    m_sceneReferred->setChecked(true);
    coGrid->addWidget(m_sceneReferred, 4, 0, 1, 2);

    rightCol->addWidget(coGroup);

    // Advanced Controls group
    auto *advGroup = new QGroupBox(tr("Advanced Controls"));
    auto *advGrid = new QGridLayout(advGroup);

    m_desaturateCheck = new QCheckBox(tr("Desaturate Monitor Colors By:"));
    m_desaturateSpin = new QSpinBox(advGroup);
    m_desaturateSpin->setRange(0, 100);
    m_desaturateSpin->setValue(20);
    m_desaturateSpin->setSuffix(tr(" %"));
    m_desaturateSpin->setEnabled(false);
    advGrid->addWidget(m_desaturateCheck, 0, 0);
    advGrid->addWidget(m_desaturateSpin, 0, 1);

    m_blendRgbCheck = new QCheckBox(tr("Blend RGB Colors Using Gamma:"));
    m_blendRgbSpin = new QDoubleSpinBox(advGroup);
    m_blendRgbSpin->setRange(0.01, 9.99);
    m_blendRgbSpin->setDecimals(2);
    m_blendRgbSpin->setValue(1.00);
    m_blendRgbSpin->setEnabled(false);
    advGrid->addWidget(m_blendRgbCheck, 1, 0);
    advGrid->addWidget(m_blendRgbSpin, 1, 1);

    m_blendTextCheck = new QCheckBox(tr("Blend Text Colors Using Gamma:"));
    m_blendTextCheck->setChecked(true);
    m_blendTextSpin = new QDoubleSpinBox(advGroup);
    m_blendTextSpin->setRange(0.01, 9.99);
    m_blendTextSpin->setDecimals(2);
    m_blendTextSpin->setValue(1.45);
    advGrid->addWidget(m_blendTextCheck, 2, 0);
    advGrid->addWidget(m_blendTextSpin, 2, 1);

    auto *advNote = new QLabel(
        tr("For more information on color settings, search for "
           "\"setting up color management\" in Help."));
    advNote->setWordWrap(true);
    advGrid->addWidget(advNote, 3, 0, 1, 2);

    rightCol->addWidget(advGroup);
    rightCol->addStretch();

    mainRow->addLayout(rightCol, 1);

    // ---- button column ---------------------------------------------------
    auto *btnCol = new QVBoxLayout;
    btnCol->setSpacing(6);

    auto *okBtn = new QPushButton(tr("OK"));
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    auto *loadBtn = new QPushButton(tr("Load..."));
    auto *saveBtn = new QPushButton(tr("Save..."));
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);

    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addWidget(loadBtn);
    btnCol->addWidget(saveBtn);
    btnCol->addSpacing(8);
    btnCol->addWidget(m_preview);
    btnCol->addStretch();

    mainRow->addLayout(btnCol);

    outer->addLayout(mainRow, 1);

    // -- Description box at bottom -----------------------------------------
    auto *descGroup = new QGroupBox(tr("Description"));
    auto *descLayout = new QVBoxLayout(descGroup);
    m_descLabel = new QLabel;
    m_descLabel->setWordWrap(true);
    m_descLabel->setMinimumHeight(40);
    descLayout->addWidget(m_descLabel);
    outer->addWidget(descGroup);

    // -- wiring ------------------------------------------------------------
    connect(m_desaturateCheck, &QCheckBox::toggled,
            m_desaturateSpin, &QSpinBox::setEnabled);
    connect(m_blendRgbCheck, &QCheckBox::toggled,
            m_blendRgbSpin, &QDoubleSpinBox::setEnabled);
    connect(m_blendTextCheck, &QCheckBox::toggled,
            m_blendTextSpin, &QDoubleSpinBox::setEnabled);

    connect(m_settingsPreset, &QComboBox::currentIndexChanged,
            this, &ColorSettingsDialog::onSettingsPresetChanged);

    auto descFromCombo = [this](QComboBox *combo) {
        connect(combo, &QComboBox::currentIndexChanged, this, [this, combo] {
            updateDescription(combo->currentText());
        });
    };
    descFromCombo(m_rgbSpace);
    descFromCombo(m_cmykSpace);
    descFromCombo(m_graySpace);
    descFromCombo(m_spotSpace);
    descFromCombo(m_rgbPolicy);
    descFromCombo(m_cmykPolicy);
    descFromCombo(m_grayPolicy);
    descFromCombo(m_intent);

    connect(okBtn, &QPushButton::clicked, this, [this] {
        saveSettings();
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);

    // Set initial description
    onSettingsPresetChanged(0);
}

void ColorSettingsDialog::updateDescription(const QString &text)
{
    for (int i = 0; kDescriptions[i]; ++i) {
        if (QString::fromLatin1(kDescriptions[i]).startsWith(text + QLatin1Char(':'))) {
            m_descLabel->setText(QString::fromLatin1(kDescriptions[i]));
            return;
        }
    }
    m_descLabel->setText(text);
}

void ColorSettingsDialog::onSettingsPresetChanged(int index)
{
    if (index < 0 || index >= int(std::size(kPresets)))
        return;
    m_descLabel->setText(QString::fromLatin1(kPresets[index].description));
}

QString ColorSettingsDialog::configPath()
{
    return QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
           + QStringLiteral("/color_settings.json");
}

static void setComboByText(QComboBox *combo, const QString &text)
{
    int idx = combo->findText(text);
    if (idx >= 0)
        combo->setCurrentIndex(idx);
}

void ColorSettingsDialog::loadSettings()
{
    QFile file(configPath());
    if (!file.open(QIODevice::ReadOnly))
        return;

    QJsonParseError err{};
    const QJsonDocument doc = QJsonDocument::fromJson(file.readAll(), &err);
    if (!doc.isObject())
        return;

    const QJsonObject o = doc.object();

    setComboByText(m_settingsPreset, o.value(QStringLiteral("preset")).toString());

    setComboByText(m_rgbSpace, o.value(QStringLiteral("ws_rgb")).toString());
    setComboByText(m_cmykSpace, o.value(QStringLiteral("ws_cmyk")).toString());
    setComboByText(m_graySpace, o.value(QStringLiteral("ws_gray")).toString());
    setComboByText(m_spotSpace, o.value(QStringLiteral("ws_spot")).toString());

    setComboByText(m_rgbPolicy, o.value(QStringLiteral("policy_rgb")).toString());
    setComboByText(m_cmykPolicy, o.value(QStringLiteral("policy_cmyk")).toString());
    setComboByText(m_grayPolicy, o.value(QStringLiteral("policy_gray")).toString());

    m_mismatchAskOpen->setChecked(o.value(QStringLiteral("mismatch_ask_open")).toBool());
    m_mismatchAskPaste->setChecked(o.value(QStringLiteral("mismatch_ask_paste")).toBool());
    m_missingAskOpen->setChecked(o.value(QStringLiteral("missing_ask_open")).toBool());

    setComboByText(m_engine, o.value(QStringLiteral("engine")).toString());
    setComboByText(m_intent, o.value(QStringLiteral("intent")).toString());
    m_blackPoint->setChecked(o.value(QStringLiteral("black_point")).toBool(true));
    m_dither->setChecked(o.value(QStringLiteral("dither")).toBool(true));
    m_sceneReferred->setChecked(o.value(QStringLiteral("scene_referred")).toBool(true));

    m_desaturateCheck->setChecked(o.value(QStringLiteral("desaturate_enabled")).toBool());
    m_desaturateSpin->setValue(o.value(QStringLiteral("desaturate_pct")).toInt(20));
    m_blendRgbCheck->setChecked(o.value(QStringLiteral("blend_rgb_enabled")).toBool());
    m_blendRgbSpin->setValue(o.value(QStringLiteral("blend_rgb_gamma")).toDouble(1.00));
    m_blendTextCheck->setChecked(o.value(QStringLiteral("blend_text_enabled")).toBool(true));
    m_blendTextSpin->setValue(o.value(QStringLiteral("blend_text_gamma")).toDouble(1.45));
}

void ColorSettingsDialog::saveSettings()
{
    const QString path = configPath();
    QDir().mkpath(QFileInfo(path).absolutePath());

    QJsonObject o;
    o.insert(QStringLiteral("preset"), m_settingsPreset->currentText());

    o.insert(QStringLiteral("ws_rgb"), m_rgbSpace->currentText());
    o.insert(QStringLiteral("ws_cmyk"), m_cmykSpace->currentText());
    o.insert(QStringLiteral("ws_gray"), m_graySpace->currentText());
    o.insert(QStringLiteral("ws_spot"), m_spotSpace->currentText());

    o.insert(QStringLiteral("policy_rgb"), m_rgbPolicy->currentText());
    o.insert(QStringLiteral("policy_cmyk"), m_cmykPolicy->currentText());
    o.insert(QStringLiteral("policy_gray"), m_grayPolicy->currentText());

    o.insert(QStringLiteral("mismatch_ask_open"), m_mismatchAskOpen->isChecked());
    o.insert(QStringLiteral("mismatch_ask_paste"), m_mismatchAskPaste->isChecked());
    o.insert(QStringLiteral("missing_ask_open"), m_missingAskOpen->isChecked());

    o.insert(QStringLiteral("engine"), m_engine->currentText());
    o.insert(QStringLiteral("intent"), m_intent->currentText());
    o.insert(QStringLiteral("black_point"), m_blackPoint->isChecked());
    o.insert(QStringLiteral("dither"), m_dither->isChecked());
    o.insert(QStringLiteral("scene_referred"), m_sceneReferred->isChecked());

    o.insert(QStringLiteral("desaturate_enabled"), m_desaturateCheck->isChecked());
    o.insert(QStringLiteral("desaturate_pct"), m_desaturateSpin->value());
    o.insert(QStringLiteral("blend_rgb_enabled"), m_blendRgbCheck->isChecked());
    o.insert(QStringLiteral("blend_rgb_gamma"), m_blendRgbSpin->value());
    o.insert(QStringLiteral("blend_text_enabled"), m_blendTextCheck->isChecked());
    o.insert(QStringLiteral("blend_text_gamma"), m_blendTextSpin->value());

    QFile file(path);
    if (file.open(QIODevice::WriteOnly | QIODevice::Truncate))
        file.write(QJsonDocument(o).toJson(QJsonDocument::Indented));
}
