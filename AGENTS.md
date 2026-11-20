# QNC-pro - zakon repoa

QNC-pro je nova aplikacija, čisti početak po auditu od 2026-11-20
(`QNC-AI/docs/03-audit-kocke-2026-11-20.md`, `04-audit-cijela-aplikacija-2026-11-20.md`).
Ništa se ne kopira u cjelini. Kocka iz QNC-shared ili QNC-AI ulazi ovamo tek kad prođe
prijemni popis iz odjeljka 4. Ovaj dokument je obvezan za svaki plan, audit, implementaciju,
test i popravak.

## 1. Što aplikacija radi (dvije grane)

Tray (jedan po računalu, bez prozora) gleda spojene diskove. Kartica kamere koju QNC zna
pokreće uvoz. Kamo dalje, odlučuje način rada računala (zapis u bazi, ne u datoteci):

- **Breaking news** (laptop na terenu, brzi rad): tray -> uvoz -> **AI Media Assist** otvoren
  čim je kopija gotova (valovi prvi, AI kadrovi i filmstrip, SigLIP, prijepis i opisi u
  pozadini) -> MA Script -> S-Story -> Story -> Export.
- **Story** (veći izbor ulaznog materijala): tray -> uvoz -> čeka prijepis, valove i filmstrip
  -> **Ingest** (izbor klipova, više izvora) -> AI Media Assist -> MA Script -> S-Story -> Story
  -> Export.
- **Net** (ingest točka na mreži): uvoz i AI, ne otvara formu.

Forme su iste u obje grane; razlikuje se samo redoslijed i trenutak otvaranja. Tko što otvara
zapisano je u bazi (korak priče), ne u memoriji traya.

## 2. Hijerarhija: kocka -> modul -> panel -> forma (ZAKLJUČANO)

- **Kocka** (`kind: block`): jedna radnja, jedna odgovornost, napisana jednom. Ovisi samo o
  kockama. Bez sučelja. Nema sat osim ako ugovor kaže `polls`.
- **Modul** (`kind: module`): slaže kocke u jednu cjelinu rada (npr. source timeline), drži
  njihovo stanje kad ga mora biti. Ovisi o kockama i modulima.
- **Panel** (`kind: panel`): crta jedan modul na jednom mjestu ploče i vraća radnje kataloga.
  Ovisi o kockama, modulima i panelima.
- **Forma** (`kind: form`): samo raspored: koji panel na koje mjesto, s kojim mjerama i
  tekstovima. Nema `if` nad podacima. Aplikacija (`apps/*`) je samo pokretanje forme.
- **Orkestrator** (`kind: orchestrator`): prima radnju i predaje je modulu koji je vlasnik.
  **Ne odlučuje.** Svaki `if`/`match` nad stanjem u orkestratoru je greška; pravilo ide u
  modul kojem stanje pripada. Ovisi o kockama i modulima, nikad o panelima i formama.

Svaka kocka/modul/panel ima ugovor `contracts/modules/<ime>.module.json` s poljima:
`kind`, `responsibility` (jedna rečenica), `owns` (stanje i tablice kojima je vlasnik),
`writes` (tablice koje piše), `database` (`owner` | `reader` | `none`), `polls` (sekunde ili
0), `launches` (smije pokretati programe), `paths` (smije graditi putanje postavki).
Conformance (`tools/qnc-conformance`) provjerava ugovor prema kodu i ovisnostima. Prolazi
prije svakog commita.

## 3. Pravila koja audit nije smio dopustiti da padnu

1. **Jedno stanje, jedan vlasnik.** Nijedan podatak nema kopiju u drugoj kocki ni zastavicu
   koja ga "prati". Tko ga treba, pita vlasnika.
2. **Jedno pravilo, jedno mjesto.** Prag glasa, "gotovo", "poznata kartica", pokretanje
   programa, log, tema, put do baze: svako postoji u točno jednoj kocki. Conformance prijavljuje
   iste nizove koda u dvije kocke.
3. **Baza je jedina istina.** Nema JSON datoteka stanja (način rada, izbor orkestratora, koraci).
   Postavke računala su jedna kocka s ugovorom `paths`; svi ostali je pitaju. Nema privremenih
   datoteka koje nitko ne čita. Nema stanja u memoriji procesa koje odlučuje što se briše.
4. **Jedna tablica, jedan pisac.** Svaki upis ide kroz vlasnika tablice; sirovo otvaranje baze
   izvan vlasnika je greška conformancea. Test-support kod nikad nije produkcijska ovisnost.
