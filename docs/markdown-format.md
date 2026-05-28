# Markdown Export Formatı (V0.3 kontratı)

Voice Notes TR'nin "Markdown'a Kopyala" butonu sabit ve regex-dostu bir
markdown çıkartır. Bu doküman formatın **kontratıdır** — downstream parser'lar
(Odoo wizard, n8n, Obsidian script vb.) bu yapıya güvenebilir.

## Birebir örnek

[`docs/sample-export.md`](./sample-export.md) gerçek bir kayıt çıktısı.

## Bölümler ve sıra

1. **Başlık** (`# {title}`)
2. **Metadata** (bold-prefixed satırlar)
3. **Aksiyonlar** (`## Aksiyonlar`) — sadece en az 1 aksiyon varsa
4. **Kararlar** (`## Kararlar`) — sadece en az 1 karar varsa
5. **Transkript** (`## Transkript`) — her zaman var

**Garanti:** Bölüm başlıkları her zaman tam olarak şu stringlerdir:
`## Aksiyonlar`, `## Kararlar`, `## Transkript`.

## Başlık

```markdown
# Recording (3)
```

veya canlı mod için:

```markdown
# Canlı kayıt — 2026-05-28
```

- Dosya modunda: yüklenen dosya adının uzantısız hâli
- Canlı modda: `Canlı kayıt — YYYY-MM-DD`

## Metadata bloku

Her satır `**Anahtar:** Değer` formatında:

```markdown
**Tarih:** 2026-05-28
**Süre:** 0:34
**Dil:** tr (100%)
```

| Alan | Format | Garanti |
|---|---|---|
| `Tarih` | ISO date `YYYY-MM-DD` | Her zaman |
| `Süre` | `M:SS` (dakika:saniye, zero-padded saniye) | Var olabilir |
| `Dil` | `{iso} ({prob}%)` (örn. `tr (98%)`) | Var olabilir |

## Aksiyonlar

```markdown
## Aksiyonlar

- **Ahmet** — Yarınki sunum için slaytları hazırla
- **Furkan** — Raporu cuma günü gönder
- Mert müşteri sözleşmesini pazartesi hukuk birimine iletecek
```

Her satır `- ` ile başlar. Atayan varsa **bold + em-dash + space** ile ayrılır:
- `- **{Ad}** — {Aksiyon metni}`
- `- {Aksiyon metni}` (atayan yok)

**Garanti:** Em-dash karakteri `—` (U+2014). Hyphen değil. Python `re` ile
sorunsuz, sadece UTF-8 okumayı unutma.

## Kararlar

```markdown
## Kararlar

- Bütçe 50 bin lira olarak onaylandı
- X kütüphanesini kullanacağız
```

Düz bullet listesi. Atayan yok, sadece karar metni.

## Transkript

```markdown
## Transkript

`0:00–0:01` Sesim geliyor mu?
`0:01–0:03` Benim sesim geliyor mu?
`0:05–0:07` Ben de fısıldadım.
```

Her satır biçimi:

```
`{start_m}:{start_ss}–{end_m}:{end_ss}` {metin}
```

Zaman damgaları **en-dash** `–` (U+2013) ile ayrılır. Hyphen değil. Dakikalar
zero-pad'siz, saniyeler `MM` (iki haneli). Metin trimmed, tek satırda.

## Python parser referansı

Odoo wizard veya başka entegrasyon için tam çalışan örnek:

