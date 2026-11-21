# Dubinski audit lanca uvoza: QNC-AI prema QNC-pro (2026-11-21, noć)

Zašto: korisnik je uočio da kartica G: uvezena četiri puta zaredom nije prepoznata kao poznata
("qnc-ai vodi evidenciju serijskih brojeva kartica i klipova"), iako je to pravilo bilo u mom
popisu od popodneva. Ovaj dokument je popis **svake** funkcije lanca QNC-AI traya, iz koda,
s mjestom i pravilom, stanje u QNC-pro i plan po kojem se radi dalje. Sve što nije u planu ne
radi se; svaki korak plana je commit.

Izvori (QNC-AI, čitano u cijelosti): `apps/qnc-ai-tray/src/main.rs` (526),
`apps/qnc-ai-card-ingest/src/main.rs` (584) i `eject.rs` (135), `crates/qnc-card-ingest`,
`crates/qnc-attached-cards`, `crates/qnc-ingest-base/src/lib.rs` (1139) i `card_index.rs`,
`crates/qnc-ingest-orchestrator`, `crates/qnc-tray-mode`, `tools/qnc-ingest-worker/src/{main,run,transcribe}.rs`,
`crates/qnc-transcribe-run`, `crates/qnc-work-progress`.

## 1. Tray (`qnc-ai-tray`)

| # | Funkcija | QNC-AI (mjesto, pravilo) | QNC-pro |
|---|---|---|---|
| T1 | Jedan primjerak traya | `OneCopy::take("QNC-AI-tray")` main.rs:73 | ima (`qnc-tray-run`) |
| T2 | Posao procesa: sve što tray pokrene umire s njim | `qnc_process_job::Job` :81 | ima (`qnc-process-job`) |
| T3 | Način rada računala (Breaking news / Story / Net), kvačica u izborniku, zapis na računalu | `qnc_tray_mode` :90, menu :46 | ima (`qnc-mode`, baza računala) |
| T4 | Autoplay stražar: za karticu poznate kamere sustav ne otvara Explorer | `qnc_autoplay_guard::guard` :105 | ima |
| T5 | Profili kamera za ime kamere po modelu | `read_profiles` :116 | ima |
| T6 | Gledanje diskova svake 2 s, katalog kamera čita se ponovno svake minute kad ga nema | LOOK :28, CATALOG_AGAIN :30, :260 | ima (gledanje); katalog je ugrađen u profile |
| T7 | Smirivanje kartice 5 s prije starta (čitač se budi) | SETTLE :32, settling :286 | ima (`qnc-card-watch` settle) |
| T8 | Nečitljiva kartica pokušava se 5 puta, pa obavijest | READ_TRIES :34, :348 | **provjeriti** (`qnc-card-watch` Seen::Unreadable?) |
| T9 | Kartica koju neki prozor već uvozi ne otvara se opet | `runs_card` :305 | ima (`runs_card`) |
| T10 | **Poznata kartica**: originali kartice uspoređuju se s klipovima svih ranijih baza (po izvornom URI-ju kartice + URI-ju originala); nema novih → samo obavijest, ništa se ne uvozi; nedovršena (odabrano, ne uvezeno) → nastavlja u toj bazi (`--goes-on`, resume_all); ima novih → nova baza koja uvozi **samo nove** (`new_with_only_new`, `not_taken`), vezana na raniju (`link_active_to`); baza koja se briše → kartica se gleda ponovno poslije | :308-335, ingest-base :632-720, :730-760, card-ingest :281-300 | **NEMA** (uzrok četiri uvoza iste kartice) |
| T11 | Orkestrator (gdje uvoz piše) iz postavki; bez njega obavijest | `orchestrator()` :382 | nema (QNC-pro: uvijek `projects_root` iz registra; svjesno jednostavnije) |
| T12 | Nepoznati disk: samo log, obavijest osim pri prvom pogledu | :337 | ima |
| T13 | Kartica izvađena: smirivanje se briše | Found::Gone :360 | ima |
| T14 | Prekini sav rad: pitanje Zadrži / Obriši / Odustani; Obriši briše baze od zadnjeg prekida kad procesi stanu (ili nakon 10 s) | ask_stop :507, to_delete :174 | ima |
| T15 | Izlaz dok radi posao: isto pitanje, pa izlaz | QUIT :212 | ima |
| T16 | Uvoz iz direktorija: pitanje nova baza ili aktivna; prozor s preglednikom; drugi primjerak prozora za istu bazu samo pošalje signal | INGEST_FOLDER :232, open_folder :372, card-ingest main :453-468 | **nema** |
| T17 | Administracija baza (zaseban program) | ADMIN :229, open_admin :460 | QNC-pro ima Projekte i Postavke (drugi oblik) |
| T18 | Otvori aplikaciju (samo Breaking news), kad već radi samo obavijest | OPEN_FORM :230, open_form :517 | ima (bez provjere "već radi") |
| T19 | Prozor po bazi, otvara se pomaknut (`--place n`) | open_window :430 | **nema** pomaka (svi prozori na istom mjestu) |
| T20 | Status u izborniku: uvozi n / u pozadini n / čeka karticu | status :65 | ima |

