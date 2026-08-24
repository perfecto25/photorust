#include "GifWriter.h"

#include <QBuffer>
#include <QFile>
#include <QIODevice>
#include <QVector>

#include <algorithm>
#include <cstring>

namespace {

struct LzwState
{
    int minCodeSize;
    int clearCode;
    int eoiCode;
    int nextCode;
    int codeSize;

    struct Entry {
        int prefix;
        int suffix;
    };
    QVector<Entry> table;
    QByteArray output;
    int bitBuffer = 0;
    int bitsInBuffer = 0;

    void init(int colorBits)
    {
        minCodeSize = qMax(colorBits, 2);
        clearCode = 1 << minCodeSize;
        eoiCode = clearCode + 1;
        reset();
    }

    void reset()
    {
        codeSize = minCodeSize + 1;
        nextCode = eoiCode + 1;
        table.clear();
        table.resize(4096);
        for (int i = 0; i < (1 << minCodeSize); ++i) {
            table[i] = {-1, i};
        }
    }

    void emitCode(int code)
    {
        bitBuffer |= (code << bitsInBuffer);
        bitsInBuffer += codeSize;
        while (bitsInBuffer >= 8) {
            output.append(static_cast<char>(bitBuffer & 0xFF));
            bitBuffer >>= 8;
            bitsInBuffer -= 8;
        }
    }

    void flush()
    {
        if (bitsInBuffer > 0) {
            output.append(static_cast<char>(bitBuffer & 0xFF));
            bitBuffer = 0;
            bitsInBuffer = 0;
        }
    }

    int findEntry(int prefix, int suffix) const
    {
        for (int i = eoiCode + 1; i < nextCode; ++i) {
            if (table[i].prefix == prefix && table[i].suffix == suffix)
                return i;
        }
        return -1;
    }
};

void writeLzw(QIODevice *dev, const quint8 *pixels, int count, int colorBits)
{
    LzwState lzw;
    lzw.init(colorBits);

    dev->write(reinterpret_cast<const char *>(&lzw.minCodeSize), 1);

    lzw.emitCode(lzw.clearCode);

    int current = pixels[0];
    for (int i = 1; i < count; ++i) {
        int pixel = pixels[i];
        int found = lzw.findEntry(current, pixel);
        if (found >= 0) {
            current = found;
        } else {
            lzw.emitCode(current);
            if (lzw.nextCode < 4096) {
                lzw.table[lzw.nextCode] = {current, pixel};
                lzw.nextCode++;
                if (lzw.nextCode > (1 << lzw.codeSize) && lzw.codeSize < 12)
                    lzw.codeSize++;
            } else {
                lzw.emitCode(lzw.clearCode);
                lzw.reset();
            }
            current = pixel;
        }
    }
    lzw.emitCode(current);
    lzw.emitCode(lzw.eoiCode);
    lzw.flush();

    // Write sub-blocks (max 255 bytes each)
    const char *data = lzw.output.constData();
    int remaining = lzw.output.size();
    while (remaining > 0) {
        int blockSize = qMin(remaining, 255);
        quint8 size = static_cast<quint8>(blockSize);
        dev->write(reinterpret_cast<const char *>(&size), 1);
        dev->write(data, blockSize);
        data += blockSize;
        remaining -= blockSize;
    }
    // Block terminator
    quint8 zero = 0;
    dev->write(reinterpret_cast<const char *>(&zero), 1);
}

void writeU16LE(QIODevice *dev, quint16 value)
{
    quint8 buf[2] = {static_cast<quint8>(value & 0xFF),
                     static_cast<quint8>((value >> 8) & 0xFF)};
    dev->write(reinterpret_cast<const char *>(buf), 2);
}

} // namespace

QByteArray encodeGif(const QImage &image)
{
    QImage indexed = image.convertToFormat(QImage::Format_Indexed8);
    const QVector<QRgb> colorTable = indexed.colorTable();
    const int colorCount = colorTable.size();

    int colorBits = 1;
    while ((1 << colorBits) < colorCount)
        colorBits++;
    if (colorBits < 2)
        colorBits = 2;
    const int tableSize = 1 << colorBits;

    QByteArray result;
    QBuffer buf(&result);
    buf.open(QIODevice::WriteOnly);

    // Header
    buf.write("GIF89a", 6);

    // Logical Screen Descriptor
    writeU16LE(&buf, static_cast<quint16>(indexed.width()));
    writeU16LE(&buf, static_cast<quint16>(indexed.height()));
    quint8 packed = 0x80                                      // Global Color Table flag
                    | ((colorBits - 1) << 4)                  // Color Resolution
                    | (colorBits - 1);                        // Size of Global Color Table
    buf.write(reinterpret_cast<const char *>(&packed), 1);
    quint8 bgColor = 0;
    buf.write(reinterpret_cast<const char *>(&bgColor), 1);
    quint8 aspectRatio = 0;
    buf.write(reinterpret_cast<const char *>(&aspectRatio), 1);

    // Global Color Table
    for (int i = 0; i < tableSize; ++i) {
        quint8 rgb[3];
        if (i < colorCount) {
            rgb[0] = static_cast<quint8>(qRed(colorTable[i]));
            rgb[1] = static_cast<quint8>(qGreen(colorTable[i]));
            rgb[2] = static_cast<quint8>(qBlue(colorTable[i]));
        } else {
            rgb[0] = rgb[1] = rgb[2] = 0;
        }
        buf.write(reinterpret_cast<const char *>(rgb), 3);
    }

    // Check for transparency
    int transparentIndex = -1;
    for (int i = 0; i < colorCount; ++i) {
        if (qAlpha(colorTable[i]) < 128) {
            transparentIndex = i;
            break;
        }
    }

    if (transparentIndex >= 0) {
        // Graphic Control Extension for transparency
        quint8 gce[] = {0x21, 0xF9, 0x04, 0x01, 0x00, 0x00,
                        static_cast<quint8>(transparentIndex), 0x00};
        buf.write(reinterpret_cast<const char *>(gce), 8);
    }

    // Image Descriptor
    quint8 separator = 0x2C;
    buf.write(reinterpret_cast<const char *>(&separator), 1);
    writeU16LE(&buf, 0);  // left
    writeU16LE(&buf, 0);  // top
    writeU16LE(&buf, static_cast<quint16>(indexed.width()));
    writeU16LE(&buf, static_cast<quint16>(indexed.height()));
    quint8 imgPacked = 0; // no local color table, not interlaced
    buf.write(reinterpret_cast<const char *>(&imgPacked), 1);

    // Image Data (LZW)
    const int pixelCount = indexed.width() * indexed.height();
    QVector<quint8> pixels(pixelCount);
    for (int y = 0; y < indexed.height(); ++y) {
        const quint8 *line = indexed.constScanLine(y);
        std::memcpy(pixels.data() + y * indexed.width(), line, indexed.width());
    }
    writeLzw(&buf, pixels.constData(), pixelCount, colorBits);

    // Trailer
    quint8 trailer = 0x3B;
    buf.write(reinterpret_cast<const char *>(&trailer), 1);

    buf.close();
    return result;
}

bool writeGif(const QImage &image, const QString &path)
{
    QByteArray data = encodeGif(image);
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly))
        return false;
    return file.write(data) == data.size();
}