5. **Jedan sat po potrebi.** Kocka koja čita bazu u ritmu to zapisuje u ugovor (`polls`); forma
   ne smije imati više od jednog čitača iste tablice.
6. **Svaka radnja ima šifru u katalogu** (`contracts/keyboard-shortcuts.json`); tipka, miš,
   touchpad i gumb šalju istu šifru. Pravilo unosa: tipka radi odmah, Ctrl+ uzima, Shift+
   dodaje, Alt+ skače, Enter potvrda, Esc odustani, Delete briše uzeto; desni klik = Ctrl+.
7. **Timeline i zoom su jedno** (QNC 0.12): zoom je uvećani dio timelinea; iste radnje, isti
   kod, vlasnik daje samo os.
8. **Svaki natpis ima šifru u rječniku** (`contracts/text/<jezik>.json`); engleski je osnovni.
9. **Samo Rust.** Bez Pythona ni skripti. Izmjene alatom za uređivanje.
10. **Testovi nikad ne pišu u stvarni projekt; kartica je samo za čitanje.**
11. **Commit po koraku, push po bloku rada, oznaka `v-GGGG-MM-DD` na kraju dana,
    `docs/razvoj/<dan>.md`.** Force push samo uz izričitu dozvolu.
13. **GUI je odvojen od koda (ZAKLJUČANO).** Kocka, modul i orkestrator nemaju nijednu GUI
    biblioteku (ni kao ovisnost). Panel i forma crtaju ono što im modul da i vraćaju šifre
    radnji: bez baze, datoteka, niti, procesa, sata i vlastitog stanja. Pravilo koje treba
    stanje ide u modul; prikaz koji treba podatak pita modul. Conformance C11.
12. **Zlatne tablice.** Prije promjene ponašanja postoji test koji zaključava postojeće
    ponašanje (tipke, miš, slike zooma); promjena retka traži korisnikovu odluku.

## 4. Prijemni popis za kocku iz QNC-shared / QNC-AI

Kocka se prenosi (ne kopira u cjelini) tek kad za nju vrijedi sve:

- [ ] jedna odgovornost koja stane u jednu rečenicu ugovora;
- [ ] nema stanja koje je kopija tuđeg (uzeto/odabrano/aktivno samo kod vlasnika);
- [ ] nema pravila koje postoji i drugdje (provjeriti audit §E i §5);
- [ ] bazu otvara samo kroz vlasnika tablice ili je sama vlasnik (ugovor `database`);
- [ ] nema `Command::new` osim u kocki za pokretanje programa;
- [ ] nema vlastitog `fn log`, boja, gumba, polja: koristi kocke dijagnostike, teme, ui-kita;
- [ ] nema `(user 2026-..)` iznimaka u kodu: pravilo je zapisano u ugovoru, ne u komentaru;
- [ ] radni redci <= 600, datoteka <= 400 (conformance C2);
- [ ] testovi prolaze bez stvarnog projekta.

Kocka koja ne prolazi rastavlja se prije prijenosa; rastavljanje se radi ovdje, u QNC-pro.

## 5. Redoslijed prijenosa (iz audita, §H i §7)

1. Temelj: dijagnostika, tekst, tema, ui-kit, katalog radnji, postavke računala (jedna kocka),
   pokretanje programa (jedna kocka), posrednik baze.
2. Uzeti IN/OUT s jednim vlasnikom; source timeline kao modul; zoom kao modul izvan priče.
3. Jedan prag glasa; jedan čitač vala po formi; slova kod vlasnika.
4. Preview playera rastavljen (sesija, program, Sync, normalizacija, čekanje zapisa).
5. Tray i uvoz: kocke za diskove, karticu, napredak, eject; tray i uvoz bez pravila; način
   rada i korak priče u bazi.
6. Project rastavljen (zaštita, defaulti, templatei, mape); jedna tema; jedan boilerplate forme.
7. Script i priča rastavljeni; jedan undo; jedan automat za slip/trim/level/assist.
8. Forme redom obje grane: AI Media Assist, Ingest, MA Script, S-Story, Story, Export.

## 6. Odluke korisnika

- 2026-11-20: QNC-pro je čisti početak po auditu (ne kopija QNC-AI-a); kocke ulaze jedna po
  jedna kad prođu pravilo. Prva radi grana Breaking news od traya; Story grana ide od traya kroz
  Ingest.