## 2. Prozor uvoza (`qnc-ai-card-ingest` + blok `qnc-card-ingest`)

| # | Funkcija | QNC-AI | QNC-pro |
|---|---|---|---|
| P1 | Jedan primjerac po bazi | OneCopy :461 | ima jedan prozor po kartici (serijski), ne po bazi |
| P2 | Prozor čita napredak iz baze projekta svaku sekundu; kad baza više nije aktivna (briše se) prozor se zatvara | card-ingest lib :218-245 | ima čitanje; **nema** zatvaranja kad projekt nestane |
| P3 | Select kartice (sve priključene kartice ovog prozora; `only_card`), Select all, uvoz radniku | lib :274-340 | ima (jedna kartica) |
| P4 | Klipovi koje ranija baza već ima isključuju se iz uvoza (`not_taken`) | lib :320-336 | **NEMA** (dio T10) |
| P5 | Redak po kartici: kamera, "već uvezena u bazi X", "n novih" | main :309-316 | nema (jedna kartica, bez "poznata") |
| P6 | Trake po funkciji iz plana projekta, X zaustavlja funkciju (u bazi), vrijeme svake | main :325-331, work-progress | ima |
| P7 | Broj izjava u prozoru (`progress.statements`) | main :332 | **nema** (ima u bazi, prozor ne pokazuje) |
| P8 | Broj neuspjelih crveno | :336 | ima |
| P9 | Naziv, mjesto, opis baze; zatvaranje traži naziv, drugi put ili nakon 30 s imenuje po kameri i vremenu | BaseInfo, :151-175, NAME_WAIT | ima polja; **nema** pravila imenovanja pri zatvaranju |
| P10 | Odustani → zaustavi sve funkcije koje rade, kartica van, pitanje Zadrži/Obriši; Obriši u zasebnom procesu (prozor drži bazu) | :365-406, delete_in_background | ima (brisanje u istom procesu nakon što posao stane) |
| P11 | **Izbacivanje kartice**: nakon potvrđenog Odustani; i nakon što je sve kopirano bez greške, a baza ništa ne drži na kartici (Original ili Optimized+original); pokušava se 40×0,5 s dok je disk u upotrebi; 10 s obavijest, prozor se zatvara; posao ide u pozadini | eject.rs | **NEMA** (`qnc-eject` kocka postoji, nitko je ne zove) |
| P12 | Breaking news: forma se otvara tek kad je **sve kopirano i kartica izbačena** (ili eject nije moguć / ima grešaka); jednom; baza postaje aktivni projekt | main :180-192 | djelomično: otvara se čim je kopirano (ne čeka eject), korak priče se zapisuje |
| P13 | Prozor naprijed kad je gotovo (`qnc-window-front`) | :231-237 | ima (`qnc-window-front`) |
| P14 | Kartični indeks baze (`QNCCARD.JSON`, zapisi, MHL s kontrolnim zbrojevima) kad je gotovo: QNC čita bazu kao karticu | :225-230, card_index.rs | **nema** (QNC-pro: QNC čita bazu projekta izravno; MHL nema) |
| P15 | `--goes-on`: zaustavljene funkcije se vraćaju (nastavak nedovršene kartice) | main :472 | **nema** (dio T10) |
| P16 | Look datoteka prozora, uređivač izgleda | FormLook | ima (`ingest-window.look.json`) |
| P17 | Uvoz iz direktorija / `--folder-uri` iz Ingest forme | main :452-485 | **nema** (T16) |

