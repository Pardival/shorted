#!/usr/bin/env python3
"""Generates the Shorted library: project-made SVG stickers, badges, arrows and the rest.

Writes engine/crates/shorted_render/library/<category>/<name>.svg, the MANIFEST.md beside them,
and src/library_index.rs, which embeds them in the engine. Everything is drawn here, so the
whole set is the project's own work under the project's licence: nothing is downloaded.

Run from the repository root: python3 scripts/generate_library.py
"""

import math
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATE = ROOT / "engine" / "crates" / "shorted_render"
OUT = CRATE / "library"

LIME = "#C6F135"
VIOLET = "#9A8CFF"
WHITE = "#FFFFFF"
INK = "#111114"
RED = "#FF5A5F"
YELLOW = "#FFD23F"
BLUE = "#3DA5FF"
PINK = "#FF7AC6"
ORANGE = "#FF9F1C"
COLOURS = {
    "lime": LIME,
    "violet": VIOLET,
    "white": WHITE,
    "ink": INK,
    "red": RED,
    "yellow": YELLOW,
    "blue": BLUE,
    "pink": PINK,
    "orange": ORANGE,
}


def svg(width, height, body):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}">{body}</svg>\n'
    )


def points(values):
    return " ".join(f"{x:.1f},{y:.1f}" for x, y in values)


def polygon(cx, cy, radius, sides, turn=-90.0):
    return [
        (
            cx + radius * math.cos(math.radians(turn + 360 * i / sides)),
            cy + radius * math.sin(math.radians(turn + 360 * i / sides)),
        )
        for i in range(sides)
    ]


def star(cx, cy, outer, inner, spikes, turn=-90.0):
    result = []
    for i in range(spikes * 2):
        radius = outer if i % 2 == 0 else inner
        angle = math.radians(turn + 180 * i / spikes)
        result.append((cx + radius * math.cos(angle), cy + radius * math.sin(angle)))
    return result


def heart_path(cx, cy, size):
    s = size / 2
    return (
        f"M{cx},{cy + s * 0.9} "
        f"C{cx - s * 1.4},{cy + s * 0.1} {cx - s * 0.9},{cy - s * 1.0} {cx},{cy - s * 0.35} "
        f"C{cx + s * 0.9},{cy - s * 1.0} {cx + s * 1.4},{cy + s * 0.1} {cx},{cy + s * 0.9}Z"
    )


def stroke(colour, width):
    return (
        f'fill="none" stroke="{colour}" stroke-width="{width}" '
        'stroke-linecap="round" stroke-linejoin="round"'
    )


ITEMS = []


def add(category, name, tags, content):
    ITEMS.append((category, f"{category}-{name}", tags, content))


# ── Shapes ──────────────────────────────────────────────────────────────────
add("shapes", "square", "square block", svg(300, 300, f'<rect x="10" y="10" width="280" height="280" fill="{LIME}"/>'))
add("shapes", "rounded-square", "square rounded", svg(300, 300, f'<rect x="10" y="10" width="280" height="280" rx="60" fill="{VIOLET}"/>'))
add("shapes", "circle", "circle round dot", svg(300, 300, f'<circle cx="150" cy="150" r="140" fill="{YELLOW}"/>'))
add("shapes", "triangle", "triangle", svg(300, 300, f'<polygon points="{points(polygon(150, 170, 150, 3))}" fill="{RED}"/>'))
add("shapes", "diamond", "diamond rhombus", svg(300, 300, f'<polygon points="150,10 290,150 150,290 10,150" fill="{BLUE}"/>'))
add("shapes", "pentagon", "pentagon", svg(300, 300, f'<polygon points="{points(polygon(150, 158, 145, 5))}" fill="{PINK}"/>'))
add("shapes", "hexagon", "hexagon", svg(300, 300, f'<polygon points="{points(polygon(150, 150, 145, 6, 0))}" fill="{ORANGE}"/>'))
add("shapes", "octagon", "octagon stop", svg(300, 300, f'<polygon points="{points(polygon(150, 150, 145, 8, 22.5))}" fill="{RED}"/>'))
add("shapes", "star", "star", svg(300, 300, f'<polygon points="{points(star(150, 160, 145, 60, 5))}" fill="{YELLOW}"/>'))
add("shapes", "star-six", "star six", svg(300, 300, f'<polygon points="{points(star(150, 150, 145, 80, 6))}" fill="{LIME}"/>'))
add("shapes", "ring", "ring circle outline", svg(300, 300, f'<circle cx="150" cy="150" r="120" {stroke(VIOLET, 36)}/>'))
add("shapes", "heart", "heart love", svg(300, 300, f'<path d="{heart_path(150, 150, 260)}" fill="{RED}"/>'))
add("shapes", "plus", "plus cross add", svg(300, 300, f'<path d="M110,20h80v90h90v80h-90v90h-80v-90h-90v-80h90z" fill="{WHITE}"/>'))
add("shapes", "half-circle", "half circle semicircle", svg(300, 160, f'<path d="M10,150 A140,140 0 0 1 290,150 Z" fill="{BLUE}"/>'))
add("shapes", "capsule", "capsule pill", svg(400, 160, f'<rect x="10" y="10" width="380" height="140" rx="70" fill="{PINK}"/>'))

