# Audit formi QNC-AI-ja: Ingest, AI Media Assist, MA Script, MA Audio, MA Video, S-Story, Story, Project, Export, shell (2026-11-21)

Mjera: `cargo metadata` zatvarač runtime ovisnosti svake aplikacije (`apps/*`, `tools/*`) nad
QNC-AI i QNC-shared; redci su `src/**/*.rs`. Dopuna audita tray lanca
(`05-audit-tray-lanac-2026-11-21.md`) i audita kocki QNC-AI-ja (`QNC-AI/docs/03`, `04`).

## 1. Brojke po aplikaciji

| aplikacija | kocki | redaka | iz QNC-AI | redaka AI | s egui |
|---|---|---|---|---|---|
| `qnc-app` (shell, sve forme u jednom procesu) | 319 | 147 962 | 95 | 42 671 | 77 |
| `qnc-project` | 104 | 44 435 | 22 | 10 335 | 31 |
| `qnc-ingest` | 217 | 107 381 | 50 | 22 804 | 49 |
| AI Media Assist, MA Script, MA Audio, MA Video, S-Story, Story (svaka) | 251 | 123 334 | 69 | 29 890 | 55 |
| `qnc-ai-projects` / `-settings` / `-admin` | 120-123 | ~50 000 | 32-34 | ~12 600 | 35-36 |
| `qnc-export` | 77 | 64 031 | 5 | 1 453 | 0 |
| `qnc-ai-tray` | 67 | 33 418 | 25 | 9 985 | 0 |
| `qnc-ai-card-ingest` | 217 | 109 282 | 56 | 25 159 | 40 |
| `qnc-ingest-worker` | 111 | 67 461 | 27 | 16 286 | 0 |

- Zajednička jezgra svih sedam formi: **170 kocki, 87 665 redaka** (player, priča, katalog,
  dekoderi, baza, tema, timeline).
- Šest editorial formi ima **isti zatvarač**: to je jedna aplikacija (`qnc-editorial-*`) s
  oznakom grupe; MA Video (grupa `l`, desni panel `none`) kompajlira i nosi prijepis, scenarij,
  priču i AI iako ništa od toga ne koristi.
- Kocke QNC-AI-ja koje nijedna aplikacija ne koristi: `qnc-wizard-db` 309, `qnc-wizard-flow`
  242, `qnc-wizard-steps` 124, `qnc-script-profile-editor` 253.

## 2. Što koja forma stvarno jest (ugovor `contracts/ui/editorial.layout.json`)

Svaka editorial forma je grupa istog rasporeda: monitor, transport, tabovi poola, pool,
source timeline s gumbima, desni panel. Razlikuju se samo:

| forma | grupa | desni panel | tabovi poola | gumbi source timelinea | ostalo |
|---|---|---|---|---|---|
| AI Media Assist | e | `transcript_panel` (+ tab `video_ai_panel`) | All, Transcript | Add statement, Update statement | ispod poola: Transcribe, Ponovo (HR) |
| MA Script | g | `script_writer_panel` | All, SOT, SubSegment, Voice | Add statement, Update, SubSegment, Voice over, B-roll | prozor medija (`transcript_search_panel` + `script_clip_bar`) |
| MA Audio | h | `script_panel` | All, SOT, SubSegment, Voice | isti kao g | |
| MA Video | l | `none` | (osnovni) | (osnovni) | prazna forma |
| S-Story | k | `draft_segment_panel` | All, Stories, SOT, B-roll, Segment | Talking Head, Voice over, B-roll, Sync/B-roll, Send to Story | priča verzije `story-draft-<n>.db` |
| Story | o | `segment_panel` | All, SOT, B-roll, AI B-roll, Segment | Talking Head, Voice over, B-roll, Sync/B-roll, Update, Add SubClip | |

Desni paneli i kocke iza njih (`qnc-right-panel` 72, `qnc-side-pieces` 368 odlučuje koje dijelove
forma ima po rasporedu):

