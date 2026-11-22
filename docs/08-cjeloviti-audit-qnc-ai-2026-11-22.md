# Cjeloviti audit QNC-AI i usporedba s QNC-pro (22. 11. 2026.)

Cilj: QNC-pro mora imati sve funkcije QNC-AI-ja. Kod se ne mora kopirati, a redoslijed rada
odabire se tako da posao bude gotov što prije. Ovaj dokument navodi sve što QNC-AI radi,
po područjima, s pravilom i mjestom u kodu. Uz svaku funkciju piše stanje u QNC-pro.
Na kraju je plan rada.

## 0. Kako je rađeno

- QNC-AI (15 aplikacija, 28 alata, 116 vlastitih kocki) i QNC-shared (254 kocke) samo su
  čitani. Ništa nije građeno ni pokretano.
- Pet pregleda je išlo usporedno:
  1. uvoz i pozadinski radnik;
  2. prijepis i AI;
  3. projekti, postavke, administracija i Ingest forma;
  4. player, preview, timeline, prerender i export;
  5. forme za montažu (AI Media Assist, MA Script, MA Audio, MA Video, S-Story, Story).
- QNC-pro (4 aplikacije, 2 alata, 127 kocki) uspoređen je prema popisu kocki i prema
  provjerama uživo od 21. 11.
- Oznake stanja u QNC-pro: **IMA**, **DIO** (postoji, ali ne cijela funkcija ili ne po
  pravilu QNC-AI-ja), **NEMA**.
- Kratice mjesta:
  - AI = `QNC-AI/crates`
  - SH = `QNC-shared/crates`
  - W = `QNC-AI/tools/qnc-ingest-worker`

## 1. Kartica, tray i Ingest baza

| Funkcija | Pravilo u QNC-AI | Mjesto | QNC-pro |
|---|---|---|---|
| Spojene kartice | Provjera svake 2 s. Kamera se iz profila pročita jednom po serijskom broju. | AI/qnc-attached-cards | IMA (qnc-card-watch) |
| Ingest baza | Projekt čije ime počinje s „Ingest". U mapi baza, uvijek kopira original. Ime je `Ingest <kamera> <datum vrijeme> <hex>`. | AI/qnc-ingest-base `make` | IMA (qnc-project-make) |
| Poznata kartica | Najnovija baza s klipovima te kartice daje `had`/`new`/`unfinished`. Bez novih klipova kartica se samo prijavi. S novima uvoz ide u istu bazu ili dobiva link. | qnc-ingest-base `known_card*`, `link_active_to`, `not_taken` | IMA (qnc-known-card), link baza NEMA |
| Evidencija kartice | Serijski broj, oznaka, kamera i vrijeme uvoza. | (QNC-AI to drži u izvorima i bazi) | IMA (qnc-ingest-cards) |
| Indeks kartice u bazi | QNCCARD.JSON, zapis po klipu i MHL s XXH64. Baza se tako može čitati kao kartica. | qnc-ingest-base/card_index.rs | NEMA |
| Baze kao izvor | Svaka podmapa mape baza s `project.db` postaje izvor `project-<id>`. | SH/qnc-source-bindings `with_card_bases` | NEMA |
| AutoSelect | Disk se otvori po serijskom broju (do 10 s) i radi se Select. Kartica izvađena pa vraćena dobiva ponovni Select. | AI/qnc-card-autoselect | IMA (qnc-card-select) |
| Uvoz iz mape | Bira se mapa, a oznaka mape se pamti. | AI/qnc-card-ingest `pick_folder`, `add_source` | NEMA |
| Izbacivanje kartice | Sustavnim putem: zaključavanje, odspajanje, izbacivanje medija, pa izbacivanje čitača. Kartica ide van kad je kopija gotova, a AI radi dalje. Pravilo ovisi o `storage.ingest_media`. | AI/qnc-card-eject, apps/qnc-ai-card-ingest/eject.rs | DIO: QNC-pro odlučuje po `playback.input`, pa kartica uz proxy ostaje unutra |
| Nakon kopije | Otvara se forma AI Media Assist (Breaking news) i prozor se zatvara. | qnc-ingest-orchestrator, tray | DIO: forma ne postoji |
| Orkestrator | Katalog `catalogs/ingest/orchestrators.json` (breaking_news, active_project, card_base) i izbor računala `data/ingest-orchestrators.json`. | AI/qnc-ingest-orchestrator | DIO (qnc-mode, qnc-tray-work) |
| Način traya | `data/tray-mode.json`: Local ili Net, Breaking news ili Story. | AI/qnc-tray-mode | IMA (qnc-mode) |
| Net način | Uvoz i AI na mrežnom čvoru (Jetson NX 192.168.100.28). | (planirano) | NEMA |
| Početak priče iz Ingesta | Nastavi / Novi / Odustani. Novi uzima bazu za priču (`story_ingest_bases`, `story_clips`). | AI/qnc-story-start, qnc-ingest-base `take_for_story` | NEMA |
| Brisanje baze | Dopušteno administratoru ili vlasniku iz `project_origin`. Teče: `mark_closing`, zatvaranje projekta, `forget` (do 60 pokušaja). | apps/qnc-ai-admin | NEMA |