# ── Arrows ──────────────────────────────────────────────────────────────────
add("arrows", "right", "arrow right point", svg(400, 200, f'<path d="M10,70h230v-60l150,90l-150,90v-60h-230z" fill="{LIME}"/>'))
add("arrows", "left", "arrow left point", svg(400, 200, f'<path d="M390,70h-230v-60l-150,90l150,90v-60h230z" fill="{VIOLET}"/>'))
add("arrows", "down", "arrow down point", svg(200, 400, f'<path d="M70,10v230h-60l90,150l90,-150h-60v-230z" fill="{YELLOW}"/>'))
add("arrows", "up", "arrow up point", svg(200, 400, f'<path d="M70,390v-230h-60l90,-150l90,150h-60v230z" fill="{RED}"/>'))
add("arrows", "double", "arrow double both", svg(400, 200, f'<path d="M10,100l110,-90v55h160v-55l110,90l-110,90v-55h-160v55z" fill="{BLUE}"/>'))
add("arrows", "thin", "arrow thin line", svg(400, 120, f'<path d="M20,60H370M300,15l70,45l-70,45" {stroke(WHITE, 16)}/>'))
add("arrows", "curved", "arrow curved bend", svg(360, 300, f'<path d="M40,260C40,120 140,60 280,70" {stroke(LIME, 24)}/><path d="M250,20l80,52l-90,40z" fill="{LIME}"/>'))
add("arrows", "u-turn", "arrow u turn back", svg(320, 340, f'<path d="M80,320V140A80,80 0 0 1 240,140V230" {stroke(ORANGE, 28)}/><path d="M190,210l50,80l50,-80z" fill="{ORANGE}"/>'))
add("arrows", "zigzag", "arrow zigzag", svg(400, 240, f'<path d="M20,200L120,60L200,180L300,60" {stroke(PINK, 22)}/><path d="M270,20l90,10l-30,85z" fill="{PINK}"/>'))
add("arrows", "chevron", "chevron arrow right", svg(240, 300, f'<path d="M40,30l110,120l-110,120" {stroke(LIME, 50)}/>'))
add("arrows", "triple-chevron", "chevron triple fast", svg(420, 240, ''.join(f'<path d="M{40 + i * 120},40l80,80l-80,80" {stroke(YELLOW, 36)}/>' for i in range(3))))
add("arrows", "diagonal", "arrow diagonal corner", svg(320, 320, f'<path d="M40,280L250,70" {stroke(WHITE, 28)}/><path d="M150,50h130v130z" fill="{WHITE}"/>'))
add("arrows", "outline", "arrow outline right", svg(400, 220, f'<path d="M20,80h220v-60l140,90l-140,90v-60h-220z" {stroke(VIOLET, 14)}/>'))
add("arrows", "loop", "arrow loop circle", svg(340, 340, f'<path d="M280,170A110,110 0 1 1 210,67" {stroke(BLUE, 24)}/><path d="M180,20l90,40l-75,60z" fill="{BLUE}"/>'))
add("arrows", "sketch", "arrow hand drawn sketch", svg(420, 220, f'<path d="M20,150C90,120 150,170 220,130S330,90 380,100" {stroke(INK, 14)}/><path d="M330,60L385,100L325,140" {stroke(INK, 14)}/>'))