```python
import re
from dataclasses import dataclass, field
from datetime import date

@dataclass
class ActionItem:
    text: str
    assignee: str | None = None

@dataclass
class TranscriptSegment:
    start_s: int
    end_s: int
    text: str

@dataclass
class Meeting:
    title: str
    date: date | None = None
    duration_s: int | None = None
    language: str | None = None
    language_probability: float | None = None
    actions: list[ActionItem] = field(default_factory=list)
    decisions: list[str] = field(default_factory=list)
    segments: list[TranscriptSegment] = field(default_factory=list)

TITLE_RE = re.compile(r"^#\s+(.+)$", re.M)
META_RE = re.compile(r"^\*\*(?P<key>[^:]+):\*\*\s+(?P<value>.+)$", re.M)
SECTION_RE = re.compile(r"^## (Aksiyonlar|Kararlar|Transkript)\s*$", re.M)
ACTION_RE = re.compile(r"^-\s+(?:\*\*(?P<who>[^*]+)\*\*\s+—\s+)?(?P<text>.+)$")
DECISION_RE = re.compile(r"^-\s+(?P<text>.+)$")
SEGMENT_RE = re.compile(
    r"^`(?P<sm>\d+):(?P<ss>\d{2})–(?P<em>\d+):(?P<es>\d{2})`\s+(?P<text>.+)$"
)
DURATION_RE = re.compile(r"^(\d+):(\d{2})$")
LANG_RE = re.compile(r"^(?P<code>\w+)\s+\((?P<prob>\d+)%\)$")


def parse_meeting_md(md: str) -> Meeting:
    title_m = TITLE_RE.search(md)
    meeting = Meeting(title=title_m.group(1) if title_m else "")

    for m in META_RE.finditer(md):
        k, v = m.group("key").strip(), m.group("value").strip()
        if k == "Tarih":
            meeting.date = date.fromisoformat(v)
        elif k == "Süre":
            dm = DURATION_RE.match(v)
            if dm:
                meeting.duration_s = int(dm.group(1)) * 60 + int(dm.group(2))
        elif k == "Dil":
            lm = LANG_RE.match(v)
            if lm:
                meeting.language = lm.group("code")
                meeting.language_probability = int(lm.group("prob")) / 100

    # Bölümleri sırayla yakala
    sections: dict[str, str] = {}
    matches = list(SECTION_RE.finditer(md))
    for i, m in enumerate(matches):
        name = m.group(1)
        start = m.end()
        end = matches[i + 1].start() if i + 1 < len(matches) else len(md)
        sections[name] = md[start:end].strip()

    if "Aksiyonlar" in sections:
        for line in sections["Aksiyonlar"].splitlines():
            am = ACTION_RE.match(line.strip())
            if am:
                meeting.actions.append(
                    ActionItem(
                        text=am.group("text").strip(),
                        assignee=am.group("who").strip() if am.group("who") else None,
                    )
                )

    if "Kararlar" in sections:
        for line in sections["Kararlar"].splitlines():
            dm = DECISION_RE.match(line.strip())
            if dm:
                meeting.decisions.append(dm.group("text").strip())

    if "Transkript" in sections:
        for line in sections["Transkript"].splitlines():
            sm = SEGMENT_RE.match(line.strip())
            if sm:
                start_s = int(sm.group("sm")) * 60 + int(sm.group("ss"))
                end_s = int(sm.group("em")) * 60 + int(sm.group("es"))
                meeting.segments.append(
                    TranscriptSegment(start_s, end_s, sm.group("text").strip())
                )

    return meeting


if __name__ == "__main__":
    import sys
    md = sys.stdin.read()
    m = parse_meeting_md(md)
    print(f"Başlık: {m.title}")
    print(f"Tarih: {m.date} · Süre: {m.duration_s}s · Dil: {m.language}")
    print(f"{len(m.actions)} aksiyon, {len(m.decisions)} karar, {len(m.segments)} segment")
```

Kullanım:

```bash
cat docs/sample-export.md | python parser.py
```

## Kararlı kalan şeyler (versiyonlama)

V0.3 garantisi olarak değişmeyecek:

- Bölüm başlıkları: `## Aksiyonlar`, `## Kararlar`, `## Transkript`
- Metadata bold prefix formatı: `**Anahtar:** Değer`
- Aksiyon ayraç karakteri: ` — ` (U+2014 em-dash, boşlukla çevrili)
- Transkript zaman ayracı: `–` (U+2013 en-dash)
- Tarih formatı: ISO `YYYY-MM-DD`

Değişmesi planlananlar (V1.0+):

- `## Konuşmacılar` bölümü (diarization Faz 3+'ta gelirse)
- `**Notion ID:** ...` gibi ek metadata (export hedeflerinden gelirse)
- Frontmatter YAML alternatifi (gerek olursa, geri uyumlu)