## 2. Select i zapisi klipova

| Funkcija | Pravilo | Mjesto | QNC-pro |
|---|---|---|---|
| Sken kartice | Traži se od izabrane mape prema korijenu. Ako kartice nema, svaka datoteka je klip. Nestali klipovi brišu se samo nakon potpunog skena. | SH/qnc-ingest-select scan.rs | DIO (Sony XAVC) |
| Izvorni indeks | Upisuje se u komadima od 32. Zamijenjena datoteka dobiva novi zapis. | SH/qnc-source-index-db | DIO |
| Zapis iz XML-a | Declared: objavi se odmah, a probe radi radnik poslije. | SH/qnc-camera-*, qnc-ingest-select metadata.rs | IMA (qnc-sony-clip, qnc-record-compose) |
| Jedan probe | Samo za ono što nedostaje. Proxy se probe-a samo kad ga projekt koristi. 6 niti, plus jedna nit za klip koji preview traži. Prekinut probe ponavlja se do 3 puta. | SH/qnc-record-completion | DIO (qnc-probe; nema niti za traženi klip) |
| Filtar formata | Format koji nijedan dekoder ne izjavljuje preskače se uz upozorenje. | SH/qnc-decoder-catalog `refusal` | NEMA |
| Posteri | Poster s kartice se kopira, inače se uzme prva slika. Za LAN/MP4 poster ide kroz tok bajtova. | SH/qnc-missing-posters | DIO (samo lokalno) |

## 3. Uvoz (kopija)

| Funkcija | Pravilo | Mjesto | QNC-pro |
|---|---|---|---|
| Red uvoza | `queued` → `processing` (najam 120 s) → `imported` ili `failed`. Redoslijed uzimanja: dan DESC, trajanje DESC. | SH/qnc-content-store `ClaimNext` | IMA (qnc-ingest-run) |
| Kopija | Komadi od 32 MB bez sistemskog cachea. Zastoj od 60 s je greška. Promijenjen izvor je greška. Kopija se ponovno pročita i usporedi XXH64. | AI/qnc-ingest-import-worker copy.rs | IMA (qnc-copy, qnc-checksum); provjeriti 60 s i promjenu izvora |
| Odredište | `<projekt>/<original\|proxy>/<putanja na kartici>`. | copy.rs `card_path` | IMA |
| Načini | Link, Proxy, Original, Optimized(+original) po postavkama projekta. | plan.rs `action_for` | DIO (samo Original) |
| Optimizirana kopija | H.264 8-bit, provjera broja slika, fps-a i TC-a. | optimize.rs | NEMA |
| Završen klip | Ne kopira se ponovno. Završen zapis se ne prepisuje nezavršenim. | qnc-catalog | IMA (popravljeno 21. 11.) |
| Poster kod uvoza | Ostaje ako već postoji, inače kopija s kartice, inače prva slika. | copy.rs `import_poster` | DIO |

## 4. Pozadinski rad (radnik i kirurg)

