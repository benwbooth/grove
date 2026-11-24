#!/usr/bin/env python3
"""grove's file type icons (icons/filetypes/*.svg) and the table that picks
them (src/filetype_table.rs). macOS-style: a document page with a folded
corner in the type's colour, a white picture of the kind of file on it,
and, from about 40 px up, the extension or a short type name ("JPEG",
"RUST", "ZIP") written across the bottom, so every extension looks
different. The label isn't in the SVG: grove draws it when it renders the
icon (cpp/native.cpp, TypeIcon), so extensions this table has never heard
of get their own label too. Below 40 px the picture is drawn larger and
centred instead (the label would be unreadable there).

Same family as scripts/gen-icons.py (gradient, soft sheen, white glyph)
and only what Qt's SVG renderer draws (paths, gradients, opacity). The
page is <g id="page">, the picture <g id="glyph">: the renderer draws
them separately to move the picture for the small layout.

    python3 scripts/gen-filetype-icons.py

Add an extension: put it in TYPES below and run the script (it rewrites
both outputs; commit them).
"""
import os

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(ROOT, "icons", "filetypes")

# --- colours ---------------------------------------------------------------
PALETTE = {
    "red": "#e5383b", "crimson": "#c81e3a", "orange": "#f07a1a", "amber": "#e39b0b",
    "yellow": "#d6a700", "lime": "#6fae1c", "green": "#2fa84f", "emerald": "#13946b",
    "teal": "#11a0a0", "cyan": "#1597c9", "blue": "#2f74e0", "navy": "#24509a",
    "indigo": "#5856d6", "purple": "#9450dc", "magenta": "#c2389e", "pink": "#e0457b",
    "brown": "#a8693a", "tan": "#c08a4e", "steel": "#6d7d91", "slate": "#53616f",
    "grey": "#8a8f98", "dark": "#3a404c", "rust": "#c4561f", "violet": "#7b4fd0",
    "sky": "#3aa0f0", "olive": "#8a8d2a", "plum": "#8e3f86", "coral": "#ef6a4c",
}