# ── Badges ──────────────────────────────────────────────────────────────────
for spikes, colour, name in [(12, LIME, "seal-lime"), (16, YELLOW, "seal-yellow"), (20, RED, "seal-red"), (24, VIOLET, "seal-violet")]:
    add("badges", name, f"badge seal burst {name.split('-')[1]}", svg(320, 320, f'<polygon points="{points(star(160, 160, 155, 128, spikes))}" fill="{colour}"/>'))
add("badges", "check", "badge check tick done", svg(300, 300, f'<circle cx="150" cy="150" r="140" fill="{LIME}"/><path d="M85,155l45,45l90,-100" {stroke(INK, 30)}/>'))
add("badges", "cross", "badge cross no wrong", svg(300, 300, f'<circle cx="150" cy="150" r="140" fill="{RED}"/><path d="M100,100l100,100M200,100l-100,100" {stroke(WHITE, 30)}/>'))
add("badges", "shield", "badge shield secure", svg(280, 320, f'<path d="M140,10L270,55V150C270,230 210,285 140,310C70,285 10,230 10,150V55Z" fill="{BLUE}"/>'))
add("badges", "ribbon", "badge ribbon banner", svg(440, 180, f'<path d="M10,40h70v100h-70l30,-50z" fill="{RED}" opacity="0.8"/><path d="M430,40h-70v100h70l-30,-50z" fill="{RED}" opacity="0.8"/><rect x="60" y="20" width="320" height="110" rx="10" fill="{RED}"/>'))
add("badges", "rosette", "badge rosette award prize", svg(300, 380, f'<path d="M100,220l-40,150l60,-30l30,40l30,-150z" fill="{BLUE}"/><path d="M200,220l40,150l-60,-30l-30,40l-30,-150z" fill="{BLUE}"/><polygon points="{points(star(150, 140, 135, 115, 18))}" fill="{YELLOW}"/><circle cx="150" cy="140" r="85" fill="{ORANGE}"/>'))
add("badges", "crown", "badge crown king best", svg(360, 260, f'<path d="M20,230L40,60L120,140L180,30L240,140L320,60L340,230Z" fill="{YELLOW}"/>'))
add("badges", "bolt", "badge lightning bolt fast", svg(300, 300, f'<circle cx="150" cy="150" r="140" fill="{INK}"/><path d="M170,40L80,170h60l-20,90l90,-130h-60z" fill="{YELLOW}"/>'))
add("badges", "heart", "badge heart like love", svg(300, 300, f'<circle cx="150" cy="150" r="140" fill="{PINK}"/><path d="{heart_path(150, 155, 160)}" fill="{WHITE}"/>'))
add("badges", "star", "badge star favourite", svg(300, 300, f'<circle cx="150" cy="150" r="140" fill="{VIOLET}"/><polygon points="{points(star(150, 158, 100, 42, 5))}" fill="{WHITE}"/>'))
add("badges", "new-dot", "badge dot notification new", svg(200, 200, f'<circle cx="100" cy="100" r="90" fill="{RED}"/><circle cx="100" cy="100" r="40" fill="{WHITE}"/>'))
add("badges", "play", "badge play video", svg(300, 300, f'<circle cx="150" cy="150" r="140" fill="{RED}"/><path d="M115,90v120l100,-60z" fill="{WHITE}"/>'))

# ── Stickers ────────────────────────────────────────────────────────────────
def face(colour, mouth, eyes=""):
    return svg(300, 300, f'<circle cx="150" cy="150" r="140" fill="{colour}"/>{eyes or f"<circle cx=\"105\" cy=\"120\" r=\"18\" fill=\"{INK}\"/><circle cx=\"195\" cy=\"120\" r=\"18\" fill=\"{INK}\"/>"}{mouth}')