| Funkcija | Pravilo | Mjesto | QNC-pro |
|---|---|---|---|
| Radnik kao program | Zaseban proces BELOW_NORMAL. Najam `worker_lease` 10 s. Budi ga `qnc-worker-wake`. Petlja traje dok čeka uvoz ili zapisi. | W/main.rs, run.rs | NEMA: koraci rade u procesu prozora uvoza |
| Plan koraka | `artifacts.order`, etape. Etapa n kreće kad je svaki korak etape n−1 odradio prvi krug. | SH/qnc-ingest-plan | IMA (qnc-ingest-plan) |
| Kirurg | Svake 2 s dijeli niti procesora koracima, prema izmjerenoj brzini. | AI/qnc-ingest-conductor | NEMA |
| Čekanja (Breaking news / level) | Breaking: val i prijepis ne čekaju ništa. Level: AI čeka da uvoz, valovi, filmstrip i prijepis završe. | W/run.rs held | DIO (redoslijed u seedu) |
| Prednost playera | Dok player svira, svaki korak radi samo klip koji se pušta. | SH/qnc-playback-activity `background_cap` | NEMA |
| Zaustavljanje X-om | Ključ `stopped_work`. | SH/qnc-stopped-work | IMA |
| Napredak | Brojevi po traci, `complete`, `ready_for_qnc`. | SH/qnc-work-progress | IMA (qnc-progress, qnc-work-bars) |

## 5. Val i filmstrip

| Funkcija | Pravilo | Mjesto | QNC-pro |
|---|---|---|---|
| Val | Iz originala (kod nas iz kopije), svi kanali. MXF PCM ide kroz zvučne elemente, MP4 AAC kroz indeks. Glasnoća se mjeri po traci. Prvi kanal ide slušačima izjava. | SH/qnc-wave-worker | IMA (qnc-wave-run, qnc-wave-make) |
| Broj čitača | Test medija s 1/2/4/8 čitača, uzima se najmanji broj unutar 95 % najboljeg. | SH/qnc-read-pace | NEMA (iz kopije nije bitno) |
| Filmstrip | 13 dijelova, najbliža intra slika kroz MXF indeks, jednim pozivom dekodera. Za klip do 10 s RandomSeek. | SH/qnc-filmstrip-worker | IMA (qnc-filmstrip-*) |
| Klip koji preview traži | Ide prvi. | qnc-background-jobs `prefer` | NEMA |

## 6. Prijepis i AI

| Funkcija | Pravilo | Mjesto | QNC-pro |
|---|---|---|---|
| Pravilo izjave | Klip do 20 s uzima se cijeli, inače najglasnijih 15 s. Riječi pokrivaju ≥ 0,3, pa se prepisuje cijeli klip. | qnc-transcribe-run `hear_statement` | IMA (qnc-statement-rule) |
| Red prijepisa | Dijelovi od 60 s nakon valova, najam `transcribe_lease`. Radnika ima Σparallel+1, najkraći čeka prvi, djelomični zapis ostaje. | SH/qnc-transcribe-queue | IMA (qnc-transcribe-run) |
| Motor | Parakeet na procesoru, 2 niti, parallel 3. | catalogs/ai | IMA |
| Komadi | 30 s, prag šuma +6 dB (−66..−42), preklop 2 s, odbacivanje ponovljene riječi (25 slika). | qnc-transcribe-run pieces | IMA |
| Fini prolaz HR | wav2vec2 za dijelove koje je čuo samo Parakeet. | AI/qnc-transcript `fine_pass` | NEMA |
| Prijepis onoga što svira | Komadi od 5 s, 12 s unaprijed. | `stream_played` | NEMA |
| Ponovni prijepis HR | IN-OUT ide u red s motorom Best. | `transcribe_repass` | NEMA |
| Vremena slova | wav2vec2 poravnanje. | SH/qnc-letter-times | DIO (qnc-letters) |
| Riječi u zoomu | Iz prijepisa. Nepokriveni dio prepisuje se jednom, ±5 s. | AI/qnc-zoom-words | NEMA |
| Analiza kadrova | Do 2 intra slike u sekundi, 160×90, rezovi, vrsta kadra, upotrebljivi dijelovi. 13 sličica ide filmstripu. | SH/qnc-shot-analysis, qnc-shot-detect | NEMA |
| Sadržaj kadra | SigLIP 2: poster, pa brzi, pa fini prolaz. Izjava ostaje cijela. | SH/qnc-shot-content-run | NEMA |
| Opis kadra | Florence-2 (DirectML), u bazi računala ili na AI serveru. | SH/qnc-shot-caption-run | NEMA |
| AI pokrivalice | Grupe, primjeri iz ispravaka i opisa, prijedlog za slot i seriju OFF-a, prijevod HR→EN. | SH/qnc-ai-broll, qnc-broll-suggest, qnc-translate | NEMA |
| Provjera čitanja OFF-a | Usporedba teksta i onoga što je čuto. | SH/qnc-read-check, qnc-take-heard | NEMA |
| AI mogućnosti projekta | Popis, ovisnosti, predlošci, motori, AI server s TLS-om, spremište, paket. | SH/qnc-ai-features, -engines, -place, -tls, -store, tools/qnc-ai-server, -pack | DIO (qnc-ai-catalog) |
| Glasnoća i pomoćnik | EBU R 128. NORM. AI izjednačavanje govora i ducking A2. | SH/qnc-loudness, qnc-sound-assist | DIO (qnc-loudness) |