def mix(a, b, t):
    a = [int(a[i:i + 2], 16) for i in (1, 3, 5)]
    b = [int(b[i:i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(x + (y - x) * t):02x}" for x, y in zip(a, b))


# --- pictures: white, centred on 0,0 in about ±62 × ±50 -------------------
W = 'fill="#ffffff"'


def S(w=14):
    return f'fill="none" stroke="#ffffff" stroke-width="{w}" stroke-linecap="round" stroke-linejoin="round"'


def rr(x, y, w, h, r, fill=W):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" {fill}/>'


def g_lines(c, d):
    return "".join(rr(-56, y, w, 13, 6.5) for y, w in ((-44, 112), (-19, 112), (6, 112), (31, 72)))


def g_doc(c, d):
    return rr(-56, -48, 80, 20, 7) + "".join(rr(-56, y, w, 12, 6) for y, w in ((-12, 112), (10, 112), (32, 78)))


def g_layout(c, d):
    return (rr(-58, -48, 52, 44, 7) + rr(4, -46, 54, 12, 6) + rr(4, -24, 42, 12, 6)
            + rr(-58, 6, 116, 12, 6) + rr(-58, 28, 116, 12, 6))


def g_markdown(c, d):
    return (f'<rect x="-60" y="-40" width="120" height="80" rx="14" {S(10)}/>'
            f'<path d="M-40 22 V-20 L-22 2 L-4 -20 V22" {S(11)}/>'
            f'<path d="M28 -20 V18 M14 6 L28 20 L42 6" {S(11)}/>')


def g_code(c, d):
    return f'<path d="M-30 -36 L-60 0 L-30 36 M30 -36 L60 0 L30 36 M12 -46 L-12 46" {S(15)}/>'


def g_braces(c, d):
    return (f'<path d="M-26 -46 Q-44 -46 -44 -30 V-14 Q-44 0 -60 0 Q-44 0 -44 14 V30 Q-44 46 -26 46" {S(13)}/>'
            f'<path d="M26 -46 Q44 -46 44 -30 V-14 Q44 0 60 0 Q44 0 44 14 V30 Q44 46 26 46" {S(13)}/>'
            f'<circle cx="-12" cy="0" r="8" {W}/><circle cx="12" cy="0" r="8" {W}/>')


def g_script(c, d):
    return f'<path d="M-52 -34 L-16 0 L-52 34" {S(16)}/>' + rr(0, 26, 56, 15, 7.5)


def g_sliders(c, d):
    out = "".join(f'<path d="M-58 {y} H58" {S(10)}/>' for y in (-34, 0, 34))
    for x, y in ((-22, -34), (28, 0), (-6, 34)):
        out += f'<circle cx="{x}" cy="{y}" r="14" fill="{c}" stroke="#ffffff" stroke-width="9"/>'
    return out


def g_database(c, d):
    return (f'<path d="M-52 -34 V34 A52 16 0 0 0 52 34 V-34 Z" {W}/>'
            f'<path d="M-52 -8 A52 16 0 0 0 52 -8 M-52 18 A52 16 0 0 0 52 18" fill="none" stroke="{c}" stroke-width="6"/>'
            f'<ellipse cx="0" cy="-34" rx="52" ry="16" {W}/>'
            f'<ellipse cx="0" cy="-34" rx="52" ry="16" fill="none" stroke="{c}" stroke-opacity="0.35" stroke-width="4"/>')


def g_grid(c, d):
    return (rr(-58, -46, 116, 92, 10) + rr(-58, -46, 116, 24, 10, f'fill="{d}" fill-opacity="0.35"')
            + f'<path d="M-58 -22 H58 M-58 2 H58 M-58 24 H58 M-20 -46 V46 M20 -46 V46" stroke="{c}" stroke-width="6"/>')


def g_slides(c, d):
    return (rr(-60, -48, 120, 80, 9) + f'<path d="M0 32 V50 M-22 50 H22" {S(10)}/>'
            + rr(-38, -6, 18, 26, 3, f'fill="{c}"') + rr(-9, -22, 18, 42, 3, f'fill="{c}"') + rr(20, -36, 18, 56, 3, f'fill="{c}"'))


def g_image(c, d):
    return (f'<circle cx="30" cy="-26" r="15" {W}/>'
            f'<path d="M-58 44 L-22 -12 L4 22 L22 2 L58 44 Z" {W} stroke="#ffffff" stroke-width="8" stroke-linejoin="round"/>')


def g_camera(c, d):
    return (rr(-24, -46, 48, 22, 6) + rr(-60, -32, 120, 82, 16)
            + f'<circle cx="0" cy="10" r="28" fill="{c}"/><circle cx="0" cy="10" r="15" fill="#ffffff" fill-opacity="0.85"/>'
            + f'<circle cx="42" cy="-16" r="6" fill="{c}"/>')


def g_vector(c, d):
    return (f'<path d="M-50 38 C-40 -40 40 -40 50 38" {S(10)}/>'
            f'<path d="M-52 -30 H52" fill="none" stroke="#ffffff" stroke-width="5" stroke-opacity="0.8"/>'
            + rr(-62, 26, 24, 24, 4) + rr(38, 26, 24, 24, 4)
            + f'<circle cx="-52" cy="-30" r="9" {W}/><circle cx="52" cy="-30" r="9" {W}/>' + rr(-11, -41, 22, 22, 4))


def g_layers(c, d):
    out = ""
    for y, o in ((24, 0.55), (0, 0.8), (-24, 1)):
        out += f'<path d="M0 {y - 28} L60 {y} L0 {y + 28} L-60 {y} Z" fill="#ffffff" fill-opacity="{o}" stroke="{c}" stroke-width="3" stroke-linejoin="round"/>'
    return out


def g_video(c, d):
    holes = "".join(rr(x, y, 12, 9, 2, f'fill="{c}"') for x in (-50, -26, -2, 22, 46) for y in (-38, 29))
    return (rr(-62, -46, 124, 92, 12) + holes
            + f'<path d="M-14 -20 L22 0 L-14 20 Z" fill="{c}" stroke="{c}" stroke-width="10" stroke-linejoin="round"/>')


def g_audio(c, d):
    return (f'<ellipse cx="-42" cy="32" rx="19" ry="14" transform="rotate(-22 -42 32)" {W}/>'
            f'<ellipse cx="24" cy="40" rx="19" ry="14" transform="rotate(-22 24 40)" {W}/>'
            f'<path d="M-26 28 V-36 M40 36 V-46" {S(11)}/>'
            f'<path d="M-31.5 -44 L45.5 -56 V-32 L-31.5 -20 Z" {W}/>')


def g_wave(c, d):
    return f'<path d="M-60 22 H-38 V-24 H-10 V22 H18 V-24 H46 V22 H60" {S(13)}/>'


def g_playlist(c, d):
    return ("".join(rr(-58, y, w, 13, 6.5) for y, w in ((-44, 112), (-18, 80), (8, 56)))
            + f'<circle cx="30" cy="40" r="14" {W}/><path d="M42 40 V-2 L58 6" {S(9)}/>')


def g_subtitle(c, d):
    return (rr(-60, -46, 120, 74, 16) + f'<path d="M-30 24 L-40 50 L-4 26 Z" {W} stroke="#ffffff" stroke-width="6" stroke-linejoin="round"/>'
            + rr(-42, -28, 60, 12, 6, f'fill="{c}"') + rr(-42, -6, 84, 12, 6, f'fill="{c}"'))


def g_zipper(c, d):
    teeth = "".join(rr(-20 if i % 2 else 1, -54 + i * 12, 19, 9, 3) for i in range(7))
    return (rr(-4, -54, 8, 84, 4, 'fill="#ffffff" fill-opacity="0.6"') + teeth
            + rr(-18, 20, 36, 36, 10) + rr(-7, 29, 14, 17, 5, f'fill="{c}"'))


def g_box(c, d):
    return (f'<path d="M-56 -26 L0 -2 V50 L-56 26 Z" fill="#ffffff" fill-opacity="0.82"/>'
            f'<path d="M56 -26 L0 -2 V50 L56 26 Z" fill="#ffffff" fill-opacity="0.62"/>'
            f'<path d="M0 -50 L56 -26 L0 -2 L-56 -26 Z" {W}/>'
            f'<path d="M-28 -38 L28 -14 V8" fill="none" stroke="{c}" stroke-width="9" stroke-linejoin="round"/>')


def g_disc(c, d):
    return (f'<path d="M0 -52 A52 52 0 1 1 0 52 A52 52 0 1 1 0 -52 Z M0 -12 A12 12 0 1 0 0 12 A12 12 0 1 0 0 -12 Z" {W} fill-rule="evenodd"/>'
            f'<circle cx="0" cy="0" r="34" fill="none" stroke="{c}" stroke-opacity="0.3" stroke-width="4"/>'
            f'<path d="M-30 -24 A38 38 0 0 1 -8 -37" fill="none" stroke="{c}" stroke-opacity="0.45" stroke-width="6" stroke-linecap="round"/>')


def g_drive(c, d):
    return (rr(-60, -30, 120, 64, 13) + rr(-46, -2, 54, 9, 4.5, f'fill="{c}"')
            + f'<circle cx="38" cy="2" r="8" fill="{c}"/>' + rr(-60, -30, 120, 20, 10, 'fill="#ffffff"'))


def g_font(c, d):
    return (f'<path d="M-60 44 L-31 -44 L-2 44 M-48 16 H-14" {S(15)}/>'
            f'<circle cx="32" cy="20" r="21" {S(14)}/><path d="M53 -4 V44" {S(14)}/>')


def g_book(c, d):
    return (f'<path d="M-3 -34 Q-28 -50 -60 -42 V38 Q-28 30 -3 46 Z" {W}/>'
            f'<path d="M3 -34 Q28 -50 60 -42 V38 Q28 30 3 46 Z" fill="#ffffff" fill-opacity="0.85"/>'
            f'<path d="M-46 -24 Q-30 -28 -16 -20 M-46 -4 Q-30 -8 -16 0 M-46 16 Q-30 12 -16 20" fill="none" stroke="{c}" stroke-width="5" stroke-linecap="round"/>')


def g_burst(c, d):
    import math
    pts = []
    for i in range(24):
        r = 58 if i % 2 == 0 else 38
        t = math.pi * 2 * i / 24 - math.pi / 2
        pts.append(f"{r * math.cos(t) * 1.05:.1f} {r * math.sin(t) * 0.88:.1f}")
    return (f'<path d="M{" L".join(pts)} Z" {W} stroke="#ffffff" stroke-width="4" stroke-linejoin="round"/>'
            + rr(-6, -28, 12, 34, 6, f'fill="{c}"') + f'<circle cx="0" cy="20" r="7" fill="{c}"/>')


def g_gear(c, d):
    import math
    teeth, ro, ri = 9, 56, 43
    pts = []
    for i in range(teeth):
        a = 2 * math.pi * i / teeth
        for da, r in ((-0.20, ri), (-0.13, ro), (0.13, ro), (0.20, ri)):
            pts.append(f"{r * math.cos(a + da):.1f} {r * math.sin(a + da):.1f}")
        a2 = a + math.pi / teeth
        pts.append(f"{ri * math.cos(a2):.1f} {ri * math.sin(a2):.1f}")
    return (f'<path d="M{" L".join(pts)} Z M0 -17 A17 17 0 1 0 0 17 A17 17 0 1 0 0 -17 Z" {W} fill-rule="evenodd" '
            f'stroke="#ffffff" stroke-width="4" stroke-linejoin="round"/>')


def g_blocks(c, d):
    return (rr(-56, -50, 50, 46, 9) + rr(6, -50, 50, 46, 9, 'fill="#ffffff" fill-opacity="0.7"')
            + rr(-56, 4, 50, 46, 9, 'fill="#ffffff" fill-opacity="0.7"') + rr(6, 4, 50, 46, 9))


def g_cube(c, d):
    return (f'<path d="M-40 -22 L0 -44 L40 -22 L0 0 Z" fill="#ffffff" fill-opacity="0.95"/>'
            f'<path d="M-40 -22 L0 0 V48 L-40 26 Z" fill="#ffffff" fill-opacity="0.45"/>'
            f'<path d="M40 -22 L0 0 V48 L40 26 Z" fill="#ffffff" fill-opacity="0.25"/>'
            f'<path d="M-40 -22 L0 -44 L40 -22 V26 L0 48 L-40 26 Z M-40 -22 L0 0 L40 -22 M0 0 V48" {S(8)}/>')


def g_key(c, d):
    return f'<circle cx="-34" cy="0" r="22" {S(14)}/><path d="M-12 0 H58 M40 0 V22 M56 0 V16" {S(13)}/>'


def g_seal(c, d):
    return (f'<path d="M-22 10 L-34 54 L-17 46 L-6 58 L2 20 Z" fill="#ffffff" fill-opacity="0.85"/>'
            f'<path d="M22 10 L34 54 L17 46 L6 58 L-2 20 Z" fill="#ffffff" fill-opacity="0.85"/>'
            f'<circle cx="0" cy="-14" r="36" {W}/><circle cx="0" cy="-14" r="23" fill="none" stroke="{c}" stroke-width="5"/>')


def g_envelope(c, d):
    return rr(-60, -40, 120, 80, 10) + f'<path d="M-50 -30 L0 6 L50 -30" fill="none" stroke="{c}" stroke-width="8" stroke-linecap="round" stroke-linejoin="round"/>'


def g_calendar(c, d):
    dots = "".join(rr(x, y, 16, 12, 3, f'fill="{c}"') for x in (-40, -8, 24) for y in (-2, 22))
    return (rr(-58, -42, 116, 94, 12) + rr(-58, -42, 116, 28, 12, f'fill="{d}"') + rr(-58, -26, 116, 12, 0, f'fill="{d}"')
            + rr(-36, -54, 12, 24, 6) + rr(24, -54, 12, 24, 6) + dots)


def g_person(c, d):
    return f'<circle cx="0" cy="-22" r="24" {W}/><path d="M-50 50 Q-50 6 0 6 Q50 6 50 50 Z" {W}/>'


def g_globe(c, d):
    return (f'<circle cx="0" cy="0" r="52" {S(10)}/><ellipse cx="0" cy="0" rx="22" ry="52" {S(8)}/>'
            f'<path d="M-52 0 H52 M-44 -26 H44 M-44 26 H44" {S(7)}/>')


def g_magnet(c, d):
    return (f'<path d="M-34 -40 V6 A34 34 0 0 0 34 6 V-40" fill="none" stroke="#ffffff" stroke-width="28"/>'
            + rr(-48, -54, 28, 20, 3, f'fill="{d}" fill-opacity="0.55"') + rr(20, -54, 28, 20, 3, f'fill="{d}" fill-opacity="0.55"'))


def g_log(c, d):
    return "".join(f'<circle cx="-52" cy="{y + 6}" r="7" {W}/>' + rr(-36, y, w, 12, 6) for y, w in ((-46, 94), (-20, 70), (6, 94), (32, 56)))


def g_diff(c, d):
    return f'<path d="M-28 -48 V-6 M-49 -27 H-7 M8 30 H50" {S(14)}/><path d="M40 -46 L-40 46" {S(6)} stroke-opacity="0.7"/>'


def g_binary(c, d):
    out = ""
    for row, y in ((("1", "0", "1"), -24), (("0", "1", "0"), 26)):
        for ch, x in zip(row, (-40, 0, 40)):
            out += (f'<path d="M{x - 6} {y - 12} L{x + 2} {y - 18} V{y + 18}" {S(10)}/>' if ch == "1"
                    else f'<ellipse cx="{x}" cy="{y}" rx="11" ry="17" {S(9)}/>')
    return out


def g_gamepad(c, d):
    return (f'<path d="M-42 -30 H42 Q62 -30 62 -6 L60 24 Q58 46 40 46 Q28 46 20 30 H-20 Q-28 46 -40 46 Q-58 46 -60 24 L-62 -6 Q-62 -30 -42 -30 Z" {W}/>'
            + rr(-48, -5, 30, 10, 3, f'fill="{c}"') + rr(-38, -15, 10, 30, 3, f'fill="{c}"')
            + f'<circle cx="28" cy="-6" r="7" fill="{c}"/><circle cx="44" cy="6" r="7" fill="{c}"/>')


def g_floppy(c, d):
    return (f'<path d="M-50 -50 H30 L50 -30 V50 H-50 Z" {W} stroke="#ffffff" stroke-width="8" stroke-linejoin="round"/>'
            + rr(-30, -54, 50, 32, 4, f'fill="{c}"') + rr(4, -48, 9, 20, 2) + rr(-36, 6, 72, 44, 5, f'fill="{c}" fill-opacity="0.45"'))


def g_lock(c, d):
    return (f'<path d="M-26 -8 V-22 A26 26 0 0 1 26 -22 V-8" {S(13)}/>' + rr(-44, -10, 88, 62, 11)
            + f'<circle cx="0" cy="14" r="9" fill="{c}"/>' + rr(-4, 14, 8, 22, 3, f'fill="{c}"'))


def g_hash(c, d):
    return f'<path d="M-12 -48 L-24 48 M24 -48 L12 48 M-44 -16 H50 M-50 18 H44" {S(13)}/>'


def g_pin(c, d):
    return (f'<path d="M0 54 C-10 30 -42 10 -42 -12 A42 42 0 1 1 42 -12 C42 10 10 30 0 54 Z" {W}/>'
            f'<circle cx="0" cy="-12" r="16" fill="{c}"/>')


def g_window(c, d):
    return (rr(-60, -46, 120, 92, 12) + rr(-50, -22, 100, 58, 5, f'fill="{c}" fill-opacity="0.85"')
            + "".join(f'<circle cx="{x}" cy="-34" r="5" fill="{c}"/>' for x in (-46, -32, -18)))


def g_link(c, d):
    return (f'<path d="M-8 -18 L10 -36 A20 20 0 0 1 38 -8 L22 8" {S(13)}/>'
            f'<path d="M8 18 L-10 36 A20 20 0 0 1 -38 8 L-22 -8" {S(13)}/><path d="M-14 14 L14 -14" {S(13)}/>')


def g_notes(c, d):
    return (g_lines(c, d) + f'<path d="M44 -52 L60 -36 L20 4 L2 10 L8 -8 Z" fill="{d}" stroke="#ffffff" stroke-width="6" stroke-linejoin="round"/>')


def g_blank(c, d):
    return ""


GLYPHS = {k[2:]: v for k, v in globals().items() if k.startswith("g_")}

# --- the table -------------------------------------------------------------
# (extensions, picture, colour, label). label None: the first extension in
# capitals, per extension. Extensions are matched in lower case; "tar.gz"
# style keys match the end of the name.
TYPES = [
    # text and documents
    ("txt text", "lines", "steel", None),
    ("log", "log", "grey", None),
    ("md markdown mdown mkd", "markdown", "dark", "MD"),
    ("rst", "markdown", "slate", None),
    ("adoc asciidoc", "markdown", "crimson", "ADOC"),
    ("org", "notes", "emerald", None),
    ("tex ltx sty cls bib", "lines", "teal", None),
    ("nfo diz", "lines", "indigo", None),
    ("doc docx docm dot dotx", "doc", "blue", None),
    ("odt ott fodt", "doc", "sky", None),
    ("rtf", "doc", "navy", None),
    ("pages", "doc", "orange", None),
    ("wpd wps abw", "doc", "steel", None),
    ("gdoc", "doc", "blue", "GDOC"),
    ("pdf", "layout", "red", None),
    ("ps", "layout", "slate", None),
    ("xps oxps", "layout", "teal", None),
    ("djvu djv", "layout", "plum", "DJVU"),
    ("epub", "book", "emerald", None),
    ("mobi azw azw3 kfx prc", "book", "amber", None),
    ("fb2 fbz", "book", "cyan", None),
    ("lit", "book", "olive", None),
    ("cbz cbr cb7 cbt", "burst", "orange", None),
    # spreadsheets, presentations, drawings
    ("xls xlsx xlsm xlsb xlt xltx", "grid", "green", None),
    ("ods ots fods", "grid", "lime", None),
    ("numbers", "grid", "emerald", None),
    ("csv", "grid", "teal", None),
    ("tsv tab", "grid", "cyan", None),
    ("gsheet", "grid", "green", "GSHEET"),
    ("ppt pptx pptm pps ppsx pot potx", "slides", "orange", None),
    ("odp otp fodp", "slides", "coral", None),
    ("key", "key", "amber", "KEY"),
    ("gslides", "slides", "amber", "GSLIDE"),
    ("odg otg fodg gdraw", "vector", "amber", None),
    ("vsd vsdx", "vector", "navy", None),
    ("drawio dia", "vector", "orange", None),
    # images
    ("jpg jpeg jpe jfif", "image", "teal", "JPEG"),
    ("png apng", "image", "blue", None),
    ("gif", "image", "purple", None),
    ("webp", "image", "emerald", None),
    ("avif", "image", "violet", None),
    ("heic heif", "image", "coral", None),
    ("jxl", "image", "olive", None),
    ("bmp dib", "image", "slate", None),
    ("tif tiff", "image", "navy", "TIFF"),
    ("ico cur icns", "image", "amber", None),
    ("tga", "image", "brown", None),
    ("qoi pbm pgm ppm pnm pam pcx xbm xpm hdr exr dds ktx", "image", "steel", None),
    ("cr2 cr3 crw nef nrw arw srf sr2 dng raf orf rw2 pef srw 3fr erf kdc mos mrw x3f raw", "camera", "dark", None),
    ("psd psb", "layers", "navy", None),
    ("xcf", "layers", "brown", None),
    ("kra ora", "layers", "magenta", None),
    ("afphoto afdesign", "layers", "violet", None),
    ("svg svgz", "vector", "orange", "SVG"),
    ("ai", "vector", "amber", None),
    ("eps epsf", "vector", "brown", None),
    ("wmf emf", "vector", "slate", None),
    # video
    ("mp4 m4v", "video", "purple", None),
    ("mkv mk3d", "video", "indigo", "MKV"),
    ("webm", "video", "emerald", None),
    ("mov qt", "video", "sky", "MOV"),
    ("avi divx", "video", "magenta", None),
    ("wmv asf", "video", "navy", None),
    ("flv f4v", "video", "red", None),
    ("mpg mpeg m2v vob mts m2ts", "video", "plum", None),
    ("3gp 3g2 ogv rm rmvb", "video", "violet", None),
    ("srt sub sbv", "subtitle", "slate", None),
    ("vtt", "subtitle", "teal", None),
    ("ass ssa", "subtitle", "indigo", None),
    ("idx sup", "subtitle", "grey", None),
    # audio
    ("mp3", "audio", "pink", None),
    ("flac", "audio", "crimson", None),
    ("wav wave bwf", "audio", "sky", "WAV"),
    ("ogg oga", "audio", "coral", None),
    ("opus", "audio", "emerald", None),
    ("m4a m4b aac alac", "audio", "red", None),
    ("aif aiff aifc", "audio", "blue", "AIFF"),
    ("wma", "audio", "navy", None),
    ("ape wv tta dsf dff", "audio", "plum", None),
    ("mid midi kar", "audio", "amber", "MIDI"),
    ("mod xm it s3m mptm 669 mtm", "wave", "teal", None),
    ("nsf nsfe spc gbs vgm vgz sid psid hes ay usf miniusf gsf minigsf 2sf psf minipsf", "wave", "purple", None),
    ("m3u m3u8 pls xspf cue asx wpl", "playlist", "magenta", None),
    # archives and packages
    ("zip zipx", "zipper", "brown", "ZIP"),
    ("7z", "zipper", "slate", None),
    ("rar", "zipper", "purple", None),
    ("tar", "zipper", "tan", None),
    ("tar.gz tgz", "zipper", "amber", "TGZ"),
    ("tar.xz txz", "zipper", "olive", "TXZ"),
    ("tar.bz2 tbz tbz2", "zipper", "rust", "TBZ"),
    ("tar.zst tzst", "zipper", "teal", "TZST"),
    ("gz gzip", "zipper", "amber", "GZ"),
    ("xz lzma lz lz4 lzo", "zipper", "olive", None),
    ("bz2 bz", "zipper", "rust", None),
    ("zst zstd", "zipper", "teal", "ZST"),
    ("z cpio ar arj lha lzh ace cab sit sitx", "zipper", "grey", None),
    ("jar war ear", "zipper", "red", None),
    ("deb udeb", "box", "crimson", None),
    ("rpm srpm", "box", "red", None),
    ("apk xapk aab", "box", "lime", None),
    ("appimage", "box", "navy", "APPIMG"),
    ("flatpak flatpakref flatpakrepo", "box", "sky", "FLATPAK"),
    ("snap", "box", "coral", None),
    ("pkg mpkg", "box", "tan", None),
    ("whl egg", "box", "blue", None),
    ("crate", "box", "rust", None),
    ("gem", "box", "red", None),
    ("nupkg vsix xpi crx", "box", "violet", None),
    ("msi msix appx", "box", "blue", None),
    # disk images
    ("iso", "disc", "slate", None),
    ("dmg", "drive", "grey", None),
    ("img ima", "drive", "steel", None),
    ("qcow qcow2", "drive", "rust", None),
    ("vdi vmdk vhd vhdx vpc", "drive", "indigo", None),
    ("toast nrg mdf mds ccd cdi", "disc", "dark", None),
    ("chd rvz wbfs gcm wia cso", "disc", "purple", None),
    ("squashfs sfs erofs", "drive", "dark", None),
    # games and emulators
    ("nes fds unf", "gamepad", "red", None),
    ("sfc smc fig swc", "gamepad", "indigo", None),
    ("gb gbc", "gamepad", "olive", None),
    ("gba", "gamepad", "violet", None),
    ("nds dsi 3ds cia cci cxi", "gamepad", "slate", None),
    ("n64 z64 v64", "gamepad", "green", None),
    ("smd gen 32x sms gg", "gamepad", "dark", None),
    ("pce sgx ngp ngc ws wsc lnx a26 a78", "gamepad", "brown", None),
    ("nsp xci nsz xcz nro nca", "gamepad", "crimson", None),
    ("pbp", "gamepad", "navy", None),
    ("sav srm sram eep fla mpk state ss0 ss1 ss2 ss3", "floppy", "steel", None),
    ("ips bps ups xdelta vcdiff ppf aps", "diff", "teal", None),
    # fonts
    ("ttf", "font", "dark", None),
    ("otf", "font", "slate", None),
    ("woff woff2", "font", "teal", None),
    ("ttc otc", "font", "navy", None),
    ("pfa pfb afm bdf pcf fnt fon", "font", "grey", None),
    # programming
    ("rs", "code", "rust", "RUST"),
    ("py pyw pyi", "code", "blue", "PY"),
    ("pyc pyo", "binary", "blue", None),
    ("ipynb", "code", "orange", "IPYNB"),
    ("js mjs cjs", "code", "yellow", "JS"),
    ("jsx", "code", "amber", None),
    ("ts cts", "code", "navy", "TS"),
    ("tsx", "code", "sky", None),
    ("c", "code", "slate", "C"),
    ("h", "code", "steel", "H"),
    ("cpp cc cxx c++ cp", "code", "magenta", "C++"),
    ("hpp hh hxx h++ inl ipp", "code", "plum", None),
    ("m mm", "code", "sky", None),
    ("cs csx", "code", "violet", "C#"),
    ("fs fsx fsi", "code", "teal", "F#"),
    ("vb vbs bas", "code", "indigo", None),
    ("go", "code", "cyan", "GO"),
    ("java", "code", "brown", "JAVA"),
    ("class", "binary", "brown", None),
    ("kt kts", "code", "violet", "KOTLIN"),
    ("scala sc sbt", "code", "crimson", "SCALA"),
    ("groovy gvy gradle", "code", "teal", None),
    ("clj cljs cljc edn", "code", "lime", None),
    ("swift", "code", "coral", "SWIFT"),
    ("dart", "code", "sky", "DART"),
    ("rb rake gemspec", "code", "red", "RUBY"),
    ("php phtml", "code", "indigo", "PHP"),
    ("pl pm t pod", "code", "navy", "PERL"),
    ("lua", "code", "navy", "LUA"),
    ("zig zon", "code", "amber", "ZIG"),
    ("nim nims", "code", "yellow", "NIM"),
    ("hs lhs", "code", "plum", "HASKELL"),
    ("ml mli", "code", "orange", "OCAML"),
    ("ex exs heex", "code", "violet", "ELIXIR"),
    ("erl hrl", "code", "crimson", "ERLANG"),
    ("el elc", "code", "purple", "ELISP"),
    ("lisp lsp cl", "code", "dark", "LISP"),
    ("scm ss rkt", "code", "crimson", None),
    ("jl", "code", "purple", "JULIA"),
    ("r rmd", "code", "blue", "R"),
    ("f f90 f95 f03 for", "code", "violet", "FORTRAN"),
    ("pas pp dpr lpr", "code", "amber", "PASCAL"),
    ("d di", "code", "red", "D"),
    ("v sv svh", "code", "emerald", None),
    ("vhdl", "code", "olive", "VHDL"),
    ("asm s nasm", "code", "dark", "ASM"),
    ("wat", "code", "violet", "WAT"),
    ("wasm", "binary", "violet", "WASM"),
    ("glsl vert frag geom comp tesc tese", "code", "teal", None),
    ("hlsl wgsl metal spv", "code", "olive", None),
    ("qml", "code", "green", "QML"),
    ("nix", "code", "indigo", "NIX"),
    ("html htm xhtml shtml", "code", "orange", "HTML"),
    ("css", "code", "blue", "CSS"),
    ("scss sass", "code", "pink", None),
    ("less styl", "code", "navy", None),
    ("vue", "code", "emerald", "VUE"),
    ("svelte", "code", "coral", None),
    ("astro", "code", "purple", None),
    ("elm", "code", "sky", None),
    ("purs", "code", "slate", None),
    ("sql psql mysql", "database", "amber", "SQL"),
    ("sqlite sqlite3 db db3 s3db", "database", "navy", None),
    ("mdb accdb", "database", "crimson", None),
    ("parquet arrow feather avro orc", "database", "teal", None),
    ("rdb dump", "database", "slate", None),
    ("graphql gql", "code", "magenta", "GRAPHQL"),
    ("proto", "code", "steel", "PROTO"),
    ("thrift capnp fbs", "code", "slate", None),
    ("cmake", "code", "emerald", "CMAKE"),
    ("mk mak make", "code", "slate", "MAKE"),
    ("ninja", "code", "dark", None),
    ("bazel bzl", "code", "green", None),
    ("meson", "code", "violet", None),
    ("tf tfvars hcl", "code", "violet", None),
    ("diff", "diff", "emerald", None),
    ("patch", "diff", "green", None),
    ("rej orig", "diff", "grey", None),
    # scripts
    ("sh", "script", "dark", None),
    ("bash", "script", "slate", None),
    ("zsh", "script", "navy", None),
    ("fish", "script", "teal", None),
    ("nu", "script", "emerald", None),
    ("ksh csh tcsh", "script", "steel", None),
    ("ps1 psm1 psd1", "script", "blue", None),
    ("bat cmd", "script", "grey", None),
    ("tsh tessh", "script", "indigo", "TESSH"),
    ("awk sed", "script", "olive", None),
    ("tcl exp", "script", "sky", None),
    ("vim vimrc", "code", "emerald", "VIM"),
    ("applescript scpt", "script", "slate", None),
    ("ahk", "script", "green", None),
    # data and config
    ("json jsonc json5", "braces", "amber", None),
    ("jsonl ndjson", "braces", "brown", None),
    ("xml xsd xsl xslt dtd rss atom", "code", "teal", "XML"),
    ("plist", "braces", "slate", None),
    ("yaml yml", "sliders", "crimson", "YAML"),
    ("toml", "sliders", "brown", None),
    ("ini cfg conf config cnf", "sliders", "slate", None),
    ("env", "sliders", "olive", None),
    ("properties props prefs", "sliders", "steel", None),
    ("reg", "sliders", "blue", None),
    ("desktop", "window", "slate", "DESKTOP"),
    ("service socket timer mount target path", "sliders", "indigo", None),
    ("rules", "sliders", "teal", None),
    ("lock", "lock", "steel", None),
    ("gitignore gitattributes gitmodules gitconfig", "sliders", "coral", "GIT"),
    ("editorconfig", "sliders", "dark", None),
    ("dockerignore", "sliders", "sky", None),
    ("npmrc nvmrc yarnrc", "sliders", "red", None),
    ("bak backup old tmp temp swp", "floppy", "grey", None),
    # security
    ("pem crt cer der p7b p7c", "seal", "emerald", None),
    ("csr", "seal", "teal", None),
    ("p12 pfx jks keystore", "key", "teal", None),
    ("pub", "key", "sky", None),
    ("asc gpg pgp sig", "key", "indigo", None),
    ("kdbx kdb", "key", "green", None),
    ("ppk", "key", "slate", None),
    ("sha1 sha256 sha512 md5 sfv crc b3 sum", "hash", "slate", None),
    # executables and libraries
    ("exe com scr", "window", "blue", None),
    ("app", "window", "slate", None),
    ("bin", "binary", "slate", None),
    ("run elf out", "gear", "dark", None),
    ("so dylib", "blocks", "slate", None),
    ("dll", "blocks", "blue", None),
    ("a lib", "blocks", "steel", None),
    ("o ko", "blocks", "grey", None),
    ("rlib rmeta", "blocks", "rust", None),
    ("pdb dwo dSYM", "blocks", "dark", None),
    ("dat data", "binary", "grey", None),
    ("hex srec ihex", "binary", "teal", None),
    ("rom", "binary", "red", None),
    ("efi", "gear", "navy", None),
    # 3D and CAD
    ("obj stl", "cube", "teal", None),
    ("fbx", "cube", "slate", None),
    ("gltf glb", "cube", "emerald", None),
    ("blend blend1", "cube", "orange", "BLEND"),
    ("3mf amf", "cube", "cyan", None),
    ("ply off", "cube", "steel", None),
    ("dae", "cube", "red", None),
    ("usd usda usdc usdz", "cube", "indigo", None),
    ("step stp iges igs", "cube", "navy", None),
    ("scad", "cube", "amber", None),
    ("fcstd", "cube", "red", None),
    ("dwg dxf", "vector", "red", None),
    ("gcode gco nc", "cube", "olive", None),
    ("max ma mb c4d", "cube", "purple", None),
    # mail, contacts, calendars, web, maps, torrents
    ("eml msg emlx", "envelope", "sky", None),
    ("mbox mbx", "envelope", "navy", None),
    ("ics ical ifb vcs", "calendar", "red", None),
    ("vcf vcard", "person", "brown", None),
    ("url webloc website", "globe", "blue", None),
    ("lnk", "link", "steel", None),
    ("mht mhtml warc webarchive", "globe", "teal", None),
    ("har", "globe", "slate", None),
    ("gpx kml kmz", "pin", "green", None),
    ("geojson topojson shp", "pin", "teal", None),
    ("torrent", "magnet", "emerald", None),
    ("magnet", "magnet", "teal", None),
    # misc
    ("ldif", "person", "slate", None),
    ("gform", "doc", "purple", "GFORM"),
    ("gmap", "pin", "red", "GMAP"),
    ("gsite", "globe", "indigo", "GSITE"),
    ("gtable", "grid", "blue", "GTABLE"),
    ("gscript", "code", "blue", "GSCRIPT"),
    ("gjam", "vector", "amber", "GJAM"),
    ("gvid", "video", "purple", "GVID"),
]

# Whole names (lower case): these win over the extension.
NAMES = [
    ("makefile gnumakefile bsdmakefile", "code", "slate", "MAKE"),
    ("cmakelists.txt", "code", "emerald", "CMAKE"),
    ("dockerfile containerfile", "code", "sky", "DOCKER"),
    ("justfile", "code", "amber", "JUST"),
    ("meson.build meson_options.txt", "code", "violet", "MESON"),
    ("build.bazel workspace", "code", "green", "BAZEL"),
    ("cargo.toml", "sliders", "rust", "CARGO"),
    ("cargo.lock flake.lock package-lock.json yarn.lock pnpm-lock.yaml poetry.lock gemfile.lock composer.lock", "lock", "steel", "LOCK"),
    ("flake.nix", "code", "indigo", "FLAKE"),
    ("readme", "lines", "blue", "README"),
    ("license licence copying unlicense", "seal", "steel", "LICENSE"),
    ("authors contributors changelog news todo thanks", "lines", "steel", None),
    ("gemfile rakefile podfile", "code", "red", "RUBY"),
    ("vagrantfile", "code", "sky", "RUBY"),
    ("pkgbuild apkbuild", "script", "sky", "PKGBUILD"),
    (".bashrc .bash_profile .profile .zshrc .zprofile .bash_logout .xinitrc .xprofile", "script", "dark", "SH"),
    (".env", "sliders", "olive", "ENV"),
    ("crontab fstab hosts passwd group sudoers inittab", "sliders", "slate", None),
    ("id_rsa id_ed25519 id_ecdsa id_dsa", "key", "slate", "KEY"),
    ("authorized_keys known_hosts", "key", "slate", "SSH"),
    ("core", "binary", "red", "CORE"),
]

# A type the system knows by its content, for files the tables don't name.
MIMES = [
    ("application/x-executable", "gear", "dark", "EXEC"),
    ("application/x-pie-executable", "gear", "dark", "EXEC"),
    ("application/x-sharedlib", "blocks", "slate", "LIB"),
    ("application/x-object", "blocks", "grey", "OBJ"),
    ("application/x-core", "binary", "red", "CORE"),
    ("application/x-shellscript", "script", "dark", "SH"),
    ("text/x-shellscript", "script", "dark", "SH"),
    ("text/x-python", "code", "blue", "PY"),
    ("text/x-python3", "code", "blue", "PY"),
    ("application/x-perl", "code", "navy", "PERL"),
    ("application/x-ruby", "code", "red", "RUBY"),
    ("text/x-makefile", "code", "slate", "MAKE"),
    ("application/x-zerosize", "blank", "grey", "EMPTY"),
]

# The freedesktop generic icon a type names, then the type's family
# (before the "/"): everything else is a grey page with the extension.
GENERIC = [
    ("image-x-generic", "image", "teal"), ("video-x-generic", "video", "purple"),
    ("audio-x-generic", "audio", "pink"), ("text-x-generic", "lines", "steel"),
    ("text-x-script", "script", "dark"), ("package-x-generic", "zipper", "brown"),
    ("x-office-document", "doc", "blue"), ("x-office-spreadsheet", "grid", "green"),
    ("x-office-presentation", "slides", "orange"), ("x-office-drawing", "vector", "orange"),
    ("x-office-address-book", "person", "brown"), ("x-office-calendar", "calendar", "red"),
    ("font-x-generic", "font", "dark"), ("application-x-executable", "gear", "dark"),
    ("text-html", "code", "orange"), ("model", "cube", "teal"),
    ("image", "image", "teal"), ("video", "video", "purple"), ("audio", "audio", "pink"),
    ("text", "lines", "steel"), ("font", "font", "dark"),
    ("unknown", "blank", "grey"),
]

# --- the page ----------------------------------------------------------------
L, R, T, B, RAD, FOLD = 40, 216, 14, 242, 16, 54
PAGE = (f"M{L + RAD} {T} H{R - FOLD} L{R} {T + FOLD} V{B - RAD} Q{R} {B} {R - RAD} {B} "
        f"H{L + RAD} Q{L} {B} {L} {B - RAD} V{T + RAD} Q{L} {T} {L + RAD} {T} Z")
FOLD_D = f"M{R - FOLD} {T} V{T + FOLD - 12} Q{R - FOLD} {T + FOLD} {R - FOLD + 12} {T + FOLD} H{R} Z"
# Where the picture sits with a label under it (the renderer's small layout
# centres it on the page instead).
GLYPH_AT = (128, 104)


def svg(glyph, colour):
    c = PALETTE[colour]
    top, bottom, d = mix(c, "#ffffff", 0.16), mix(c, "#000000", 0.10), mix(c, "#000000", 0.30)
    fold = mix(c, "#ffffff", 0.55)
    body = GLYPHS[glyph](c, d)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
  <!-- generated by scripts/gen-filetype-icons.py -->
  <defs>
    <linearGradient id="pg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{top}"/>
      <stop offset="1" stop-color="{bottom}"/>
    </linearGradient>
    <linearGradient id="sh" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.22"/>
      <stop offset="0.5" stop-color="#ffffff" stop-opacity="0"/>
    </linearGradient>
  </defs>
  <g id="page">
    <path d="{PAGE}" fill="url(#pg)"/>
    <path d="{PAGE}" fill="url(#sh)"/>
    <path d="{PAGE}" fill="none" stroke="{d}" stroke-opacity="0.45" stroke-width="2.5"/>
    <path d="M{R - FOLD} {T} L{R} {T + FOLD}" stroke="{d}" stroke-opacity="0.25" stroke-width="10"/>
    <path d="{FOLD_D}" fill="{fold}" stroke="{d}" stroke-opacity="0.45" stroke-width="2.5" stroke-linejoin="round"/>
  </g>
  <g id="glyph" transform="translate({GLYPH_AT[0]} {GLYPH_AT[1]})">{body}</g>
</svg>
"""


def main():
    os.makedirs(OUT, exist_ok=True)
    for f in os.listdir(OUT):
        if f.endswith(".svg"):
            os.remove(os.path.join(OUT, f))
    pics = {}

    def pic(glyph, colour):
        name = f"{glyph}-{colour}"
        if name not in pics:
            pics[name] = svg(glyph, colour)
        return name

    def rows(table):
        out, seen = [], set()
        for keys, glyph, colour, label in table:
            for k in keys.split():
                k = k.lower()
                assert k not in seen, f"{k} twice"
                seen.add(k)
                out.append((k, pic(glyph, colour), label if label is not None else k.upper()))
        return out

    exts, names, mimes = rows(TYPES), rows(NAMES), rows(MIMES)
    generic = [(g, pic(glyph, colour)) for g, glyph, colour in GENERIC]
    for name, text in pics.items():
        with open(os.path.join(OUT, name + ".svg"), "w") as f:
            f.write(text)

    def lst(rs):
        return "".join(f"    ({', '.join(rust_str(x) for x in r)}),\n" for r in rs)

    rust = f"""// generated by scripts/gen-filetype-icons.py: edit the script, not this.
//! grove's file type pictures (icons/filetypes) and the table choosing one
//! per extension, name, MIME type or freedesktop generic icon, with the
//! label grove writes on it.

/// (picture, SVG).
pub static PICTURES: &[(&str, &[u8])] = &[
{"".join(f'    ("{n}", include_bytes!("../icons/filetypes/{n}.svg")),{chr(10)}' for n in sorted(pics))}];

/// (extension, picture, label): "tar.gz"-style keys match a name's end.
pub static EXTS: &[(&str, &str, &str)] = &[
{lst(exts)}];

/// (whole name in lower case, picture, label).
pub static NAMES: &[(&str, &str, &str)] = &[
{lst(names)}];

/// (MIME type, picture, label).
pub static MIMES: &[(&str, &str, &str)] = &[
{lst(mimes)}];

/// (generic icon name or MIME family, picture).
pub static GENERIC: &[(&str, &str)] = &[
{lst(generic)}];
"""
    with open(os.path.join(ROOT, "src", "filetype_table.rs"), "w") as f:
        f.write(rust)
    print(f"{len(pics)} pictures, {len(exts)} extensions, {len(names)} names")


def rust_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


main()