add("stickers", "smile", "sticker smile happy face", face(YELLOW, f'<path d="M90,180Q150,240 210,180" {stroke(INK, 16)}/>'))
add("stickers", "grin", "sticker grin laugh face", face(YELLOW, f'<path d="M85,170H215Q205,245 150,245Q95,245 85,170Z" fill="{INK}"/>'))
add("stickers", "wink", "sticker wink face", face(ORANGE, f'<path d="M95,185Q150,230 205,185" {stroke(INK, 16)}/>', f'<circle cx="105" cy="120" r="18" fill="{INK}"/><path d="M175,122h45" {stroke(INK, 14)}/>'))
add("stickers", "surprised", "sticker wow surprised face", face(LIME, f'<circle cx="150" cy="200" r="30" fill="{INK}"/>'))
add("stickers", "sad", "sticker sad face", face(BLUE, f'<path d="M95,215Q150,165 205,215" {stroke(INK, 16)}/>'))
add("stickers", "cool", "sticker cool sunglasses face", face(VIOLET, f'<path d="M100,195Q150,220 200,195" {stroke(INK, 14)}/>', f'<path d="M60,105H240V125Q240,160 200,160Q165,160 160,125H140Q135,160 100,160Q60,160 60,125Z" fill="{INK}"/>'))
add("stickers", "sparkle", "sticker sparkle shine star", svg(300, 300, f'<path d="M150,10Q165,135 290,150Q165,165 150,290Q135,165 10,150Q135,135 150,10Z" fill="{YELLOW}"/>'))
add("stickers", "fire", "sticker fire hot flame", svg(260, 340, f'<path d="M130,10C150,90 240,130 240,220A110,110 0 0 1 20,220C20,160 60,130 80,90C90,140 110,150 120,150C110,100 110,60 130,10Z" fill="{ORANGE}"/><path d="M130,170C150,210 185,230 185,265A55,55 0 0 1 75,265C75,230 110,210 130,170Z" fill="{YELLOW}"/>'))
add("stickers", "sun", "sticker sun sunny", svg(320, 320, ''.join(f'<path d="M160,160L{160 + 150 * math.cos(math.radians(a)):.0f},{160 + 150 * math.sin(math.radians(a)):.0f}" {stroke(YELLOW, 18)}/>' for a in range(0, 360, 45)) + f'<circle cx="160" cy="160" r="85" fill="{YELLOW}"/>'))
add("stickers", "moon", "sticker moon night", svg(300, 300, f'<path d="M200,20A140,140 0 1 0 280,200A110,110 0 1 1 200,20Z" fill="{WHITE}"/>'))
add("stickers", "cloud", "sticker cloud weather", svg(360, 220, f'<path d="M80,200A70,70 0 0 1 70,62A90,90 0 0 1 240,50A75,75 0 0 1 290,200Z" fill="{WHITE}"/>'))
add("stickers", "music", "sticker music note song", svg(260, 320, f'<path d="M100,250V50L230,20V220" {stroke(VIOLET, 22)}/><ellipse cx="70" cy="255" rx="45" ry="35" fill="{VIOLET}"/><ellipse cx="200" cy="225" rx="45" ry="35" fill="{VIOLET}"/>'))
add("stickers", "eyes", "sticker eyes look", svg(360, 200, f'<ellipse cx="95" cy="100" rx="80" ry="90" fill="{WHITE}"/><ellipse cx="265" cy="100" rx="80" ry="90" fill="{WHITE}"/><circle cx="120" cy="115" r="35" fill="{INK}"/><circle cx="290" cy="115" r="35" fill="{INK}"/>'))
add("stickers", "gift", "sticker gift present", svg(300, 320, f'<rect x="30" y="130" width="240" height="180" fill="{RED}"/><rect x="15" y="90" width="270" height="55" fill="{RED}"/><rect x="130" y="90" width="40" height="220" fill="{YELLOW}"/><path d="M150,90C110,20 50,40 80,90M150,90C190,20 250,40 220,90" {stroke(YELLOW, 18)}/>'))
add("stickers", "rocket", "sticker rocket launch", svg(260, 360, f'<path d="M130,10C200,70 210,170 190,260H70C50,170 60,70 130,10Z" fill="{WHITE}"/><circle cx="130" cy="130" r="32" fill="{BLUE}"/><path d="M70,200L20,280L75,260ZM190,200L240,280L185,260Z" fill="{RED}"/><path d="M95,265L130,350L165,265Z" fill="{ORANGE}"/>'))

