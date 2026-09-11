"""Generate deterministic, synthetic engineering fixtures; no private research content."""
from hashlib import sha256
from pathlib import Path
import io
import json
import shutil

from PIL import Image, ImageDraw, ImageFont
from reportlab.lib.utils import ImageReader
from reportlab.pdfgen import canvas

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "tests" / "fixtures" / "generated"
OUT.mkdir(parents=True, exist_ok=True)
PAGE = (612, 792)
records = []

def make_pdf(name, pages, draw):
    path = OUT / name
    doc = canvas.Canvas(str(path), pagesize=PAGE, invariant=1, pageCompression=1)
    doc.setTitle("Read Atlas synthetic fixture")
    doc.setAuthor("Read Atlas contributors")
    for number in range(1, pages + 1):
        draw(doc, number)
        doc.showPage()
    doc.save()
    records.append({"file": name, "pages": pages, "sha256": sha256(path.read_bytes()).hexdigest()})

def header(doc, number, title):
    doc.setFillColorRGB(.12, .18, .22)
    doc.setFont("Helvetica-Bold", 20)
    doc.drawString(48, 730, title)
    doc.setFont("Helvetica", 10)
    doc.drawString(48, 705, "Synthetic regression material - no real study or scientific claim.")
    doc.drawString(48, 36, f"Read Atlas | Page {number}")

def text_page(doc, number):
    header(doc, number, "A simple reading experiment")
    doc.setFont("Helvetica-Bold", 13)
    doc.drawString(48, 660, "1. Question")
    doc.setFont("Helvetica", 11)
    doc.drawString(48, 638, "How does a reader compare two explanations of the same concept?")
    doc.drawString(48, 616, "This document contains invented content for software validation.")
    doc.setFont("Helvetica-Bold", 13)
    doc.drawString(48, 576, "2. Method")
    doc.setFont("Helvetica", 11)
    for i, line in enumerate([
        "Read each paragraph, identify the question, and return to the evidence.",
        "The observations below are synthetic. Do not interpret them as findings.",
        "Use this page to test text extraction, rendering, zoom and navigation.",
    ]):
        doc.drawString(48, 553-i*22, line)

def table_page(doc, number):
    header(doc, number, "Tables, equations and a figure")
    doc.setFont("Helvetica", 12)
    doc.drawString(48, 655, "Example equation: E = m c^2")
    doc.drawString(48, 632, "Mean example: (x1 + x2 + x3) / 3")
    labels = [["Condition", "Count", "Synthetic score"], ["A", "10", "0.4"], ["B", "10", "0.7"]]
    for r, row in enumerate(labels):
        y=570-r*36
        for col, value in enumerate(row):
            doc.rect(48+col*165, y-12, 165, 36)
            doc.drawString(58+col*165,y,value)
    doc.setStrokeColorRGB(.2,.4,.5)
    doc.line(65,260,65,430); doc.line(65,260,510,260)
    doc.setFillColorRGB(.2,.5,.6)
    doc.rect(135,260,70,70,fill=1); doc.rect(300,260,70,125,fill=1)
    doc.setFillColorRGB(.12,.18,.22)
    doc.drawString(135,235,"A");doc.drawString(300,235,"B")
    doc.drawString(48,190,"Figure 1. Invented values for testing selection and image rendering.")

def scan_page(doc, number):
    im=Image.new("RGB",(1224,1584),"white")
    d=ImageDraw.Draw(im)
    font=ImageFont.load_default(size=30)
    d.text((96,130),"SYNTHETIC SCANNED PAGE",font=font,fill="black")
    for i in range(10):
        d.text((96,250+i * 70),"Image-only line "+str(i+1)+": OCR test content.",font=font,fill="black")
    stream=io.BytesIO()
    im.save(stream,format="PNG")
    doc.drawImage(ImageReader(io.BytesIO(stream.getvalue())),0,0,width=612,height=792)

make_pdf("born-digital.pdf",3,text_page)
make_pdf("tables-and-formulas.pdf",2,table_page)
make_pdf("scanned-no-ocr.pdf",2,scan_page)
make_pdf("long-500-pages.pdf",500,text_page)
for name in ["duplicate-copy.pdf","moved-source.pdf"]:
    shutil.copyfile(OUT/"born-digital.pdf",OUT/name)
    records.append({"file":name,"pages":3,"sha256":sha256((OUT/name).read_bytes()).hexdigest()})
(OUT/"manifest.json").write_text(json.dumps({"generator":"tools/generate-pdf-fixtures.py","license":"MIT","synthetic":True,"files":records},indent=2)+"\n",encoding="utf-8")
print(f"Generated {len(records)} synthetic PDFs in tests/fixtures/generated/.")