## 7. Projekti, postavke, administracija

| Funkcija | Pravilo | Mjesto | QNC-pro |
|---|---|---|---|
| Registar i projekt | `data/qnc-projects.db`, stvaranje projekta, podmape, zaštita mapa. | AI/qnc-project-store | IMA (qnc-registry, qnc-project-*) |
| Predlošci | Sistemski se ne brišu, korisnički `tpl_user_*`. | qnc-project-store | IMA (qnc-project-templates) |
| Advanced postavke | AI, lokacije, ulaz, zvuk, glasnoća, prijepis, sučelje, tipke, export, izbor aplikacija. | qnc-project | DIO (qnc-settings-*) |
| Obrazac priloga | Listovi, uređivač, pretraga projekata po vrijednostima. | AI/qnc-newsroom-sheet* | IMA (qnc-newsroom-sheet*) |
| Aplikacija Projekti | Projekti korisnika ovog računala. Klik otvara AI Media Assist. „+ Novi projekt" ide iz baze. | apps/qnc-ai-projects | DIO (qnc-projects) |
| Administracija baza | Tablica baza, brisanje s provjerom vlasnika. | apps/qnc-ai-admin | NEMA |
| Pitanje preko forme | Program `qnc-ai-ask`. | apps/qnc-ai-ask | DIO (qnc-confirm) |
| Shell | Footer s aplikacijama projekta, Close project, tema, link u statusu. | AI/qnc-desktop | NEMA |

## 8. Ingest forma

| Funkcija | Pravilo | Mjesto | QNC-pro |
|---|---|---|---|
| Izvori | Računalo / LAN / Internet / Baze, preglednik mapa, Odustani / Odaberi. | AI/qnc-ingest-* forma | NEMA |
| Kartice | Kvačica, Odaberi sve, Očisti, filtar Novi/Sve, preview i source timeline. | — | NEMA |
| Uvezi | U bazi tok priče, inače provjera prema projektu, budi radnika i otvara AI Media Assist. | qnc-ingest-import-run | NEMA |

## 9. Forme za montažu

| Forma | Bitno | QNC-pro |
|---|---|---|
| e AI Media Assist | Tabovi Sve i Prijepis, stupac Izjave, Transcribe, Ponovo HR, Audio AI i Video AI, pretraga i Kopiraj, Dodaj izjavu i Ažuriraj izjavu, potpis izjave. | NEMA |
| g MA Script | Panel pisanja preko cijele forme, prozor medija, Snimi prilog, Na pregled, Nacrt XML, Pošalji u S-Story. | NEMA |
| h MA Audio | Panel scenarija, snimanje OFF-a i čitanja, složena izjava. | NEMA |
| l MA Video | Desni panel prazan. | NEMA |
| k S-Story | Verzije priče u `story-draft-<n>.db`. Svaki klip je slot. Pošalji u Story. | NEMA |
| o Story | Panel Segmenti, Wrap, M markeri, slotovi, pokrivalice, Sync/B-roll, J/L, krivulje, NORM, AI tona, zoom, Undo i Undo izabranog, Export HI-res. | NEMA |
| Zajednički okvir | Ploča, pool, fokus i Tab krug, desni paneli, AI vrata, crvena traka odbijanja. | DIO (qnc-board, qnc-key-row) |
| Baza priče | Jedna transakcija po izmjeni, pravilo „bez rupe", undo 100 koraka. | NEMA (SH/qnc-program-db se može uzeti) |