# ── Doodles ─────────────────────────────────────────────────────────────────
add("doodles", "squiggle", "doodle squiggle line", svg(420, 140, f'<path d="M20,70C60,10 100,130 140,70S220,10 260,70S340,130 400,70" {stroke(LIME, 18)}/>'))
add("doodles", "spiral", "doodle spiral swirl", svg(300, 300, '<path d="' + "M150,150 " + " ".join(f"L{150 + (4 + t * 1.6) * math.cos(t * 0.35):.1f},{150 + (4 + t * 1.6) * math.sin(t * 0.35):.1f}" for t in range(1, 80)) + f'" {stroke(VIOLET, 12)}/>'))
add("doodles", "scribble-circle", "doodle scribble circle around", svg(360, 300, f'<path d="M180,30C300,25 345,120 300,210C250,295 80,290 35,190C0,100 90,20 210,40" {stroke(RED, 12)}/>'))
add("doodles", "underline", "doodle underline swoosh", svg(420, 100, f'<path d="M20,70C140,30 280,30 400,50" {stroke(YELLOW, 20)}/>'))
add("doodles", "sparkles", "doodle sparkles stars shine", svg(320, 300, ''.join(f'<path d="M{x},{y - r}Q{x + r * 0.15},{y - r * 0.15} {x + r},{y}Q{x + r * 0.15},{y + r * 0.15} {x},{y + r}Q{x - r * 0.15},{y + r * 0.15} {x - r},{y}Q{x - r * 0.15},{y - r * 0.15} {x},{y - r}Z" fill="{WHITE}"/>' for x, y, r in [(110, 150, 90), (240, 70, 50), (250, 230, 40)])))
add("doodles", "loop", "doodle loop line", svg(420, 200, f'<path d="M20,150C120,150 160,40 210,40S260,150 210,150S150,60 260,60S360,150 400,150" {stroke(BLUE, 14)}/>'))
add("doodles", "zigzag", "doodle zigzag line", svg(420, 120, f'<path d="M20,90L70,30L120,90L170,30L220,90L270,30L320,90L370,30L400,60" {stroke(PINK, 14)}/>'))
add("doodles", "dots", "doodle dots confetti", svg(320, 320, ''.join(f'<circle cx="{x}" cy="{y}" r="{r}" fill="{c}"/>' for x, y, r, c in [(60, 70, 22, LIME), (200, 50, 16, PINK), (270, 150, 26, YELLOW), (120, 170, 18, BLUE), (60, 260, 14, ORANGE), (220, 270, 20, VIOLET)])))
add("doodles", "wave", "doodle wave water", svg(420, 160, f'<path d="M20,110C70,30 120,30 170,110S270,190 320,110S390,40 400,60" {stroke(BLUE, 18)}/>'))
add("doodles", "hatch", "doodle hatch lines shade", svg(300, 300, ''.join(f'<path d="M{20 + i * 40},280L{100 + i * 40},20" {stroke(WHITE, 10)}/>' for i in range(5))))
add("doodles", "burst-lines", "doodle burst lines action", svg(320, 320, ''.join(f'<path d="M{160 + 70 * math.cos(math.radians(a)):.0f},{160 + 70 * math.sin(math.radians(a)):.0f}L{160 + 150 * math.cos(math.radians(a)):.0f},{160 + 150 * math.sin(math.radians(a)):.0f}" {stroke(YELLOW, 14)}/>' for a in range(0, 360, 30))))
add("doodles", "heart-doodle", "doodle heart outline love", svg(300, 300, f'<path d="{heart_path(150, 150, 250)}" {stroke(PINK, 14)}/>'))
add("doodles", "star-doodle", "doodle star outline", svg(300, 300, f'<polygon points="{points(star(150, 160, 135, 55, 5))}" {stroke(YELLOW, 12)}/>'))
add("doodles", "check", "doodle check tick", svg(300, 240, f'<path d="M30,130L110,200L270,30" {stroke(LIME, 26)}/>'))
add("doodles", "cross", "doodle cross x", svg(260, 260, f'<path d="M40,40L220,220M220,40L40,220" {stroke(RED, 26)}/>'))

# ── Bubbles ─────────────────────────────────────────────────────────────────
def speech(colour, tail_left=True):
    tail = "M90,225L60,300L160,225Z" if tail_left else "M310,225L340,300L240,225Z"
    return svg(400, 310, f'<rect x="10" y="10" width="380" height="230" rx="110" fill="{colour}"/><path d="{tail}" fill="{colour}"/>')


