# Audit lanca traya: QNC-AI i QNC-shared (2026-11-21)

Što QNC-AI tray stvarno radi i pokreće, kocka po kocku, iz koda (ne iz pamćenja). Mjera:
`cargo metadata` zatvarač ovisnosti triju programa `qnc-ai-tray`, `qnc-ai-card-ingest` i
`qnc-ingest-worker` (samo runtime ovisnosti, bez dev).

## 1. Brojke

| | kocki | redaka |
|---|---|---|
| lanac ukupno | 236 | 116 590 |
| iz QNC-AI | 60 | 14 616 |
| iz QNC-shared | 176 | 101 974 |
| s GUI bibliotekom (egui) u lancu bez prozora | 36 | ~12 000 |

Lanac bez ijednog prozora (radnik) povlači player (`qnc-player-*`, `qnc-source-preview` 1507,
`qnc-source-timeline` 1034, `qnc-timeline` 3124 s egui-jem), priču (`qnc-program-db` 7403,
`qnc-program-playlist` 2596, `qnc-program-input`), spremište sadržaja (`qnc-content-store` 3206)
i cijeli Ingest bez forme (`qnc-ingest-application` 2659, `qnc-ingest-select` 2472,
`qnc-ingest-import-worker` 1879). Uzrok: prozor kartice vozi `qnc-ingest-application` kao da je
korisnik (Select, Select all, Uvezi), a radnik prijepis čita kroz ulaz playera
(`qnc-still-locator` -> `qnc-player-input`).

## 2. Što tray pokreće (procesi)

1. `qnc-ai-tray` (504 + `qnc-tray` 980): ikona, izbornik, upozorenja, IPC, jedan primjerak,
   dijalozi. Gleda diskove svake 2 s, kartica se "slegne" 5 s, katalog kamera svakih 60 s.
2. Po kartici `qnc-ai-card-ingest` (688 + `qnc-card-ingest` 430): Select kartice, Select all,
   Uvezi, poznata kartica (veza na raniju bazu), eject, pitanje "Pokreni QNC".
3. `qnc-ingest-worker` (1338; 37 qnc ovisnosti, najviše u QNC-AI-ju): plan templatea
   `artifacts.order` (`qnc-ingest-plan` 170), nit po koraku s krugom od 1 s, dirigent
   (`qnc-ingest-conductor` 238) dijeli procesor, pauza dok player svira
   (`qnc-playback-activity`), najam u bazi.
4. Programi radnika: `ffprobe`, `ffmpeg`, `qnc-decoder-ffmpeg`, `qnc-transcribe-parakeet(-dml)`,
   `qnc-shot-content-siglip`, `qnc-shot-caption-florence`; AI dodaci kroz `qnc-plugin-process`.

## 3. Koraci radnika i njihove kocke

| korak | što radi | kocke (redaka) | QNC-pro ima |
|---|---|---|---|
| records | probe zapisa kojima nešto fali | record-completion 584, record-probe 237, media-probe 722, ffprobe-metadata 816, media-metadata-compose 488, media-record-db 1807, media-records 635 | **da**: probe u istom prolazu s kopijom (`qnc-ingest-run`, `qnc-probe`, `qnc-ffprobe-read`, `qnc-record-compose`) |
| posters | poster s kartice ili prva slika | missing-posters 377, poster-create 117, media-thumbnail 333 | poster s kartice da; prva slika ne |
| poster_look | SigLIP nad posterima | shot-content-run 568, shot-content 166, shot-content-db 384, shot-labels 207, shot-vocabulary 127, ai-catalog 588, plugin-process 532 | ne |
| waves | val svakog klipa (grubi pa puni), glasnoća | wave-worker 1023, wave 972, mxf-sound 420, mp4-sound 550, mxf-index 422, read-pace 160, loudness 313, background-jobs 468, artifact-db 742, timeline-artifacts 190, content-artifacts 643 | `qnc-wave-read` (čitanje), ne pisanje |
| filmstrips | 13 slika klipa | filmstrip-worker 1337, filmstrip 595, decoder-catalog 1008, ffmpeg-decode 1393, media-decode 1657 | ne |
| import | kopija ili link po postavkama | ingest-import-worker 1879, import-fit 136, copy u `qnc-media-stream` 1283 | **da**: `qnc-ingest-run` + `qnc-copy` |
| transcripts | prijepis CH1 (Parakeet) u komadima | transcribe-run 766, transcribe-queue 335, transcribe-jobs 681, transcript-db 1218, sound-range 147, still-locator 182 (+ player-input 1678) | ne |
| shots | rezovi i mirni dijelovi | shot-analysis 478, shot-detect 586, shot-db 393 | ne |
| content | SigLIP po kadru | kao poster_look | ne |
| captions | Florence opis po kadru | shot-caption-run 137, shot-caption 94, shot-captions 143 | ne |