| panel | crta | modul stanja | baza |
|---|---|---|---|
| `segment_panel`, `draft_segment_panel` | `qnc-segment-panel` 976 | `qnc-program-segments` 6416 (25 ovisnosti), `qnc-segments-keys` 333, `qnc-wrap-session` 234, `qnc-sync-session` 133 + `qnc-sync-cover` 585 | `qnc-program-db` 7403, `qnc-virtual-shots` 1153, `qnc-skeleton-db` 223 |
| `transcript_panel`, `transcript_search_panel` | `qnc-transcript-panel` 402 | `qnc-transcript` 885, `qnc-clip-transcripts` 198, `qnc-stream-transcript` 146 | `qnc-transcript-db` 1218 |
| `video_ai_panel` | `qnc-shot-panel` 124 | `qnc-clip-shots` 121 | `qnc-shot-db` 393, `qnc-shot-content-db` 384 |
| `script_panel` | `qnc-script-panel` 701 | `qnc-script` 2353 (36 ovisnosti), `qnc-subsegment-edit` 2122 (22), `qnc-script-draft` 343, `qnc-script-export` 335, `qnc-script-profile` 394 | `qnc-script-db` 2040 |
| `script_writer_panel` | `qnc-script-writer-panel` 329 | isti kao `script_panel` + `qnc-voice-take` 321, `qnc-voice-record` 340, `qnc-voice-clip` 254, `qnc-session-readings` 290, `qnc-read-check` 279, `qnc-take-heard` 155 | `qnc-script-db` |
| tab `ai_broll_tab` | kartice poola | `qnc-ai-broll` 655 (20), `qnc-broll-suggest` 601 | `qnc-shot-examples` 204, `qnc-shot-vocabulary` 127 |

Zajedničko svim editorial formama izvan jezgre: `qnc-panel-focus` 1284 (30 ovisnosti; tipke po
panelu), `qnc-editorial-application` 730 (29), `qnc-editorial-blocks` 168 (24, egui),
`qnc-editorial-desktop` 101, `qnc-ai-editorial-layout` 564, `qnc-trim-stills` 785 + `qnc-zoom-words`
473 + `qnc-letter-times` 174 + `qnc-word-align` 303 (zoom reza), `qnc-short-save` 344,
`qnc-virtual-short-stills` 341, `qnc-virtual-short-cards` 255, `qnc-shot-stills` 205,
`qnc-playhead-moves` 114, `qnc-keyframe-nav` 176, `qnc-keyframe-jump` 53, `qnc-smooth-joins` 215
+ `qnc-frame-interp` 109, `qnc-form-host` 77, `qnc-form-window` 86, `qnc-key-intents` 97,
`qnc-key-cheatsheet` 132, `qnc-export-box` 122 + `qnc-export-choice` 357 + `qnc-export-launch` 92.

### Ingest (grupa a/b, vlastiti raspored `ingest.layout.json`)

`qnc-ingest-application` 1728 (36 ovisnosti), `qnc-ingest-select` 2584 (25), `qnc-ingest-import-worker`
2016 (22), `qnc-ingest-catalog` 460, `qnc-ingest-blocks` 214 (egui), `qnc-ingest-desktop` 131,
`qnc-ingest-project` 180, `qnc-ingest-select-run` 114, `qnc-ingest-import-run` 106,
`qnc-ingest-clip-list` 318, `qnc-ingest-preview-source` 274, `qnc-ingest-layout` 264,
`qnc-ingest-import-bar` 91, `qnc-ingest-base` 1181 (7 sirovih otvaranja SQLite-a), `qnc-focus-walk`
306. Zatvarač 217 kocki jer vuče player, timeline i katalog kamera.

### Project (grupa a)

`qnc-project-store` 3614, `qnc-project-blocks` 1415 (egui), `qnc-project-session` 691,
`qnc-project-jobs` 262, `qnc-project-application` 208, `qnc-project-desktop` 185, obrazac priloga
`qnc-newsroom-sheet*`, AI mogućnosti `qnc-ai-features` 463 + `-view`, postavke `qnc-settings-form` 379.
Zatvarač 104 kocke; GUI u 31 kocki.

### Shell (`qnc-app`, 88 redaka) i Export

`qnc-desktop` 934, `qnc-desktop-tabs` 177, `qnc-shell-footer` 205, adapteri po formi; jedan proces
nosi sve forme (319 kocki, 148 000 redaka). `tools/qnc-export` 329 redaka nad `qnc-program-render`,
`qnc-unit-join`, `qnc-fcp7-xml`, prerenderom; zatvarač 77 kocki bez GUI-ja.

## 3. Nalazi (uz audit 03 §D-G, potvrđeni mjerom)

1. **Jedna aplikacija za šest formi.** Raspored je podatak (dobro), ali kod nije podijeljen: svaka
   forma nosi sve module i sve baze. MA Video je prazna forma od 123 000 redaka.
2. **Pravila u orkestratorima.** `qnc-panel-focus` (1284, 30 ovisnosti) i
   `qnc-editorial-application` (730, 29) odlučuju tko dobiva tipku i što se crta; isto u Ingestu
   (`qnc-ingest-application` 1728) i tray lancu.
