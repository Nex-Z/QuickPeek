"""Deterministic synthetic files; no user documents are modified."""
from pathlib import Path
import json, random, zipfile
from PIL import Image

root = Path('target/perf-fixtures')
root.mkdir(parents=True, exist_ok=True)
for mb in (5, 10):
    target = mb * 1_000_000
    for ext, line in {
        'log': '2026-09-26 12:30:00 INFO preview item 1234567890 completed successfully\n',
        'js': 'const preview = {name: "QuickPeek", count: 123}; // performance fixture\n',
        'csv': '12345,QuickPeek,Preview performance,2026-09-26,Ready,1024,Local,Sample\n',
    }.items():
        (root/f'{mb}mb.{ext}').write_text(line*(target//len(line)), encoding='utf-8')
    for ext,line in {'md':'## Heading\n\nThis is a paragraph of a large Markdown document for preview performance.\n\n','html':'<p>This is a paragraph of a large HTML document for preview performance.</p>\n'}.items():
        (root/f'{mb}mb.{ext}').write_text(line*(target//len(line)),encoding='utf-8',newline='\n')
    record = '{"name":"QuickPeek","count":12345678901234567890,"enabled":true}'
    (root/f'{mb}mb.json').write_text('['+','.join([record]*(target//(len(record)+1)))+']',encoding='utf-8')
rng = random.Random(42)
Image.frombytes('RGB',(2048,1536),rng.randbytes(2048*1536*3)).save(root/'9mb.png')
# Minimal but valid OOXML, generated solely as a test fixture.
with zipfile.ZipFile(root/'large.xlsx','w',zipfile.ZIP_DEFLATED,compresslevel=1) as z:
    z.writestr('[Content_Types].xml','<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>')
    z.writestr('_rels/.rels','<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>')
    z.writestr('xl/workbook.xml','<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>')
    z.writestr('xl/_rels/workbook.xml.rels','<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>')
    with z.open('xl/worksheets/sheet1.xml','w') as sheet:
        sheet.write(b'<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="A1:J50000"/><sheetData>')
        for r in range(1,50001):
            cells=''.join(f'<c r="{chr(65+c)}{r}" t="inlineStr"><is><t>{rng.randbytes(12).hex()}</t></is></c>' for c in range(10))
            sheet.write(f'<row r="{r}">{cells}</row>'.encode())
        sheet.write(b'</sheetData></worksheet>')
print('\n'.join(f'{p.name}: {p.stat().st_size:,} bytes' for p in root.iterdir()))
