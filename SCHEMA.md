# SQLite Schema

V1 şeması. Dosya modu ve canlı mod aynı tablolara yazar. Diarization V2'ye, ama `speaker_id` alanı bugün hazır.

Sürüm: 1. Migration mekanizması Faz 1'de eklenir (basit `PRAGMA user_version` + numbered SQL files).

## Tablolar

```sql
-- Bir toplantı = bir oturum (dosya yükleme veya canlı kayıt)
CREATE TABLE meetings (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    title           TEXT NOT NULL,
    started_at      INTEGER NOT NULL,        -- unix epoch (saniye)
    duration_s      REAL,                    -- transcript bitince doldurulur
    source          TEXT NOT NULL,           -- 'file' | 'live'
    audio_path      TEXT,                    -- 'file' modunda orijinal dosya yolu
    language        TEXT NOT NULL DEFAULT 'tr',
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- Transkript segmentleri (Whisper'ın çıktısı, zaman damgalı)
CREATE TABLE segments (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id      INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    start_ms        INTEGER NOT NULL,        -- toplantı başından milisaniye
    end_ms          INTEGER NOT NULL,
    text            TEXT NOT NULL,
    speaker_id      INTEGER,                 -- V2: diarization sonucu. V1: hep NULL
    edited          INTEGER NOT NULL DEFAULT 0,  -- kullanıcı manuel düzeltti mi
    created_at      INTEGER NOT NULL
);
CREATE INDEX idx_segments_meeting ON segments(meeting_id, start_ms);

-- LLM'in çıkardığı aksiyon maddeleri
CREATE TABLE action_items (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id      INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    text            TEXT NOT NULL,
    assignee        TEXT,                    -- "Furkan", "Ahmet" — LLM çıkarırsa
    due_at          INTEGER,                 -- opsiyonel, ileride NLP ile
    source_segment_id INTEGER REFERENCES segments(id) ON DELETE SET NULL,
                                             -- hangi segmentten çıktı (opsiyonel)
    completed       INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
CREATE INDEX idx_actions_meeting ON action_items(meeting_id);

-- LLM'in çıkardığı kararlar
CREATE TABLE decisions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id      INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    text            TEXT NOT NULL,
    source_segment_id INTEGER REFERENCES segments(id) ON DELETE SET NULL,
    created_at      INTEGER NOT NULL
);
CREATE INDEX idx_decisions_meeting ON decisions(meeting_id);

-- V2'ye hazırlık: konuşmacılar (V1'de kullanılmaz)
CREATE TABLE speakers (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id      INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    label           TEXT NOT NULL,           -- "Konuşmacı 1", kullanıcı sonra "Ahmet" eder
    embedding       BLOB,                    -- speaker embedding (V2)
    created_at      INTEGER NOT NULL
);

PRAGMA user_version = 1;
```

## Tasarım notları

### Niye `source` enum string olarak?

SQLite enum desteği yok. CHECK constraint ekleyebiliriz: `CHECK (source IN ('file', 'live'))`. Faz 1'de ekleriz.

### Niye `start_ms` INTEGER (REAL değil)?

Milisaniye granularitesi yeterli. Index/sort daha hızlı. Whisper float saniye dönüyor, `(start * 1000)::int` ile yazılır.

### `speaker_id` neden bugün var?

V2'de eklemek = `ALTER TABLE segments ADD COLUMN speaker_id` — sorun değil aslında. Ama UI/serde tarafında `Option<i64>` her yere yayılır, bugün eklemek migration riskini sıfırlar.

### Cascade delete

Bir toplantı silinince segments/action_items/decisions/speakers hepsi gider. Kullanıcı "toplantıyı sil" deyince bütünsel silinir, manuel cleanup gerekmez.

### `updated_at` nerede var, nerede yok?

`meetings` ve `action_items`'ta var — kullanıcı düzenleyebileceği için. `segments`'te `edited` flag yeterli; her segment için updated_at yığını az faydalı. `decisions` immutable kabul edilir (LLM yazar, kullanıcı silebilir ama düzenlemez — bu bir karar, gerekirse değişir).

## V2'de neler değişir (öngörü)

- `speakers` aktif kullanılır, `segments.speaker_id` doldurulmaya başlar
- `meetings`'e `tags` (TEXT, JSON array) eklenebilir
- Full-text search için `segments_fts` (FTS5 virtual table) eklenir — Türkçe arama için
- `notion_page_id`, `synced_at` gibi export tracking alanları

Bunlar şimdi yok; eklemenin maliyeti orta.