add("bubbles", "speech-left", "bubble speech talk", speech(WHITE, True))
add("bubbles", "speech-right", "bubble speech talk", speech(WHITE, False))
add("bubbles", "speech-lime", "bubble speech talk lime", speech(LIME, True))
add("bubbles", "speech-violet", "bubble speech talk violet", speech(VIOLET, False))
add("bubbles", "speech-yellow", "bubble speech talk yellow", speech(YELLOW, True))
add("bubbles", "square-left", "bubble square message chat", svg(400, 300, f'<rect x="10" y="10" width="380" height="220" rx="30" fill="{WHITE}"/><path d="M60,225L40,290L130,225Z" fill="{WHITE}"/>'))
add("bubbles", "square-right", "bubble square message chat", svg(400, 300, f'<rect x="10" y="10" width="380" height="220" rx="30" fill="{BLUE}"/><path d="M340,225L360,290L270,225Z" fill="{BLUE}"/>'))
add("bubbles", "thought", "bubble thought think", svg(400, 330, f'<ellipse cx="200" cy="120" rx="185" ry="110" fill="{WHITE}"/><circle cx="90" cy="255" r="28" fill="{WHITE}"/><circle cx="50" cy="305" r="16" fill="{WHITE}"/>'))
add("bubbles", "shout", "bubble shout yell loud", svg(400, 320, f'<polygon points="{points(star(200, 160, 190, 125, 14))}" fill="{YELLOW}"/>'))
add("bubbles", "shout-red", "bubble shout yell loud red", svg(400, 320, f'<polygon points="{points(star(200, 160, 190, 120, 11))}" fill="{RED}"/>'))
add("bubbles", "oval-outline", "bubble oval outline", svg(400, 300, f'<path d="M200,15C310,15 385,70 385,125S310,235 200,235C175,235 150,232 130,226L60,285L85,205C40,185 15,155 15,125C15,70 90,15 200,15Z" {stroke(WHITE, 12)}/>'))
add("bubbles", "cloud", "bubble cloud think", svg(400, 280, f'<path d="M90,220A60,60 0 0 1 60,110A80,80 0 0 1 190,50A80,80 0 0 1 330,80A70,70 0 0 1 320,220Z" fill="{WHITE}"/><circle cx="70" cy="255" r="18" fill="{WHITE}"/>'))
add("bubbles", "whisper", "bubble whisper dashed", svg(400, 300, f'<rect x="15" y="15" width="370" height="210" rx="100" {stroke(WHITE, 8)} stroke-dasharray="20 14"/><path d="M90,225L60,290L150,226" {stroke(WHITE, 8)}/>'))
add("bubbles", "pink", "bubble speech pink", speech(PINK, False))
add("bubbles", "ink", "bubble speech dark", speech(INK, True))

# ── Highlights ──────────────────────────────────────────────────────────────
def marker(colour):
    return svg(420, 120, f'<path d="M15,30C120,15 300,18 405,28L398,95C300,103 120,104 22,92Z" fill="{colour}" opacity="0.55"/>')

add("highlights", "marker-yellow", "highlight marker yellow", marker(YELLOW))
add("highlights", "marker-lime", "highlight marker lime", marker(LIME))
add("highlights", "marker-pink", "highlight marker pink", marker(PINK))
add("highlights", "marker-blue", "highlight marker blue", marker(BLUE))
add("highlights", "circle", "highlight circle around ring", svg(360, 300, f'<ellipse cx="180" cy="150" rx="160" ry="125" {stroke(RED, 12)}/>'))
add("highlights", "circle-lime", "highlight circle around ring lime", svg(360, 300, f'<ellipse cx="180" cy="150" rx="160" ry="125" {stroke(LIME, 12)}/>'))
add("highlights", "underline", "highlight underline", svg(420, 60, f'<path d="M15,30H405" {stroke(LIME, 16)}/>'))
add("highlights", "double-underline", "highlight underline double", svg(420, 80, f'<path d="M15,22H405M35,58H385" {stroke(YELLOW, 12)}/>'))
add("highlights", "box", "highlight box frame rectangle", svg(400, 260, f'<rect x="15" y="15" width="370" height="230" rx="12" {stroke(WHITE, 10)}/>'))
add("highlights", "brackets", "highlight brackets focus", svg(400, 260, f'<path d="M70,15H15V245H70M330,15H385V245H330" {stroke(LIME, 14)}/>'))
add("highlights", "corners", "highlight corners focus frame", svg(400, 300, f'<path d="M15,85V15H85M315,15H385V85M385,215V285H315M85,285H15V215" {stroke(WHITE, 12)}/>'))
add("highlights", "strike", "highlight strike through", svg(420, 60, f'<path d="M15,30H405" {stroke(RED, 12)}/>'))
add("highlights", "glow", "highlight glow spot light", svg(320, 320, f'<defs><radialGradient id="g"><stop offset="0" stop-color="{YELLOW}" stop-opacity="0.9"/><stop offset="1" stop-color="{YELLOW}" stop-opacity="0"/></radialGradient></defs><circle cx="160" cy="160" r="155" fill="url(#g)"/>'))
add("highlights", "spotlight", "highlight spotlight beam", svg(300, 400, f'<defs><linearGradient id="b" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{WHITE}" stop-opacity="0.7"/><stop offset="1" stop-color="{WHITE}" stop-opacity="0"/></linearGradient></defs><path d="M120,10H180L290,390H10Z" fill="url(#b)"/>'))
add("highlights", "pointer", "highlight pointer dot tap", svg(260, 260, f'<circle cx="130" cy="130" r="120" fill="{WHITE}" opacity="0.35"/><circle cx="130" cy="130" r="55" fill="{WHITE}"/>'))

