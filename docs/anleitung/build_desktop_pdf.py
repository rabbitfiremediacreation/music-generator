"""Erzeugt die Anleitung für die Desktop-App als PDF (de, en, zh).

Aufruf: build_desktop_pdf.py <ausgabe.pdf> [sprache]   (Texte in texts_desktop.py)
Nutzt Schriften, Farben und Bausteine (header, h2, para, code, note, table, footer, logo, badge) aus build_pdf.py.
Screenshots folgen.
"""
import sys
from pathlib import Path

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE))
_src = (HERE / "build_pdf.py").read_text()
_head = _src.split("\ndef page_cover")[0].replace("from texts import TEXTS", "from texts_desktop import TEXTS")
exec(compile(_head, "build_pdf_head", "exec"))   # definiert Schriften, Farben, T() und alle Bausteine


def page_cover_d(c):
    c.setFillColor(BG)
    c.rect(0, 0, W, H, stroke=0, fill=1)
    for i in range(40):
        c.setFillColor(BLUE, alpha=0.006)
        c.circle(40, H - 40, 380 - i * 9, stroke=0, fill=1)
        c.setFillColor(GOLD, alpha=0.006 * 0.7)
        c.circle(W - 40, H - 40, 340 - i * 8, stroke=0, fill=1)
    c.setFillAlpha(1)
    y = H - 150
    ratio = 908 / 149
    wh = (W - 2 * M - 14) / (1 + ratio)
    logo(c, M, y, wh)
    c.drawImage(str(HERE / "alphatester.png"), M + wh + 14, y - wh, height=wh, width=wh * ratio, mask="auto")
    c.setFillColor(HexColor("#8a90a0"))
    c.setFont("Arial-Bold", 50)
    c.drawString(M, y - 170, "MUSIC")
    c.setFont("Arial-Bold", 30)
    c.drawString(M, y - 206, "GENERATOR")
    c.setStrokeColor(GOLD)
    c.setLineWidth(2)
    c.line(M, y - 228, M + 60, y - 228)
    c.setFillColor(white)
    c.setFont("Arial-Bold", 22)
    c.drawString(M, y - 266, T("d1_01"))
    sub = ParagraphStyle("sub", parent=body, fontSize=12, leading=17, textColor=HexColor("#b8bcc8"))
    para(c, T("d1_02"), M, y - 282, W - 2 * M, sub)
    sy = 250
    bw = (W - 2 * M - 24) / 3
    for i, (t, s) in enumerate([(T("d1_03"), T("d1_04")), (T("d1_05"), T("d1_06")), (T("d1_07"), T("d1_08"))]):
        x = M + i * (bw + 12)
        c.setFillColor(PANEL)
        c.setStrokeColor(BORDER)
        c.roundRect(x, sy - 92, bw, 92, 10, stroke=1, fill=1)
        badge(c, i + 1, x + 24, sy - 26, 12)
        c.setFillColor(white)
        c.setFont("Arial-Bold", 12.5)
        c.drawString(x + 16, sy - 58, t)
        c.setFillColor(HexColor("#8a90a0"))
        c.setFont("Arial", 9)
        c.drawString(x + 16, sy - 75, s)
    c.setFillColor(HexColor("#6b7080"))
    c.setFont("Arial", 8.5)
    c.drawString(M, 52, T("d1_09"))


def page_overview_d(c):
    y = header(c, T("d2_01"), T("d2_02"))
    y = para(c, T("d2_03"), M, y, W - 2 * M)
    y -= 18
    y = h2(c, T("d2_04"), y)
    rows = [[T("d2_05"), T("d2_06"), T("d2_07")],
            [T("d2_08"), T("d2_09"), T("d2_10")],
            [T("d2_11"), T("d2_12"), T("d2_13")],
            [T("d2_14"), T("d2_15"), T("d2_16")],
            [T("d2_17"), T("d2_18"), T("d2_19")]]
    y = table(c, rows, y, [95, 165, W - 2 * M - 260])
    y -= 10
    y = para(c, T("d2_20"), M, y, W - 2 * M, small)
    y -= 18
    y = h2(c, T("d2_21"), y)
    rows = [[T("d2_22"), T("d2_23")], [T("d2_24"), T("d2_25")]]
    y = table(c, rows, y, [95, W - 2 * M - 95], head=False)
    y -= 16
    note(c, T("d2_26"), y, BLUE, "Web-App")
    footer(c, 2)


