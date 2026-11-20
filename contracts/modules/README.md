# Ugovori kocki

Svaki `crates/qnc-<ime>` ima `contracts/modules/<ime>.module.json`. Conformance ga provjerava
prema kodu i ovisnostima (AGENTS §2, §3).

```json
{
  "kind": "block",
  "responsibility": "Jedna rečenica: što ova kocka radi i ništa drugo.",
  "owns": ["stanje ili tablice kojima je jedini vlasnik"],
  "writes": ["tablice koje piše (samo database: owner)"],
  "database": "none | reader | owner",
  "polls": 0,
  "launches": false,
  "paths": false
}
```

- `kind`: `block` (ovisi samo o kockama, ne crta, bez GUI biblioteke), `module` (slaže kocke), `kit` (crtaći pribor bez stanja), `panel` (crta jedan
  modul, vraća radnje), `form` (samo raspored), `orchestrator` (usmjerava, ne odlučuje).
- `polls`: sekunde između dva čitanja kad kocka čita bazu u ritmu; 0 znači da nema sat.
- `launches`: smije pokretati programe (`Command::new`); samo jedna kocka u repou.
- `paths`: smije graditi putanje postavki i kataloga; samo jedna kocka u repou.