## 3. Pozadinski rad (`qnc-ingest-worker` + kocke)

| # | Funkcija | QNC-AI | QNC-pro |
|---|---|---|---|
| R1 | Radnik je zaseban program (preživi prozor), jedan po projektu (najam u bazi), staje kad projekt više nije aktivan ili se briše | run.rs :39-60, main.rs | **nema**: koraci su niti prozora; prozor zatvoren = rad stao |
| R2 | Plan koraka iz `artifacts.order` projekta, bez zadane vrijednosti | run.rs :44 | ima (`qnc-ingest-plan`) |
| R3 | X po funkciji u bazi (`stopped_work`), radnik preskače i zaustavlja | Stops | ima |
| R4 | Kopija: klip po klip, zapis u bazi čim je kopiran, probe nad kopijom, kontrolni zbroj | import-worker | ima (xxh64) |
| R5 | Dovršetak zapisa iz kopije | records | ima (u kopiranju) |
| R6 | Posteri s kartice, inače s početka klipa | posters | ima |
| R7 | Val: svi kanali originala iz kopije, glasnoća po kanalu, 1200 točaka; otvoreni klip prvi; sam na kartici | wave-worker | ima (po slici, ne 1200; sva 4 kanala, glasnoća) — bez "otvoreni klip prvi" (nema playera) |
| R8 | **Val i prijepis zajedno**: prvi kanal iz prolaza vala slušateljima | transcribe.rs Statements | ima (večeras) |
| R9 | Pravilo izjave: ≤20 s cijeli, dulji po najglasnijih 15 s, udio riječi ≥0,3 | transcribe-run :655-720 | ima (večeras) |
| R10 | Više motora istodobno (`parallel`, grafička + procesor) | installed_fast_transcribers | ima (parallel, samo procesor u katalogu) |
| R11 | Red prijepisa po 60 s za ono što val nije odlučio, tek nakon valova; NotFinal se ponavlja | transcribe.rs :44-100 | ima (bez NotFinal pravila: zapisi su gotovi u kopiji) |
| R12 | Komadi 30 s na tihom mjestu, 2 s preklopa, prag po klipu, razina govora, sekunda sa strane, spremanje djelomičnog prijepisa nakon svakog komada | transcribe-run | ima osim djelomičnog spremanja (#3 audita 8. 11.) |
| R13 | Fini prijepis (wav2vec2, hrvatski) popravlja Parakeetov, ne prepisuje | corrected_by_fine | **nema** |
| R14 | Filmstrip: 13 dijelova, najbliža intra kroz MXF indeks, jedan ffmpeg; MP4 kroz tablice uzoraka; bez indeksa seek | filmstrip-worker | ima (MXF indeks + seek; **MP4 indeks nema**: FX3A ide seekom) |
| R15 | Dirigent: prijepis drži sva mjesta, filmstrip jedno, val čeka; omjer gotovih klipova ravan | run.rs :95-125 | **nema** (tri koraka slobodno istodobno) |
| R16 | Redoslijed po načinu: Breaking news valovi > kadrovi (rade i filmstrip) > SigLIP > opisi, prijepis slobodan; Story/Net AI tek nakon kopije, prijepisa, valova i filmstripa | run.rs :136-165 | **nema** |
| R17 | Dok klip svira, sva pozadina stoji (`playback_level`) | :166 | nema (QNC-pro nema player) |
| R18 | AI kadrovi (rezovi, mirno/svenk, 1 slika/s pa svaka intra), poster kadra | shot-analysis | **nema** |
| R19 | SigLIP: poster kartice odmah nakon Selecta, pa kadrovi 1-3 slike; izjave se ne gledaju; grupe iz oznaka, rječnik, ispravci (baza računala / server) | shot-content-run | **nema** |
| R20 | Florence opisi kadrova (baza računala / server) | shot-caption-run | **nema** |
| R21 | Što AI uzima: izjava se ne gleda (osim ≥2 kadra), `ai_takes` | transcribe.rs :138-200 | **nema** |
| R22 | Kraj rada: svaka traka puna ili obustavljena | work-progress complete | ima (`counts.done`) |
| R23 | Čekanje na čitanje kartice (test medija, broj čitača) | read-pace | nema (QNC-pro čita kopiju, SSD) |

## 4. Nalazi iz večerašnjih proba (log 19:27–21:10)

- Ista kartica G: (serijski 67870d0c) uvezena 4×: nema T10. Svaki uvoz 16,8 GB → disk pun.
- Kartica D: (04739a66) pojavila se u 20:38 i 21:01; drugi put prozor je našao 0 klipova
  (prozor se zatvorio za 1,9 s, projekt ostao); uzrok neprovjeren (kartica izvađena tijekom
  smirivanja?). Treba T8 i zapis razloga u log.
- Nakon zatvaranja prozora rad staje (R1): u QNC-AI radnik je zaseban program.
- Prozor ne nestaje kad se projekt obriše (P2).

## 5. Plan (redoslijed, svaki korak commit, ništa izvan plana)

1. **T10 + P4 + P15: poznata kartica.** Kocka `qnc-known-card` (čista: originali kartice
   iz indeksa ↔ klipovi po `clip_sources` svih projekata registra, po izvornom URI-ju kartice i
   URI-ju originala): `Known { project, had, new, unfinished }`. Tray: nema novih → obavijest,
   bez projekta; nedovršena → nastavak u tom projektu (prozor s `--goes-on`, resume_all);
   ima novih → novi projekt uvozi samo nove (zapis `only_new_of` u bazi projekta, Select
   isključuje klipove koje raniji projekt ima); projekt koji se briše → kartica se gleda poslije.
   Prozor: redak "već uvezena u X, n novih". Test na kopiji baze.
2. **P11 + P12: izbacivanje kartice i otvaranje forme** po pravilu QNC-AI (eject nakon
   Odustani i nakon pune kopije kad projekt ništa ne drži na kartici; 10 s obavijest, prozor se
   zatvara, rad ide dalje; Breaking news forma tek nakon ejecta).
3. **R1: radnik kao zaseban program** (`qnc-pro-worker`): koraci vala, filmstripa, prijepisa i
   AI-ja idu u njega; prozor ga budi i čita napredak iz baze; najam u bazi; staje kad projekt
   nije aktivan. Prozor se zatvara kad projekt nestane (P2).
4. **R16 + R15: redoslijed po načinu i dirigent** (podatak po načinu, ne kod).
5. **R18–R21: AI koraci** (kadrovi iz svih intra slika u istom prolazu kao filmstrip, po
   izračunu večeras; SigLIP; Florence; `ai_takes`).
6. **T16 + P17: uvoz iz direktorija.** 7. **T8, T19, P7, P9, R12 djelomično spremanje, R14 MP4 indeks, R13 fini prijepis.**
7. Ne radi se (svjesna razlika): T11 orkestrator, P14 kartični indeks/MHL, T17 admin (QNC-pro
   ima Projekte i Postavke), R17/R23 dok nema playera.