def page_mac_d(c):
    y = header(c, T("d3_01"), T("d3_02"))
    y = h2(c, T("d3_03"), y)
    y = para(c, T("d3_04"), M, y, W - 2 * M)
    y -= 16
    y = h2(c, T("d3_05"), y)
    y = para(c, T("d3_06"), M, y, W - 2 * M)
    y -= 10
    y = note(c, T("d3_07"), y, BLUE, "Mac")
    y -= 16
    y = h2(c, T("d3_08"), y)
    para(c, T("d3_09"), M, y, W - 2 * M)
    footer(c, 3)


def page_win_linux_d(c):
    y = header(c, T("d4_01"), T("d4_02"))
    y = h2(c, T("d4_03"), y)
    y = para(c, T("d4_04"), M, y, W - 2 * M)
    y -= 10
    y = note(c, T("d4_05"), y, BLUE, "Windows")
    y -= 10
    y = para(c, T("d4_06"), M, y, W - 2 * M, small)
    y -= 16
    y = h2(c, T("d4_07"), y)
    y = para(c, T("d4_08"), M, y, W - 2 * M)
    y -= 10
    y = code(c, T("d4_09"), M, y, W - 2 * M)
    y -= 10
    para(c, T("d4_10"), M, y, W - 2 * M)
    footer(c, 4)


def page_start_d(c):
    y = header(c, T("d5_01"), T("d5_02"))
    y = h2(c, T("d5_03"), y)
    y = para(c, T("d5_04"), M, y, W - 2 * M)
    y -= 8
    y = para(c, T("d5_05"), M, y, W - 2 * M)
    y -= 8
    y = para(c, T("d5_06"), M, y, W - 2 * M)
    y -= 14
    y = h2(c, T("d5_07"), y)
    y = para(c, T("d5_08"), M, y, W - 2 * M)
    y -= 12
    y = h2(c, T("d5_09"), y)
    rows = [[T("d5_10"), T("d5_11")],
            [T("d5_12"), T("d5_13")],
            [T("d5_14"), T("d5_15")],
            [T("d5_16"), T("d5_17")],
            [T("d5_18"), T("d5_19")]]
    y = table(c, rows, y, [200, W - 2 * M - 200])
    y -= 8
    para(c, T("d5_20"), M, y, W - 2 * M, small)
    footer(c, 5)


def page_remote_d(c):
    y = header(c, T("d7_01"), T("d7_02"))
    y = h2(c, T("d7_03"), y)
    y = para(c, T("d7_04"), M, y, W - 2 * M)
    y -= 10
    y = note(c, T("d7_06"), y, BLUE, T("d7_05"))
    y -= 8
    y = para(c, T("d7_07"), M, y, W - 2 * M, small)
    y -= 16
    y = h2(c, T("d7_08"), y)
    y = para(c, T("d7_09"), M, y, W - 2 * M)
    y -= 10
    y = code(c, T("d7_10"), M, y, W - 2 * M)
    y -= 10
    note(c, T("d7_11"), y, GOLD, "MCP")
    footer(c, 6)


def page_daily_d(c):
    y = header(c, T("d6_01"), T("d6_02"))
    y = h2(c, T("d6_03"), y)
    rows = [[T("d6_04"), T("d6_05")], [T("d6_06"), T("d6_07")], [T("d6_08"), T("d6_09")],
            [T("d6_10"), T("d6_11")], [T("d6_12"), T("d6_13")]]
    y = table(c, rows, y, [150, W - 2 * M - 150], head=False)
    y -= 6
    y = para(c, T("d6_14"), M, y, W - 2 * M, small)
    y -= 12
    y = h2(c, T("d6_15"), y)
    rows = [[T("d6_16"), T("d6_17")], [T("d6_18"), T("d6_19")], [T("d6_20"), T("d6_21")]]
    y = table(c, rows, y, [80, W - 2 * M - 80], head=False)
    y -= 6
    y = para(c, T("d6_22"), M, y, W - 2 * M, small)
    y -= 12
    y = h2(c, T("d6_23"), y)
    rows = [[T("d6_24"), T("d6_25")],
            [T("d6_26"), T("d6_27")], [T("d6_28"), T("d6_29")], [T("d6_30"), T("d6_31")],
            [T("d6_32"), T("d6_33")], [T("d6_34"), T("d6_35")]]
    y = table(c, rows, y, [190, W - 2 * M - 190])
    y -= 8
    para(c, T("d6_36"), M, y, W - 2 * M, small)
    footer(c, 7)


c = canvas.Canvas(str(OUT), pagesize=A4)
c.setTitle(T("title"))
c.setAuthor("Alphatester")
for fn in (page_cover_d, page_overview_d, page_mac_d, page_win_linux_d, page_start_d, page_remote_d, page_daily_d):
    fn(c)
    c.showPage()
c.save()
print("ok", OUT)
