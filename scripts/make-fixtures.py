"""Small deterministic two-page PDF fixture; no third-party package required."""
from pathlib import Path

objects = [
    b"<< /Type /Catalog /Pages 2 0 R >>",
    b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>",
    b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
]
for page in (1, 2):
    stream = f"0.9 0.94 1 rg 40 300 520 300 re f 0.08 0.2 0.4 rg BT /F1 28 Tf 50 700 Td (QuickPeek - Page {page}) Tj ET".encode()
    objects.append(b"<< /Length " + str(len(stream)).encode() + b" >>\nstream\n" + stream + b"\nendstream")
data = bytearray(b"%PDF-1.4\n")
offsets = [0]
for i, obj in enumerate(objects, 1):
    offsets.append(len(data))
    data += f"{i} 0 obj\n".encode() + obj + b"\nendobj\n"
xref = len(data)
data += f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode()
for offset in offsets[1:]:
    data += f"{offset:010} 00000 n \n".encode()
data += f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
Path("tests/fixtures/sample.pdf").write_bytes(data)