## 10. Player, preview, timeline, zoom

| Funkcija | Pravilo | Mjesto | QNC-pro |
|---|---|---|---|
| Proces playera | `--monitor-host`, jedna sesija za drugom, TCP naredbe, frame-mapa. | tools/qnc-player-runner | DIO (qnc-player-session, -wire, frame-map) |
| Sat i transport | Ready tek kad je buffer spreman. Play nikad ne otvara medij. Kraj raspona vraća na početak. | SH/qnc-broadcast-player | DIO |
| Program (priča) | Ravna playlista, trake zvuka, dekoder idućeg reza 2 s unaprijed, dijelovi prerendera. | SH/qnc-broadcast-engine program.rs | NEMA |
| Zvuk | Rute, razine, krivulje, dio razine, Sync bed i preslušavanje u jednom pozivu uređaja. | SH/qnc-audio-output | DIO |
| Preview | Otvaranje na slici, korak, cue, javljanje razine playera, ponovni pokušaj kad zapis nije gotov. | AI/qnc-source-preview | IMA (qnc-preview) |
| Source timeline | IN/OUT, Ctrl+I/O, škare, A1/A2 trake, krivulja razine, AI tona, prozor izjave, J/K/L, Shift+K. | AI/qnc-source-timeline, SH/qnc-timeline | DIO (qnc-source-timeline, qnc-marks, qnc-timeline-panel) |
| Zoom reza | Slike, fini val, riječi i slova, Poništi / Spremi / Zatvori. | AI/qnc-trim-stills, SH/qnc-trim-overlay | DIO (qnc-zoom) |
| HDMI izlaz | Proces na vanjskom ekranu, vblank, pokazivač ne prelazi na taj ekran. | SH/qnc-program-output, tools/qnc-program-output-host | NEMA |
| Miš i tipke | Mapa miša, katalog tipki po opsegu panela. | SH/qnc-mouse-map, qnc-key-intents | DIO (qnc-mouse-map, qnc-actions) |

## 11. Prerender i export

| Funkcija | Pravilo | Mjesto | QNC-pro |
|---|---|---|---|
| Prerender | Dijelovi priče u formatu predaje, otisak, najam, čekanje playera. | tools/qnc-prerender, SH/qnc-program-render | NEMA |
| Export | XML (FCP7), Express iz prerendera, Original/postavke iz originala, spajanje uz glasnoću i limiter, start TC. | tools/qnc-export, SH/qnc-unit-join, qnc-fcp7-xml | NEMA |
| Izbor exporta | XML, Express, PAL/NTSC, grupe. | SH/qnc-export-choice | DIO (qnc-export-preset) |
| Dekoderi | Katalog, libav u procesu, čitanje unaprijed s rokom 10 s, program poslova. | SH/qnc-decoder-catalog, AI/qnc-libav-decode | DIO (ffmpeg uz aplikaciju) |

## 12. Greške QNC-AI-ja koje se ne prenose (popraviti u QNC-pro)