3. **Kocke koje su više kocki**: `qnc-program-segments` 6416, `qnc-script` 2353,
   `qnc-subsegment-edit` 2122, `qnc-source-preview` 1507, `qnc-ingest-select` 2584,
   `qnc-ingest-import-worker` 2016, `qnc-project-store` 3614, `qnc-content-store` 3206,
   `qnc-program-db` 7403, `qnc-timeline` 3124 (egui).
4. **Isto napisano više puta** (audit 03 §E): prag glasa 3×, mapa tipki krivulje 3×, `say_level`
   3×, `slip/trim/save_and_keep` 2×, čitač finog vala 3-5 primjeraka po formi, čitač prijepisa
   2-3 čitanja baze u sekundi, slika monitora 4×.
5. **GUI u logici**: 55 od 251 kocki editorial forme ima egui; među njima `qnc-timeline` 3124 i
   `qnc-ai-editorial-layout` 564 koje logičke kocke vuku zbog tipova.
6. **Baza mimo vlasnika**: `qnc-ingest-base` 7 sirovih `Connection::open`, `qnc-project-store` 3,
   `qnc-shot-captions`, `qnc-camera-profiles`; AI vektori i analiza kadrova u `ai.db`, prijepis u
   `project.db`, opisi i primjeri u bazama računala (tri mjesta istine).
7. **Zakrpe po komentarima** (`user/live 2026-..`): program-segments 124, timeline 82,
   program-db 75, program-view 60, subsegment-edit 60, script 45, panel-focus 44.
8. **Undo**: samo priča (`story_undo`) i scenarij (`script_undo`); izjave, prijepis, kadrovi,
   postavke i Project nemaju undo.

## 4. Što QNC-pro od toga ima (21. 11.)

Jezgra (dijagnostika, tekst, tema, ui-kit, radnje, postavke računala, pokretanje, baza, registar,
Project s obrascem i templateima, player ulaz i sesija, source timeline, timeline panel, zoom,
oznake, val čitanje, kartica i disk, kopija, probe, katalog, projekt napravi/otvori/obriši,
tray, prozor uvoza). Nema: pozadinski rad (valovi, filmstrip, prijepis, AI), ni jednu
editorial formu, Ingest formu, Export.

## 5. Redoslijed prijenosa formi (nakon tray lanca iz dokumenta 05)

Pravilo: forma = raspored (grupa u layout ugovoru) + paneli; panel = modul + crtanje; modul samo
nad svojim kockama i svojom bazom. Nijedna forma ne kompajlira module drugih formi: ovisnost
forme na panel ide kroz raspored, a aplikacija forme ovisi samo o panelima koje njezin raspored
imenuje.

1. **AI Media Assist** (prva forma Breaking news): panel prijepisa (`qnc-transcript-panel` ←
   `qnc-transcript` ← `qnc-transcript-db`, čitač prijepisa jedan), tab Prijepis, Add/Update
   statement (`qnc-short-save` ← `qnc-virtual-shots`, slike izjave), Video AI panel
   (`qnc-shot-panel` ← `qnc-clip-shots` ← `qnc-shot-db`). Treba prijepis iz tray lanca (korak 4
   dokumenta 05).
2. **MA Script**: scenarij (`qnc-script-db` + `qnc-script` rastavljen: blokovi, dijelovi,
   tekst, riječi, nacrt, izvoz), panel pisca, profil scenarija, snimanje OFF-a
   (`qnc-voice-record/-take/-clip/-catalog`), provjera čitanja, prozor medija. MA Audio je isti
   scenarij s drugim panelom.
3. **S-Story i Story**: priča (`qnc-program-db` rastavljen po radnjama), Wrap redovi
   (`qnc-segment-panel`), `qnc-program-segments` rastavljen (u QNC-shared već: commands, view,
   lane-choice, edit-cut, edit-edge, edit-words, edit-step, edit-verbs), Sync/B-roll, zoom reza,
   AI pokrivalice, export prozor; S-Story je ista forma nad `story-draft-<n>.db`.
4. **Ingest forma** (Story grana): preglednik lokacija, Select više izvora, uvoz; nad istim
   tray lancem (isti radnik, iste trake).
5. **Export** (`qnc-program-render`, `qnc-unit-join`, `qnc-fcp7-xml`, prerender).
6. MA Video ostaje prazna forma dok ne dobije sadržaj (odluka korisnika u QNC-AI-ju).

Svaki korak: kopija kocke iz QNC-shared/QNC-AI, rastavljena po prijemnom popisu (AGENTS §4), bez
GUI-ja u logici, bez pravila u orkestratoru, undo kroz bazu, testovi bez stvarnog projekta,
conformance, commit.
