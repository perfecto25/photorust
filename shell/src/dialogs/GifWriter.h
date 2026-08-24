#pragma once

#include <QImage>
#include <QString>

/// Write a QImage as a GIF file. Qt's GIF plugin is read-only, so this
/// provides a minimal GIF89a encoder with LZW compression.
///
/// The image is quantised to at most 256 colours (Indexed8). Returns true
/// on success.
bool writeGif(const QImage &image, const QString &path);

/// Encode the image as GIF into a byte array.
QByteArray encodeGif(const QImage &image);
