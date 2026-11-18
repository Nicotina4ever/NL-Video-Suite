"""Read-only photo evidence. No identity inference, downloads or EXIF rewriting."""
import hashlib
import json
import sys
from PIL import Image, ImageOps, ExifTags

source = sys.argv[1]
digest = hashlib.sha256()
with open(source, 'rb') as stream:
    for block in iter(lambda: stream.read(1024 * 1024), b''):
        digest.update(block)
with Image.open(source) as original:
    exif = original.getexif()
    dates = {}
    for tag, value in exif.items():
        name = ExifTags.TAGS.get(tag, str(tag))
        if name in ('DateTimeOriginal', 'DateTimeDigitized', 'DateTime', 'OffsetTimeOriginal'):
            dates[name] = str(value)
    try:
        for tag, value in exif.get_ifd(34665).items():
            name = ExifTags.TAGS.get(tag, str(tag))
            if name in ('DateTimeOriginal', 'DateTimeDigitized', 'OffsetTimeOriginal'):
                dates[name] = str(value)
    except (KeyError, TypeError, AttributeError):
        pass
    image = ImageOps.exif_transpose(original).convert('RGB')
    width, height = image.size
    gray = list(image.resize((9, 8), Image.Resampling.LANCZOS).convert('L').getdata())
    bits = 0
    for y in range(8):
        for x in range(8):
            bits = (bits << 1) | int(gray[y * 9 + x] > gray[y * 9 + x + 1])
    print(json.dumps({
        'sha256': digest.hexdigest(), 'dhash': format(bits, '016x'),
        'width': width, 'height': height,
        'orientation': 'portrait' if height > width else 'landscape' if width > height else 'square',
        'exif_orientation': exif.get(274, 1), 'exif_dates': dates,
        'capture_date_verified': False,
        'date_notice': 'EXIF dichiarati dal file, non verificati. Nome WhatsApp/importazione non sono date di scatto.',
        'quality_notice': 'Dimensioni misurate; nitidezza, risoluzione originale e qualità artistica richiedono revisione.'
    }, ensure_ascii=False))