1. Breaking news ne radi sa sistemskim predloškom, jer traži izbor aplikacija koji seed nema.
2. Brisanje projekta briše mapu prije nego se izmjena potvrdi u bazi.
3. Svako otvaranje spremišta prepiše seed i ponovno zaključa sve mape.
4. Ime „Ingest…" pretvara projekt u bazu, jer se baza razlikuje samo po imenu.
5. Filmstrip i val klipa s greškom ponavljaju se bez kraja (do roka od 1 sat).
6. Kopija i zapisi čekaju i dok korisnik samo montira, a ne samo dok svira.
7. U Breaking newsu AI kadrovi stoje dok prijepis ne završi.
8. Otisak prerendera uključuje cijeli blok exporta, pa svaka promjena exporta briše sve dijelove.
9. d3d11va je ispred libava, pa proxyji idu kroz ffmpeg proces.
10. Reprodukcija s LAN-a i intraneta ne radi.
11. XML export pada cijeli ako ijedan klip nije na ovom računalu.
12. Prozor izjave je 1/13 sa svake strane, a dogovor je 70/15/15.
13. Ponovni pokušaj za nedovršen zapis nikad ne budi radnika i gubi traženu sliku.
14. Enter na source timelineu s uzetim IN/OUT vraća oznaku na prvi položaj umjesto da je spremi.
15. Klik i izbor slota tiho bacaju ono što čeka Enter.
16. Ime zapisa glasa `<projekt>_<oznaka>` nije jedinstveno: drugi OFF može prepisati `_T1`.
17. Nacrt XML i Pošalji u S-Story slažu različit kostur, jer nesnimljeni OFF u jednom ostaje kao pauza, a u drugom se izostavlja.
18. Sve poruke panela scenarija su crvene, pa i uspjeh.
19. ingest-base privremeno mijenja korijen projekata u registru, što nije atomarno.
20. Val kod projekta koji kopira samo proxy i dalje čita original s kartice.

## 13. Mrtvi kod koji se ne prenosi

- Player: načini rada `--native-output` i `--monitor-output`, slika kroz socket, naredbe
  Load/Preload/SetActive/Unload/SetPlaybackRequest.
- Uvoz: `run_service`, `drain`, `import_clip*`, brojač kopiranih bajtova.
- Tray: `StartAsk`, `take_skipped`. Gruba staza vala. `ingest_base_only_new` se u radu nikad
  ne piše.
- Forme: gumbi B-roll, Voice over i Talking head bez radnje (g, h, l); red prijepisa koji
  nitko ne crta; `transcript_add_quote` nedostupan iz sučelja; radnje složene izjave bez
  gumba; `qnc-wizard-flow`, `qnc-wizard-db`.

## 14. Što treba odluku korisnika

1. Pravilo izbacivanja kartice: QNC-AI odlučuje po `storage.ingest_media` (Original ili
   Optimized+original), a QNC-pro po onome što se pušta. Prijedlog: kao QNC-AI, kartica van
   čim su kopirani svi originali.
2. Prozor izjave: 70/15/15 (dogovor) ili 1/13 (kako QNC-AI danas radi).
3. Enter na source timelineu s uzetom oznakom: sprema (prijedlog) ili vraća (kako QNC-AI radi).
4. Kodek za Original export: XAVC 4:2:2 10-bit LGPL nema, pa ostaje XDCAM HD422 kao u QNC-AI-ju.

## 15. Plan (redoslijed za brzinu)

Svaki korak je jedan commit, s testom i conformanceom. Provjera uživo je s pravom karticom,
koju ubacuje korisnik.

1. **Kartica van po pravilu QNC-AI-ja** (odluka 14.1) i **radnik kao zaseban program**
   (najam, buđenje, petlja), da AI ide dalje kad se prozor zatvori. Uz to ide
   **prednost playera** (`background_cap`).
2. **Kirurg i dva načina čekanja** (Breaking news / level), bez grešaka 12.5–12.7.
3. **AI koraci**: analiza kadrova (intra slike u istom prolazu s filmstripom), sadržaj (SigLIP 2),
   opis (Florence-2), `ai_takes`.
4. **AI Media Assist (e)** na zajedničkom okviru forme: pool, stupac Izjave, prijepis s
   pretragom, Video AI, Dodaj i Ažuriraj izjavu. Breaking news ga tada otvara nakon kopije.
5. **Ingest forma** (izvori, kartice, Uvezi), **baze kao izvor**, **indeks kartice**,
   **početak priče**, **administracija baza**.
6. **MA Script / MA Audio**: scenarij, snimanje OFF-a, čitanja, složena izjava.
7. **Baza priče, S-Story i Story**: program u playeru, Wrap, pokrivalice, Sync, J/L, krivulje,
   NORM, undo.
8. **Prerender i export** (XML, Express, Original), s otiskom bez nebitnih postavki.
9. **Ostalo**: fini prolaz HR, prijepis onoga što svira, vremena slova u zoomu, provjera čitanja,
   AI pokrivalice, HDMI izlaz, shell, LAN/Net način (Jetson NX).
