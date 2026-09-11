# Synthetic PDF regression fixtures

Run `npm run fixtures` after installing Python with `reportlab` and `Pillow` (`python -m pip install reportlab Pillow`). Set `READ_DESKTOP_PYTHON` to choose a Python executable if needed.

The generator writes deterministic, MIT-licensed synthetic documents into the ignored `generated/` directory:

- `born-digital.pdf`: selectable text.
- `scanned-no-ocr.pdf`: image-only pages, no text layer.
- `tables-and-formulas.pdf`: a drawn table, equation notation and figure.
- `long-500-pages.pdf`: navigation, virtual rendering and memory checks.
- `duplicate-copy.pdf`: byte-identical to born-digital for deduplication.
- `moved-source.pdf`: a disposable source for external movement tests.

`manifest.json` records page counts and SHA-256. These are engineering fixtures, not real scientific material or a substitute for scholarly content-quality review.

仓库不提交私人论文。真实 PDF 只用于有权读取的本地验收，记录 hash、页数、机器和结果；不要将原文或个人工作区上传。