Prikaz: `qnc-work-progress` 301 (čita bazu svake sekunde, traka po koraku, X u
`qnc-stopped-work` 57), `qnc-work-progress-view` 128, `qnc-work-info` 121.

## 4. Što je u QNC-AI-ju krivo (iz koda, uz audit 04)

- Kartica se čita **četiri puta** po klipu: kopija (`qnc-media-stream`), probe (`ffprobe` nad
  kopijom), val (`qnc-mxf-sound` prolaz po zvučnim elementima), filmstrip (ffmpeg po indeksu
  MXF-a); SigLIP i Florence još jednom čitaju postere.
- Koraci rade u krugovima po 1 s s vlastitim nitima, dirigent dijeli mjesta; kraj se prepoznaje
  "dvaput ništa za raditi". Radnik je zato spor na početku i na kraju i teško ga je pratiti.
- "Gotovo/kopirano" formula na 4 mjesta, napredak se broji i u bazi i u memoriji
  (`qnc-done-count`) pa se usklađuje svakih 30 s.
- Pravila su u aplikacijama i radniku (`main.rs`, `run.rs` 459 redaka s `if` nad stanjem).
- Radnik ovisi o Ingest aplikaciji, playeru i priči iako ih ne treba.

## 5. Odluka za QNC-pro (brže i jednostavnije, ista procedura)

Procedura ostaje QNC-AI-jeva (isti koraci, isti redoslijed po načinu rada, iste trake s X-om,
isti programi AI-ja), a izvedba:

1. **Jedan prolaz po klipu**: dok `qnc-copy` kopira klip s kartice, isti bajtovi daju checksum,
   poster (prva ključna slika), zvučne elemente za val i glasnoću te 13 ključnih slika za
   filmstrip (`qnc-mxf-sound::read_sound_and_pictures` već to zna za MXF; MP4 kroz
   `qnc-mp4-sound`). Probe ide nad kopijom kao i sad. Kartica se čita jednom.
2. **Lanac po klipu, ne po koraku**: klip prolazi kopija -> probe -> val -> filmstrip ->
   prijepis -> AI, svaki na svojoj niti s redom; nema krugova od 1 s ni dirigenta. Redoslijed
   po načinu rada (Breaking news: valovi prvi, pa kadrovi i filmstrip, pa SigLIP, pa prijepis i
   opisi) je redoslijed redova, zapisan kao plan u templateu (`artifacts.order`) kao do sada.
3. **Istina je baza**: svaki korak upisuje svoj rezultat kroz vlasnika tablice; prozor i tray
   čitaju trake iz baze (kao `qnc-work-progress`), X je zapis u bazi (`stopped_work`).
4. Radnik je zaseban program po projektu (najam u bazi), bez ovisnosti o Ingest aplikaciji,
   playeru i priči.

Redoslijed prijenosa (svaki korak: kopija kocke iz QNC-shared/QNC-AI, rastavljena po
prijemnom popisu, testovi, conformance, commit):

1. Plan koraka, X u bazi, trake iz baze, radnik kao program s korakom uvoza (kopija + probe +
   poster) i prozor spojen na trake.
2. Val i glasnoća iz prolaza kopije; artefakti u bazi (`qnc-artifact-db`).
3. Filmstrip iz ključnih slika istog prolaza.
4. Prijepis (Parakeet program, red u bazi).
5. AI kadrovi, SigLIP, Florence (AI katalog, dodaci, paket).
6. Poznata kartica, eject, "Pokreni QNC".