# ── Label chips ─────────────────────────────────────────────────────────────
for colour_name in ["lime", "violet", "white", "ink", "red", "yellow", "blue", "pink"]:
    add("labels", f"pill-{colour_name}", f"label chip pill {colour_name}", svg(440, 140, f'<rect x="5" y="5" width="430" height="130" rx="65" fill="{COLOURS[colour_name]}"/>'))
add("labels", "pill-outline", "label chip pill outline", svg(440, 140, f'<rect x="10" y="10" width="420" height="120" rx="60" {stroke(WHITE, 10)}/>'))
add("labels", "tag", "label tag price", svg(440, 180, f'<path d="M90,10H430V170H90L10,90Z" fill="{YELLOW}"/><circle cx="85" cy="90" r="16" fill="{INK}"/>'))
add("labels", "ticket", "label ticket coupon", svg(440, 180, f'<path d="M10,10H430V65A25,25 0 0 0 430,115V170H10V115A25,25 0 0 0 10,65Z" fill="{PINK}"/>'))
add("labels", "box", "label box rectangle", svg(440, 150, f'<rect x="5" y="5" width="430" height="140" rx="16" fill="{INK}"/>'))
add("labels", "box-lime", "label box rectangle lime", svg(440, 150, f'<rect x="5" y="5" width="430" height="140" rx="16" fill="{LIME}"/>'))
add("labels", "flag", "label flag banner", svg(440, 160, f'<path d="M10,10H430L390,80L430,150H10Z" fill="{RED}"/>'))
add("labels", "slanted", "label slanted tilted", svg(460, 160, f'<path d="M50,10H450L410,150H10Z" fill="{VIOLET}"/>'))

CATEGORIES = ["badges", "arrows", "stickers", "shapes", "doodles", "bubbles", "highlights", "labels"]


def main():
    for category in CATEGORIES:
        (OUT / category).mkdir(parents=True, exist_ok=True)
        for stale in (OUT / category).glob("*.svg"):
            stale.unlink()
    names = set()
    rows = []
    entries = []
    for category, name, tags, content in ITEMS:
        assert name not in names, name
        names.add(name)
        (OUT / category / f"{name}.svg").write_text(content)
        rows.append(f"| `{category}/{name}.svg` | {category} | {tags} |")
        entries.append(
            f'    LibraryAsset {{ name: "{name}", category: "{category}", tags: "{tags}", '
            f'svg: include_str!("../library/{category}/{name}.svg") }},'
        )
    counts = {c: sum(1 for item in ITEMS if item[0] == c) for c in CATEGORIES}
    manifest = [
        "# Shorted library",
        "",
        "Every file here is drawn by `scripts/generate_library.py`: project-made, under the "
        "project's licence, with no third-party artwork. Edit the script, not the files.",
        "",
        f"{len(ITEMS)} items: " + ", ".join(f"{c} {n}" for c, n in counts.items()) + ".",
        "",
        "| File | Category | Search words |",
        "| --- | --- | --- |",
        *rows,
        "",
    ]
    (OUT / "MANIFEST.md").write_text("\n".join(manifest))
    index = [
        "// Generated by scripts/generate_library.py. Do not edit.",
        "",
        "use crate::library::LibraryAsset;",
        "",
        "pub const ASSETS: &[LibraryAsset] = &[",
        *entries,
        "];",
        "",
    ]
    (CRATE / "src" / "library_index.rs").write_text("\n".join(index))
    print(f"{len(ITEMS)} items")


if __name__ == "__main__":
    main()
